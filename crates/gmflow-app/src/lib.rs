//! API axum de GMFlow et frontend statique.

pub mod acces;
pub mod erreur;
pub mod etat;
pub mod mdp;
pub mod premier_compte;
pub mod routes;

use std::net::SocketAddr;

use anyhow::Result;
use gmflow_core::config::Config;
use tower_sessions_sqlx_store::PostgresStore;

pub use etat::{DureesSession, Etat};

pub async fn lancer(config: Config) -> Result<()> {
    let pool = gmflow_core::db::connecter(&config.database_url).await?;
    gmflow_core::db::initialiser(&pool).await?;

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
    let adresse = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!(%adresse, "GMFlow app démarrée");
    let ecoute = tokio::net::TcpListener::bind(adresse).await?;
    axum::serve(ecoute, app).await?;
    Ok(())
}
