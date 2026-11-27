//! API axum de GMFlow et frontend statique.

pub mod acces;
pub mod erreur;
pub mod etat;
pub mod freinage;
pub mod mdp;
pub mod premier_compte;
pub mod routes;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use gmflow_core::config::Config;
use tower_sessions_sqlx_store::PostgresStore;

pub use etat::{DureesSession, Etat};

pub async fn lancer(config: Config) -> Result<()> {
    let pool = gmflow_core::db::connecter(&config.database_url).await?;
    gmflow_core::db::initialiser(&pool).await?;
    let revus = gmflow_core::depot::mettre_a_jour_controles(&pool).await?;
    if revus > 0 {
        tracing::info!(revus, "phrases des contrôles mises à jour");
    }

    let store = PostgresStore::new(pool.clone());
    tokio::spawn({
        use tower_sessions::session_store::ExpiredDeletion;
        let store = store.clone();
        async move {
            if let Err(e) = store
                .continuously_delete_expired(tokio::time::Duration::from_secs(3600))
                .await
            {
                tracing::error!(erreur = %e, "purge des sessions arrêtée");
            }
        }
    });

    let etat = Etat::new(pool, config, DureesSession::default());
    premier_compte::annoncer_si_besoin(&etat).await?;

    let web = std::env::var("GMFLOW_WEB").unwrap_or_else(|_| "/srv/web".into());
    let app = routes::routeur(etat, store, Some(web.into()));
    let adresse = SocketAddr::from(([0, 0, 0, 0], port_ecoute()?));
    tracing::info!(%adresse, "GMFlow app démarrée");
    let ecoute = tokio::net::TcpListener::bind(adresse).await?;
    // Adresse de la connexion : sert au freinage des essais de connexion (`freinage`).
    axum::serve(
        ecoute,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Port d'écoute de l'app : `GMFLOW_PORT`, 8347 par défaut (8080 est souvent déjà pris).
fn port_ecoute() -> Result<u16> {
    match std::env::var("GMFLOW_PORT") {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse()
            .with_context(|| format!("GMFLOW_PORT invalide : {v:?}")),
        _ => Ok(8347),
    }
}
