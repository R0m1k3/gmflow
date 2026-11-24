//! Envois depuis l'adresse d'expédition : dépôt d'une facture dans Dext ou Yooz (une facture
//! par mail, le fichier d'origine en pièce jointe), test de l'expédition et alertes.

use gmflow_core::expedition::{self, PieceJointe};
use gmflow_core::{depot, eml, file, journal, parametre, stockage};
use serde_json::json;
use sqlx::PgPool;

use crate::Contexte;

/// Un envoi resté « en cours » plus longtemps a été interrompu (worker arrêté) : on ne sait
/// pas s'il est parti.
const ENVOI_BLOQUE_MINUTES: i32 = 15;

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Envoie une alerte aux destinataires configurés. Rend faux si rien n'est configuré.
pub async fn alerter(ctx: &Contexte, objet: &str, corps: &str) -> anyhow::Result<bool> {
    let a = expedition::destinataires_alertes(&ctx.pool).await?;
    if a.is_empty() {
        return Ok(false);
    }
    let Some((e, mdp)) = expedition::lire(&ctx.pool, &ctx.chiffreur).await? else {
        return Ok(false);
    };
    expedition::envoyer(
        &e,
        mdp.as_deref(),
        &a,
        &format!("[GMFlow] {objet}"),
        corps,
        None,
    )
    .await?;
    Ok(true)
}

async fn alerter_sans_echec(ctx: &Contexte, objet: &str, corps: &str) {
    if let Err(e) = alerter(ctx, objet, corps).await {
        tracing::warn!(erreur = ?e, objet, "alerte non envoyée");
    }
}

/// Envoi en échec : ligne `envoi_depot` et statut du document.
async fn noter_echec(
    pool: &PgPool,
    envoi_id: i64,
    document_id: i64,
    message: &str,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "update envoi_depot set etat = 'erreur', message = $2 where id = $1",
        envoi_id,
        message
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update document set statut_depot = 'erreur' where id = $1",
        document_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn deposer(
    ctx: &Contexte,
    document_id: i64,
    forcer: bool,
    auteur_id: Option<i64>,
) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    // Doublon revérifié juste avant l'envoi (la file `envoi` traite une tâche à la fois).
    let doublon = if forcer {
        None
    } else {
        depot::doublon_de(pool, document_id).await?
    };
    let mut tx = pool.begin().await?;
    let Some(d) = sqlx::query!(
        r#"select d.statut_depot, d.valide_le is not null as "valide!", d.societe_id, d.type,
                  d.numero, d.date_document, d.montant_ttc::float8 as ttc, d.fournisseur_nom,
                  f.nom as "fiche?", s.adresse_depot as "adresse?", s.outil_depot as "outil?",
                  coalesce(s.depot_auto, false) as "auto!", s.raison_sociale as "societe?",
                  p.nom, p.type_mime, p.rang, m.chemin_eml
           from document d
           join piece_jointe p on p.id = d.piece_jointe_id
           join mail m on m.id = d.mail_id
           left join societe s on s.id = d.societe_id
           left join fournisseur f on f.id = d.fournisseur_id
           where d.id = $1
           for update of d"#,
        document_id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(());
    };
    let statut = d.statut_depot.as_deref().unwrap_or("");
    let autorise = if d.valide {
        matches!(statut, "a_verifier" | "pret" | "erreur") || (statut == "doublon" && forcer)
    } else {
        statut == "pret" && d.auto
    };
    if !autorise {
        tracing::info!(
            document = document_id,
            statut,
            "dépôt ignoré (statut ou validation)"
        );
        return Ok(());
    }
    if let Some((autre, _)) = doublon {
        let echec = json!([{ "code": "doublon", "message": format!("Même facture que le document {autre}") }]);
        sqlx::query!(
            "update document set statut_depot = 'doublon', controles = $2 || controles where id = $1",
            document_id,
            echec
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(());
    }
    let (Some(societe_id), Some(adresse)) = (
        d.societe_id,
        d.adresse.clone().filter(|a| !a.trim().is_empty()),
    ) else {
        sqlx::query!(
            "update document set statut_depot = 'a_verifier',
                controles = controles || '[{\"code\": \"depot\", \"message\": \"Société ou adresse Dext/Yooz manquante\", \"champs\": [\"societe_id\"]}]'::jsonb
             where id = $1",
            document_id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(());
    };
    let envoi_id = sqlx::query_scalar!(
        "insert into envoi_depot (document_id, societe_id, adresse, outil, auteur_id)
         values ($1, $2, $3, $4, $5) returning id",
        document_id,
        societe_id,
        adresse,
        d.outil,
        auteur_id
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "update document set statut_depot = 'en_envoi' where id = $1",
        document_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let resultat = async {
        let Some((e, mdp)) = expedition::lire(pool, &ctx.chiffreur).await? else {
            anyhow::bail!("adresse d'expédition non configurée (Administration > Expédition)");
        };
        let brut =
            tokio::fs::read(stockage::chemin_absolu(&ctx.config.fichiers, &d.chemin_eml)).await?;
        let rang = d.rang;
        let contenu =
            tokio::task::spawn_blocking(move || eml::extraire_piece(&brut, rang)).await??;
        let genre = if d.r#type.as_deref() == Some("avoir") {
            "Avoir"
        } else {
            "Facture"
        };
        let fournisseur = d
            .fiche
            .clone()
            .or_else(|| d.fournisseur_nom.clone())
            .unwrap_or_else(|| "fournisseur inconnu".into());
        let numero = d.numero.clone().unwrap_or_default();
        let objet = format!("{genre} {fournisseur} {numero}").trim().to_string();
        let corps = format!(
            "{genre} déposée par GMFlow.\n\nStructure : {}\nFournisseur : {fournisseur}\nNuméro : {numero}\nDate : {}\nMontant TTC : {}\nFichier : {}\n",
            d.societe.clone().unwrap_or_default(),
            d.date_document.map(|x| x.to_string()).unwrap_or_default(),
            d.ttc.map(|x| format!("{x:.2} €")).unwrap_or_default(),
            d.nom
        );
        expedition::envoyer(
            &e,
            mdp.as_deref(),
            std::slice::from_ref(&adresse),
            &objet,
            &corps,
            Some(PieceJointe {
                nom: &d.nom,
                type_mime: &d.type_mime,
                contenu,
            }),
        )
        .await
    }
    .await;

    match resultat {
        Ok(message_id) => {
            let mut tx = pool.begin().await?;
            sqlx::query!(
                "update envoi_depot set etat = 'envoye', message_id_smtp = $2 where id = $1",
                envoi_id,
                message_id
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                "update document set statut_depot = 'depose', controles = '[]' where id = $1",
                document_id
            )
            .execute(&mut *tx)
            .await?;
            journal::ecrire(
                &mut *tx,
                auteur_id,
                "depot",
                "document",
                Some(document_id.to_string()),
                json!({ "adresse": adresse, "outil": d.outil, "message_id": message_id, "force": forcer }),
            )
            .await?;
            tx.commit().await?;
            // Facture envoyée : ses livraisons dans Logiflow (si configuré) reçoivent la facture.
            if d.r#type.as_deref() == Some("facture")
                && parametre::lire(pool, gmflow_core::logiflow::CLE)
                    .await?
                    .is_some()
            {
                file::pousser_envoi(pool, file::TacheEnvoi::RapprocherLogiflow { document_id })
                    .await?;
            }
        }
        Err(e) => {
            let message = format!("{e:#}");
            noter_echec(pool, envoi_id, document_id, &message).await?;
            journal::ecrire(
                pool,
                auteur_id,
                "depot_echec",
                "document",
                Some(document_id.to_string()),
                json!({ "adresse": adresse, "message": message }),
            )
            .await?;
            alerter_sans_echec(
                ctx,
                "dépôt en échec",
                &format!(
                    "Le dépôt du document {document_id} ({}) vers {adresse} a échoué :\n{message}\n\nLe document est en erreur dans GMFlow ; un administrateur peut le redéposer.",
                    d.nom
                ),
            )
            .await;
        }
    }
    Ok(())
}

pub async fn tester_expedition(ctx: &Contexte, destinataire: &str) -> anyhow::Result<()> {
    let resultat = async {
        let Some((e, mdp)) = expedition::lire(&ctx.pool, &ctx.chiffreur).await? else {
            anyhow::bail!("adresse d'expédition non configurée");
        };
        expedition::envoyer(
            &e,
            mdp.as_deref(),
            &[destinataire.to_string()],
            "[GMFlow] test de l'adresse d'expédition",
            "Ce mail de test confirme que GMFlow peut envoyer des mails (dépôts Dext ou Yooz, alertes).",
            None,
        )
        .await
    }
    .await;
    let (ok, message) = match resultat {
        Ok(_) => (true, format!("mail de test envoyé à {destinataire}")),
        Err(e) => (false, format!("{e:#}")),
    };
    parametre::ecrire(
        &ctx.pool,
        expedition::CLE_TEST,
        &json!({ "quand": maintenant(), "ok": ok, "message": message }),
    )
    .await?;
    Ok(())
}

/// Envois restés « en cours » (worker arrêté pendant l'envoi) : passés en erreur, jamais
/// renvoyés automatiquement. Rend les documents concernés.
pub async fn envois_interrompus(pool: &PgPool) -> anyhow::Result<Vec<i64>> {
    let ids = sqlx::query_scalar!(
        "update envoi_depot set etat = 'erreur',
            message = 'envoi interrompu : état inconnu, vérifier dans Dext ou Yooz avant de redéposer'
         where etat = 'en_cours' and quand < now() - make_interval(mins => $1)
         returning document_id",
        ENVOI_BLOQUE_MINUTES
    )
    .fetch_all(pool)
    .await?;
    if !ids.is_empty() {
        sqlx::query!(
            "update document set statut_depot = 'erreur' where id = any($1) and statut_depot = 'en_envoi'",
            &ids
        )
        .execute(pool)
        .await?;
    }
    Ok(ids)
}

/// Tâche `Alertes` (à chaque tour du planificateur) : envois interrompus, boîtes en échec
/// depuis plus de 30 min (une alerte par incident), boîtes rétablies.
pub async fn alertes(ctx: &Contexte) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let interrompus = envois_interrompus(pool).await?;
    if !interrompus.is_empty() {
        alerter_sans_echec(
            ctx,
            "dépôt interrompu",
            &format!(
                "L'envoi des documents {interrompus:?} a été interrompu. Vérifiez dans Dext ou Yooz s'ils sont arrivés avant de les redéposer."
            ),
        )
        .await;
    }
    if expedition::destinataires_alertes(pool).await?.is_empty()
        || expedition::lire(pool, &ctx.chiffreur).await?.is_none()
    {
        return Ok(());
    }
    let en_echec = sqlx::query!(
        "select id, libelle, derniere_erreur, erreur_depuis, pause_auto from boite_mail
         where erreur_depuis < now() - interval '30 minutes' and alerte_le is null
         order by id"
    )
    .fetch_all(pool)
    .await?;
    if !en_echec.is_empty() {
        let mut corps =
            String::from("La relève de ces boîtes échoue depuis plus de 30 minutes :\n\n");
        for b in &en_echec {
            corps.push_str(&format!(
                "- {} : {}{}\n",
                b.libelle,
                b.derniere_erreur.as_deref().unwrap_or("erreur inconnue"),
                if b.pause_auto {
                    " (en pause automatique après 3 refus de connexion)"
                } else {
                    ""
                }
            ));
        }
        corps.push_str("\nUn mail de suivi partira quand la relève fonctionnera de nouveau.");
        if alerter(ctx, "relève IMAP en échec", &corps).await? {
            let ids: Vec<i64> = en_echec.iter().map(|b| b.id).collect();
            sqlx::query!(
                "update boite_mail set alerte_le = now() where id = any($1)",
                &ids
            )
            .execute(pool)
            .await?;
        }
    }
    let retablies = sqlx::query!(
        "select id, libelle from boite_mail where alerte_le is not null and erreur_depuis is null order by id"
    )
    .fetch_all(pool)
    .await?;
    if !retablies.is_empty() {
        let noms: Vec<&str> = retablies.iter().map(|b| b.libelle.as_str()).collect();
        if alerter(
            ctx,
            "relève IMAP rétablie",
            &format!(
                "La relève fonctionne de nouveau pour : {}.",
                noms.join(", ")
            ),
        )
        .await?
        {
            let ids: Vec<i64> = retablies.iter().map(|b| b.id).collect();
            sqlx::query!(
                "update boite_mail set alerte_le = null where id = any($1)",
                &ids
            )
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}
