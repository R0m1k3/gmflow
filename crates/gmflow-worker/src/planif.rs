//! Toutes les 5 minutes (heure ronde, UTC) : battement du worker, une relève par boîte active
//! et non en pause, les pompes de lecture et de tri, la tâche d'alertes et, toutes les 12 h, la
//! copie des fournisseurs du magasin, toutes les heures la vérification Logiflow ; une fois par
//! jour, la purge des publicités de plus de 90 jours et des tâches terminées de plus de 7 jours.
//! Hors des files : la surveillance des relèves arrêtées (`envoi::releves_muettes`).

use std::time::Duration;

use gmflow_core::{file, parametre};
use serde_json::json;
use sqlx::PgPool;

pub const PERIODE: Duration = Duration::from_secs(300);
/// Délai après le démarrage avant de surveiller les relèves arrêtées.
const SURVEILLANCE_APRES: Duration = Duration::from_secs(15 * 60);

pub async fn tour(pool: &PgPool) -> anyhow::Result<usize> {
    let quand =
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    parametre::ecrire(
        pool,
        parametre::WORKER_BATTEMENT,
        &json!({ "quand": quand }),
    )
    .await?;
    let boites = sqlx::query_scalar!(
        "select id from boite_mail where active and not pause_auto order by id"
    )
    .fetch_all(pool)
    .await?;
    for id in &boites {
        file::pousser_releve(pool, *id).await?;
    }
    crate::lecture::pompe_apres_releve(pool).await;
    crate::tri::pompe_apres_releve(pool).await;
    file::pousser_envoi(pool, file::TacheEnvoi::Alertes).await?;
    // Validations dont la tâche d'envoi s'est perdue (worker arrêté pendant le délai).
    gmflow_core::envoi_prevu::relancer_en_retard(pool).await?;
    if crate::serveur_api::synchro_due(pool).await? {
        file::pousser_envoi(pool, file::TacheEnvoi::SynchroMagasin).await?;
    }
    if crate::logiflow::suivi_du(pool).await? {
        file::pousser_envoi(pool, file::TacheEnvoi::SuiviLogiflow).await?;
    }
    Ok(boites.len())
}

fn jusqu_au_prochain_tour() -> Duration {
    let s = time::OffsetDateTime::now_utc()
        .unix_timestamp()
        .rem_euclid(PERIODE.as_secs() as i64) as u64;
    Duration::from_secs(PERIODE.as_secs() - s)
}

pub async fn boucle(ctx: crate::Contexte) {
    let (pool, fichiers) = (ctx.pool.clone(), ctx.config.fichiers.clone());
    let demarre = std::time::Instant::now();
    // Battement immédiat : /health est vert dès le démarrage.
    let quand = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let _ = parametre::ecrire(
        &pool,
        parametre::WORKER_BATTEMENT,
        &json!({ "quand": quand }),
    )
    .await;
    loop {
        tokio::time::sleep(jusqu_au_prochain_tour()).await;
        match tour(&pool).await {
            Ok(n) => tracing::info!(boites = n, "relèves planifiées"),
            Err(e) => tracing::error!(erreur = ?e, "planification impossible"),
        }
        // Juste après un démarrage, les relèves n'ont pas encore eu le temps de passer.
        if demarre.elapsed() >= SURVEILLANCE_APRES
            && let Err(e) = crate::envoi::releves_muettes(&ctx).await
        {
            tracing::error!(erreur = ?e, "surveillance des relèves impossible");
        }
        if let Err(e) = crate::purge::si_due(&pool, &fichiers).await {
            tracing::error!(erreur = ?e, "purge impossible");
        }
    }
}
