//! Relève d'une boîte : bail en base, puis pour chaque dossier surveillé, lecture des nouveaux
//! messages par paquets de 50 (sans les marquer lus), une transaction par paquet.

use std::collections::HashSet;

use futures::TryStreamExt;
use gmflow_core::eml::{self, MailDecoupe};
use gmflow_core::imap::{self, ErreurImap, Session};
use gmflow_core::{journal, stockage};
use serde_json::json;
use sqlx::{PgConnection, PgPool};

use crate::Contexte;

pub const PAQUET: usize = 50;
pub const PAUSE_APRES: i32 = 3;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    /// Faux si une autre relève tenait déjà le bail : rien n'a été fait.
    pub bail_obtenu: bool,
    /// Messages lus sur le serveur (FETCH).
    pub messages_lus: usize,
    /// Nouvelles lignes `mail`.
    pub nouveaux_mails: usize,
}

struct Boite {
    id: i64,
    serveur: String,
    port: i32,
    identifiant: String,
    mot_de_passe_chiffre: Vec<u8>,
    dossiers: Vec<String>,
    reprise_mois: i32,
}

/// Prend le bail (15 min) si personne ne le tient. Ne relève qu'une boîte active et non en pause.
async fn prendre_bail(pool: &PgPool, id: i64) -> sqlx::Result<Option<Boite>> {
    sqlx::query_as!(
        Boite,
        "update boite_mail set releve_jusqu_a = now() + interval '15 minutes'
         where id = $1 and active and not pause_auto
           and (releve_jusqu_a is null or releve_jusqu_a < now())
         returning id, serveur, port, identifiant, mot_de_passe_chiffre, dossiers, reprise_mois",
        id
    )
    .fetch_optional(pool)
    .await
}

async fn prolonger_bail(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "update boite_mail set releve_jusqu_a = now() + interval '15 minutes' where id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn relever(ctx: &Contexte, boite_id: i64) -> anyhow::Result<Bilan> {
    let Some(boite) = prendre_bail(&ctx.pool, boite_id).await? else {
        tracing::info!(
            boite_id,
            "relève ignorée : bail déjà pris, boîte inactive ou en pause"
        );
        return Ok(Bilan::default());
    };
    let mut bilan = Bilan {
        bail_obtenu: true,
        ..Default::default()
    };
    let resultat = relever_avec_bail(ctx, &boite, &mut bilan).await;

    match &resultat {
        Ok(()) => {
            sqlx::query!(
                "update boite_mail set derniere_releve = now(), derniere_erreur = null, erreur_depuis = null,
                     echecs_auth = 0, releve_jusqu_a = null
                 where id = $1",
                boite.id
            )
            .execute(&ctx.pool)
            .await?;
        }
        Err(e) => {
            let refus = matches!(e, ErreurImap::RefusAuth(_));
            let message: String = e.to_string().chars().take(500).collect();
            let pause = sqlx::query_scalar!(
                "update boite_mail set derniere_erreur = $2, erreur_depuis = coalesce(erreur_depuis, now()),
                     echecs_auth = case when $3 then echecs_auth + 1 else echecs_auth end,
                     pause_auto = pause_auto or ($3 and echecs_auth + 1 >= $4),
                     releve_jusqu_a = null
                 where id = $1
                 returning pause_auto",
                boite.id,
                message,
                refus,
                PAUSE_APRES
            )
            .fetch_one(&ctx.pool)
            .await?;
            if pause {
                journal::ecrire(
                    &ctx.pool,
                    None,
                    "pause_auto",
                    "boite_mail",
                    Some(boite.id.to_string()),
                    json!({ "message": message }),
                )
                .await?;
            }
            tracing::warn!(boite_id, erreur = %message, "relève en échec");
        }
    }
    resultat?;
    Ok(bilan)
}

async fn relever_avec_bail(
    ctx: &Contexte,
    boite: &Boite,
    bilan: &mut Bilan,
) -> Result<(), ErreurImap> {
    let mdp = ctx
        .chiffreur
        .dechiffrer_texte(&boite.mot_de_passe_chiffre)
        .map_err(|e| ErreurImap::Autre(e.to_string()))?;
    let port = u16::try_from(boite.port).map_err(|_| ErreurImap::Autre("port invalide".into()))?;
    let mut session = imap::connecter(
        &boite.serveur,
        port,
        &boite.identifiant,
        &mdp,
        ctx.transport,
    )
    .await?;

    let mut erreurs = Vec::new();
    for dossier in &boite.dossiers {
        if let Err(e) = relever_dossier(ctx, boite, &mut session, dossier, bilan).await {
            erreurs.push(format!("{dossier} : {e}"));
        }
    }
    let _ = session.logout().await;
    if erreurs.is_empty() {
        Ok(())
    } else {
        Err(ErreurImap::Autre(erreurs.join(" ; ")))
    }
}

fn erreur_base(e: impl std::fmt::Display) -> ErreurImap {
    ErreurImap::Autre(format!("base : {e}"))
}

/// Date IMAP (`SINCE`) : aujourd'hui moins `mois` mois.
pub fn date_reprise(mois: i32) -> String {
    let d = time::OffsetDateTime::now_utc().date();
    let total = d.year() * 12 + (d.month() as i32 - 1) - mois;
    let (annee, mois_num) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let mois_t = time::Month::try_from(mois_num as u8).expect("mois valide");
    let jour = d.day().min(28);
    let date = time::Date::from_calendar_date(annee, mois_t, jour).expect("date valide");
    const NOMS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}-{}-{}",
        date.day(),
        NOMS[mois_num as usize - 1],
        date.year()
    )
}

async fn relever_dossier(
    ctx: &Contexte,
    boite: &Boite,
    session: &mut Session,
    dossier: &str,
    bilan: &mut Bilan,
) -> Result<(), ErreurImap> {
    // EXAMINE : dossier en lecture seule, le serveur ne pose jamais \Seen.
    let infos = session.examine(dossier).await?;
    let uidvalidity = infos.uid_validity.map(i64::from).unwrap_or(0);
    let uid_next = infos.uid_next.map(i64::from);

    let etat = sqlx::query!(
        "select uidvalidity, dernier_uid from boite_dossier where boite_id = $1 and nom = $2",
        boite.id,
        dossier
    )
    .fetch_optional(&ctx.pool)
    .await
    .map_err(erreur_base)?;

    let mut dernier = match etat {
        Some(e) if e.uidvalidity == uidvalidity => e.dernier_uid,
        autre => {
            let mut tx = ctx.pool.begin().await.map_err(erreur_base)?;
            if autre.is_some() {
                // UID tous changés : on garde les liens, sans UID, et on relit la période.
                sqlx::query!(
                    "update mail_boite set uid = null where boite_id = $1 and dossier = $2",
                    boite.id,
                    dossier
                )
                .execute(&mut *tx)
                .await
                .map_err(erreur_base)?;
                journal::ecrire(
                    &mut *tx,
                    None,
                    "uidvalidity_change",
                    "boite_mail",
                    Some(boite.id.to_string()),
                    json!({ "dossier": dossier }),
                )
                .await
                .map_err(erreur_base)?;
            }
            sqlx::query!(
                "insert into boite_dossier (boite_id, nom, uidvalidity, dernier_uid) values ($1, $2, $3, 0)
                 on conflict (boite_id, nom) do update set uidvalidity = excluded.uidvalidity, dernier_uid = 0",
                boite.id,
                dossier,
                uidvalidity
            )
            .execute(&mut *tx)
            .await
            .map_err(erreur_base)?;
            tx.commit().await.map_err(erreur_base)?;
            0
        }
    };

    let requete = if dernier == 0 {
        format!("SINCE {}", date_reprise(boite.reprise_mois))
    } else {
        format!("UID {}:*", dernier + 1)
    };
    let trouves: HashSet<u32> = session.uid_search(&requete).await?;
    let mut uids: Vec<u32> = trouves
        .into_iter()
        .filter(|u| i64::from(*u) > dernier)
        .collect();
    uids.sort_unstable();

    for paquet in uids.chunks(PAQUET) {
        prolonger_bail(&ctx.pool, boite.id)
            .await
            .map_err(erreur_base)?;
        let ensemble = paquet
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let messages: Vec<(u32, Vec<u8>)> = {
            let flux = session.uid_fetch(&ensemble, "(UID BODY.PEEK[])").await?;
            let fetches: Vec<_> = flux.try_collect().await?;
            fetches
                .iter()
                .filter_map(|f| Some((f.uid?, f.body()?.to_vec())))
                .collect()
        };
        bilan.messages_lus += messages.len();
        let max = paquet
            .iter()
            .copied()
            .max()
            .map(i64::from)
            .unwrap_or(dernier);
        let nouveaux = enregistrer_paquet(ctx, boite.id, dossier, &messages, max)
            .await
            .map_err(erreur_base)?;
        bilan.nouveaux_mails += nouveaux;
        dernier = max;
        if nouveaux > 0 {
            crate::lecture::pompe_apres_releve(&ctx.pool).await;
            crate::tri::pompe_apres_releve(&ctx.pool).await;
        }
    }

    // Dossier vide ou sans message dans la période : tout ce qui existe est considéré comme vu.
    if uids.is_empty()
        && let Some(n) = uid_next
        && n - 1 > dernier
    {
        sqlx::query!(
            "update boite_dossier set dernier_uid = $3 where boite_id = $1 and nom = $2",
            boite.id,
            dossier,
            n - 1
        )
        .execute(&ctx.pool)
        .await
        .map_err(erreur_base)?;
    }
    Ok(())
}

fn decoupe_de_secours(brut: &[u8]) -> MailDecoupe {
    MailDecoupe {
        message_id: format!("sha256:{}", hex::encode(eml::sha256(brut))),
        expediteur: String::new(),
        destinataires: Vec::new(),
        objet: "(message illisible)".into(),
        date: None,
        corps_texte: String::new(),
        pieces: Vec::new(),
        empreinte_contenu: eml::sha256(brut),
    }
}

/// Écrit un paquet dans une transaction et avance `dernier_uid`. Rend le nombre de nouveaux mails.
async fn enregistrer_paquet(
    ctx: &Contexte,
    boite_id: i64,
    dossier: &str,
    messages: &[(u32, Vec<u8>)],
    max_uid: i64,
) -> anyhow::Result<usize> {
    let racine = ctx.config.fichiers.clone();
    let mut tx = ctx.pool.begin().await?;
    let mut nouveaux = 0;
    for (uid, brut) in messages {
        let (chemin, sha) = {
            let racine = racine.clone();
            let brut = brut.clone();
            tokio::task::spawn_blocking(move || stockage::ecrire_eml(&racine, &brut)).await??
        };
        let d = eml::decouper(brut).unwrap_or_else(|_| decoupe_de_secours(brut));
        let (mail_id, nouveau) =
            inserer_mail(&mut tx, &d, &chemin, &sha, brut.len() as i64).await?;
        if nouveau {
            nouveaux += 1;
        }
        rattacher(&mut tx, mail_id, boite_id, dossier, i64::from(*uid)).await?;
    }
    sqlx::query!(
        "update boite_dossier set dernier_uid = greatest(dernier_uid, $3) where boite_id = $1 and nom = $2",
        boite_id,
        dossier,
        max_uid
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(nouveaux)
}

async fn inserer_mail(
    tx: &mut PgConnection,
    d: &MailDecoupe,
    chemin: &str,
    sha: &[u8],
    taille: i64,
) -> anyhow::Result<(i64, bool)> {
    let insere = sqlx::query_scalar!(
        "insert into mail (message_id, empreinte_contenu, expediteur, destinataires, objet, date_mail,
                           corps_texte, chemin_eml, sha256, taille)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         on conflict (message_id, empreinte_contenu) do nothing
         returning id",
        d.message_id,
        d.empreinte_contenu,
        d.expediteur,
        &d.destinataires,
        d.objet,
        d.date,
        d.corps_texte.replace('\0', ""),
        chemin,
        sha,
        taille
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = insere else {
        let id = sqlx::query_scalar!(
            "select id from mail where message_id = $1 and empreinte_contenu = $2",
            d.message_id,
            d.empreinte_contenu
        )
        .fetch_one(&mut *tx)
        .await?;
        return Ok((id, false));
    };
    for p in &d.pieces {
        sqlx::query!(
            "insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256) values ($1, $2, $3, $4, $5, $6)",
            id,
            p.rang,
            p.nom,
            p.type_mime,
            p.taille,
            p.sha256
        )
        .execute(&mut *tx)
        .await?;
    }
    let autres = sqlx::query_scalar!(
        "select count(*) as \"n!\" from mail where message_id = $1 and id <> $2",
        d.message_id,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if autres > 0 && !d.message_id.starts_with("sha256:") {
        journal::ecrire(
            &mut *tx,
            None,
            "message_id_reutilise",
            "mail",
            Some(id.to_string()),
            json!({ "message_id": d.message_id }),
        )
        .await?;
    }
    Ok((id, true))
}

/// Lien boîte/dossier/UID. Après un changement d'UIDVALIDITY, la ligne sans UID du même mail
/// reçoit le nouvel UID ; sinon une ligne est ajoutée.
async fn rattacher(
    tx: &mut PgConnection,
    mail_id: i64,
    boite_id: i64,
    dossier: &str,
    uid: i64,
) -> anyhow::Result<()> {
    let maj = sqlx::query!(
        "update mail_boite set uid = $4
         where id = (select id from mail_boite
                     where mail_id = $1 and boite_id = $2 and dossier = $3 and uid is null
                     order by id limit 1)",
        mail_id,
        boite_id,
        dossier,
        uid
    )
    .execute(&mut *tx)
    .await?;
    if maj.rows_affected() == 0 {
        sqlx::query!(
            "insert into mail_boite (mail_id, boite_id, dossier, uid) values ($1, $2, $3, $4)
             on conflict (boite_id, dossier, uid) do nothing",
            mail_id,
            boite_id,
            dossier,
            uid
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}
