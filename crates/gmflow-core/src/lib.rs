//! Code partagé par l'app et le worker : configuration, base, chiffrement,
//! file de tâches, lecture des `.eml`, connexion IMAP et appels à Claude CLI.

pub mod claude;
pub mod config;
pub mod crypto;
pub mod db;
pub mod eml;
pub mod file;
pub mod imap;
pub mod journal;
pub mod parametre;
pub mod stockage;
#[cfg(feature = "test-utils")]
pub mod test_utils;

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
