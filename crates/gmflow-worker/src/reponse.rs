//! Envoi d'une réponse depuis la boîte d'origine (tâche `EnvoyerReponse`). La tâche n'existe que
//! si un utilisateur a cliqué « Envoyer » : l'interface met le brouillon à l'état `envoi` et la
//! pousse. Jamais rejouée : en cas d'échec, le brouillon revient à l'état `modifie` avec le
//! message, et seul un nouveau clic le renvoie. Juste avant le SMTP, l'envoi est réservé
//! (`envoi_pris_le`) : une tâche rejouée (worker tué, file relancée) trouve la réservation et ne
//! renvoie rien ; le brouillon passe alors en « envoi interrompu, état inconnu » au bout de 15 min.

use gmflow_core::expedition::{self, Expedition};
use gmflow_core::reponse::{self, Original};
use gmflow_core::{imap, journal, stockage, tri};
use serde_json::json;
use sqlx::PgPool;

use crate::Contexte;

/// Un envoi ou une génération resté plus longtemps en cours a été interrompu.
const BLOQUE_MINUTES: i32 = 15;

struct AEnvoyer {
    mail_id: i64,
    texte: String,
    message_id: String,
    expediteur: String,
    objet: String,
    date_mail: Option<time::OffsetDateTime>,
    corps_texte: String,
    chemin_eml: String,
    raison_sociale: String,
    identifiant: String,
    serveur: String,
    port: i32,
    mot_de_passe_chiffre: Vec<u8>,
    smtp_serveur: Option<String>,
    smtp_port: Option<i32>,
    smtp_securite: Option<String>,
    dossier_envoyes: Option<String>,
}

async fn echec(
    pool: &PgPool,
    id: i64,
    mail_id: i64,
    auteur_id: Option<i64>,
    message: &str,
) -> anyhow::Result<()> {
    sqlx::query!(
        "update brouillon_reponse set statut = 'modifie', erreur = $2, modifie_le = now(),
             envoi_pris_le = null
         where id = $1 and statut = 'envoi'",
        id,
        message
    )
    .execute(pool)
    .await?;
    journal::ecrire(
        pool,
        auteur_id,
        "reponse_echec",
        "mail",
        Some(mail_id.to_string()),
        json!({ "brouillon_id": id, "message": message }),
    )
    .await?;
    Ok(())
}

pub async fn envoyer(
    ctx: &Contexte,
    brouillon_id: i64,
    auteur_id: Option<i64>,
) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some(e) = sqlx::query_as!(
        AEnvoyer,
        "select b.mail_id, b.texte, m.message_id, m.expediteur, m.objet, m.date_mail, m.corps_texte,
                m.chemin_eml, s.raison_sociale, bm.identifiant, bm.serveur, bm.port,
                bm.mot_de_passe_chiffre, bm.smtp_serveur, bm.smtp_port, bm.smtp_securite,
                bm.dossier_envoyes
         from brouillon_reponse b
         join mail m on m.id = b.mail_id
         join boite_mail bm on bm.id = b.boite_id
         join societe s on s.id = bm.societe_id
         where b.id = $1 and b.statut = 'envoi' and b.envoye_le is null and b.envoi_pris_le is null",
        brouillon_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let (Some(serveur), Some(port), Some(securite)) = (
        e.smtp_serveur.clone().filter(|s| !s.trim().is_empty()),
        e.smtp_port,
        e.smtp_securite.clone(),
    ) else {
        return echec(
            pool,
            brouillon_id,
            e.mail_id,
            auteur_id,
            "SMTP de la boîte non configuré (Administration > Boîtes)",
        )
        .await;
    };
    if e.texte.trim().is_empty() {
        return echec(pool, brouillon_id, e.mail_id, auteur_id, "réponse vide").await;
    }
    let mdp = match ctx.chiffreur.dechiffrer_texte(&e.mot_de_passe_chiffre) {
        Ok(m) => m,
        Err(err) => return echec(pool, brouillon_id, e.mail_id, auteur_id, &err.to_string()).await,
    };
    let brut = tokio::fs::read(stockage::chemin_absolu(&ctx.config.fichiers, &e.chemin_eml))
        .await
        .unwrap_or_default();
    let entetes = tri::entetes(&brut);
    let original = Original {
        message_id: &e.message_id,
        references: &entetes.references,
        expediteur: &e.expediteur,
        reply_to: entetes.reply_to.as_deref(),
        objet: &e.objet,
        date: e.date_mail,
        corps: &e.corps_texte,
    };
    let rep =
        match reponse::construire(&e.identifiant, Some(&e.raison_sociale), &original, &e.texte) {
            Ok(r) => r,
            Err(err) => {
                return echec(pool, brouillon_id, e.mail_id, auteur_id, &err.to_string()).await;
            }
        };
    let config = Expedition {
        serveur,
        port: u16::try_from(port).unwrap_or(465),
        securite,
        identifiant: e.identifiant.clone(),
        adresse: e.identifiant.clone(),
        nom: Some(e.raison_sociale.clone()),
    };
    let destinataire = reponse::destinataire(&original).to_string();
    // Réservation : une seule tâche passe ce point pour un même clic.
    let reserve = sqlx::query!(
        "update brouillon_reponse set envoi_pris_le = now()
         where id = $1 and statut = 'envoi' and envoye_le is null and envoi_pris_le is null",
        brouillon_id
    )
    .execute(pool)
    .await?
    .rows_affected();
    if reserve == 0 {
        tracing::warn!(
            brouillon = brouillon_id,
            "réponse déjà prise par une autre tâche : rien n'est renvoyé"
        );
        return Ok(());
    }
    if let Err(err) = expedition::envoyer_message(&config, Some(&mdp), rep.message).await {
        return echec(
            pool,
            brouillon_id,
            e.mail_id,
            auteur_id,
            &format!("envoi SMTP : {err}"),
        )
        .await;
    }
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "update brouillon_reponse set statut = 'envoye', envoye_le = now(), envoye_par = $2,
            message_id_envoye = $3, erreur = null, modifie_le = now()
         where id = $1",
        brouillon_id,
        auteur_id,
        rep.message_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update mail set statut_reponse = 'repondu' where id = $1",
        e.mail_id
    )
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        auteur_id,
        "reponse_envoyee",
        "mail",
        Some(e.mail_id.to_string()),
        json!({ "brouillon_id": brouillon_id, "a": destinataire, "message_id": rep.message_id }),
    )
    .await?;
    tx.commit().await?;

    // Copie dans le dossier des envoyés : la réponse est partie, un échec ici est seulement noté.
    if let Some(dossier) = e.dossier_envoyes.as_deref().filter(|d| !d.is_empty()) {
        let copie = async {
            let port = u16::try_from(e.port)?;
            let mut s =
                imap::connecter(&e.serveur, port, &e.identifiant, &mdp, ctx.transport).await?;
            let r = imap::ajouter(&mut s, dossier, &rep.brut).await;
            imap::deconnecter(&mut s).await;
            anyhow::Ok(r?)
        };
        if let Err(err) = copie.await {
            tracing::warn!(brouillon = brouillon_id, erreur = %err, "copie dans les envoyés impossible");
            sqlx::query!(
                "update brouillon_reponse set erreur = $2 where id = $1",
                brouillon_id,
                format!("réponse envoyée, mais copie dans « {dossier} » impossible : {err}")
            )
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// Envois et générations restés en cours (worker arrêté) : rendus à l'utilisateur avec un
/// message, jamais relancés automatiquement.
pub async fn interrompus(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query!(
        "update brouillon_reponse set statut = 'modifie', modifie_le = now(), envoi_pris_le = null,
            erreur = 'envoi interrompu : état inconnu, vérifiez le dossier des envoyés avant de renvoyer'
         where statut = 'envoi' and modifie_le < now() - make_interval(mins => $1)",
        BLOQUE_MINUTES
    )
    .execute(pool)
    .await?;
    sqlx::query!(
        "update brouillon_reponse set statut = 'propose', modifie_le = now(),
            erreur = 'génération interrompue, relancez-la'
         where statut = 'generation' and modifie_le < now() - make_interval(mins => $1)",
        BLOQUE_MINUTES
    )
    .execute(pool)
    .await?;
    Ok(())
}
