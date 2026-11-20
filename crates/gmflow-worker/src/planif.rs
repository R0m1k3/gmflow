//! Toutes les 5 minutes (heure ronde, UTC) : battement du worker, puis une relève par boîte
//! active et non en pause.

use std::time::Duration;

use gmflow_core::{file, parametre};
use serde_json::json;
use sqlx::PgPool;

pub const PERIODE: Duration = Duration::from_secs(300);

pub async fn tour(pool: &PgPool) -> anyhow::Result<usize> {
    let quand = time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    parametre::ecrire(pool, parametre::WORKER_BATTEMENT, &json!({ "quand": quand })).await?;
    let boites = sqlx::query_scalar!("select id from boite_mail where active and not pause_auto order by id")
        .fetch_all(pool)
        .await?;
    for id in &boites {
        file::pousser_releve(pool, *id).await?;
    }
    Ok(boites.len())
}

fn jusqu_au_prochain_tour() -> Duration {
    let s = time::OffsetDateTime::now_utc().unix_timestamp().rem_euclid(PERIODE.as_secs() as i64) as u64;
    Duration::from_secs(PERIODE.as_secs() - s)
}

pub async fn boucle(pool: PgPool) {
    // Battement immédiat : /health est vert dès le démarrage.
    let quand = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let _ = parametre::ecrire(&pool, parametre::WORKER_BATTEMENT, &json!({ "quand": quand })).await;
    loop {
        tokio::time::sleep(jusqu_au_prochain_tour()).await;
        match tour(&pool).await {
            Ok(n) => tracing::info!(boites = n, "relèves planifiées"),
            Err(e) => tracing::error!(erreur = ?e, "planification impossible"),
        }
    }
}
