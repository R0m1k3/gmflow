//! Conservation : les tâches apalis terminées sont supprimées au bout de 7 jours (la table
//! grossit d'environ 1 000 lignes par jour) ; les mails classés en publicité au bout de 90 jours
//! (cahier des charges, RGPD). Un mail reste s'il porte une facture, un avoir, un BL ou un document passé
//! par le dépôt, ou s'il a un brouillon de réponse. Le `.eml` n'est effacé que si plus aucun
//! mail ne le désigne.

use std::path::Path;

use gmflow_core::{journal, parametre, stockage};
use serde_json::json;
use sqlx::PgPool;

pub const PUBLICITES_JOURS: i32 = 90;
pub const TACHES_JOURS: i32 = 7;
const CLE: &str = "purge_le";

/// Supprime les publicités de plus de `jours` jours. Rend le nombre de mails supprimés.
pub async fn publicites(pool: &PgPool, racine: &Path, jours: i32) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    let chemins = sqlx::query_scalar!(
        "delete from mail m
         where m.categorie = 'publicite'
           and coalesce(m.date_mail, m.recu_le) < now() - make_interval(days => $1)
           and m.recu_le < now() - make_interval(days => $1)
           and not exists (
               select 1 from document d
               where d.mail_id = m.id
                 and (d.type in ('facture', 'avoir', 'bl') or d.statut_depot is not null))
           and not exists (select 1 from brouillon_reponse b where b.mail_id = m.id)
         returning m.chemin_eml",
        jours
    )
    .fetch_all(&mut *tx)
    .await?;
    let n = chemins.len() as u64;
    if n > 0 {
        journal::ecrire(
            &mut *tx,
            None,
            "purge_publicites",
            "mail",
            None,
            json!({ "supprimes": n, "jours": jours }),
        )
        .await?;
    }
    tx.commit().await?;
    let mut fichiers = 0;
    for chemin in chemins {
        let encore = sqlx::query_scalar!(
            r#"select exists (select 1 from mail where chemin_eml = $1) as "e!""#,
            chemin
        )
        .fetch_one(pool)
        .await?;
        if encore {
            continue;
        }
        match tokio::fs::remove_file(stockage::chemin_absolu(racine, &chemin)).await {
            Ok(()) => fichiers += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(chemin, erreur = %e, "suppression du .eml impossible"),
        }
    }
    if n > 0 {
        tracing::info!(mails = n, fichiers, "publicités purgées");
    }
    Ok(n)
}

/// Supprime les tâches apalis finies depuis plus de `jours` jours : réussies (`Done`), abandonnées
/// (`Killed`) ou en échec sans essai restant. Une tâche en attente, en cours ou qui sera
/// réessayée n'est jamais touchée. Rend le nombre de tâches supprimées.
pub async fn taches_terminees(pool: &PgPool, jours: i32) -> anyhow::Result<u64> {
    let n = sqlx::query!(
        "delete from apalis.jobs
         where done_at < now() - make_interval(days => $1)
           and (status in ('Done', 'Killed') or (status = 'Failed' and attempts >= max_attempts))",
        jours
    )
    .execute(pool)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::info!(taches = n, "tâches terminées purgées");
    }
    Ok(n)
}

/// Lance les purges si la dernière date de plus de 24 h.
pub async fn si_due(pool: &PgPool, racine: &Path) -> anyhow::Result<()> {
    let due = sqlx::query_scalar!(
        r#"select not exists (
              select 1 from parametre
              where cle = $1 and (valeur->>'quand')::timestamptz > now() - interval '24 hours'
           ) as "d!""#,
        CLE
    )
    .fetch_one(pool)
    .await?;
    if !due {
        return Ok(());
    }
    // Une purge en échec n'empêche pas l'autre.
    let taches = taches_terminees(pool, TACHES_JOURS).await;
    publicites(pool, racine, PUBLICITES_JOURS).await?;
    taches?;
    let quand =
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    parametre::ecrire(pool, CLE, &json!({ "quand": quand })).await?;
    Ok(())
}
