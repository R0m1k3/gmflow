//! Code partagé par l'app et le worker : configuration, base, chiffrement,
//! file de tâches, lecture des `.eml`, connexion IMAP et appels à Claude CLI.

pub mod attente;
pub mod claude;
pub mod config;
pub mod controles;
pub mod crypto;
pub mod db;
pub mod depot;
pub mod eml;
pub mod essai_boite;
pub mod expedition;
pub mod file;
#[cfg(feature = "test-utils")]
pub mod fixtures;
pub mod fournisseurs;
pub mod imap;
pub mod journal;
pub mod lecture;
pub mod parametre;
pub mod rapprochement;
pub mod reglages;
pub mod reinitialisation;
pub mod reponse;
pub mod serveur_api;
pub mod stockage;
#[cfg(feature = "test-utils")]
pub mod test_utils;
pub mod traitements;
pub mod tri;

/// Logs JSON sur la sortie standard (consultables dans Portainer).
pub fn initialiser_logs() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}
