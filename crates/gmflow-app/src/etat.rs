use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmflow_core::config::Config;
use gmflow_core::crypto::Chiffreur;
use sqlx::PgPool;

/// Durées des sessions : réglables pour les tests.
#[derive(Clone, Copy, Debug)]
pub struct DureesSession {
    pub inactivite: Duration,
    pub absolue: Duration,
}

impl Default for DureesSession {
    fn default() -> Self {
        Self {
            inactivite: Duration::from_secs(8 * 3600),
            absolue: Duration::from_secs(7 * 24 * 3600),
        }
    }
}

/// Lien du premier compte : jeton gardé seulement en mémoire.
pub struct JetonPremierCompte {
    pub jeton: String,
    pub cree: Instant,
}

#[derive(Clone)]
pub struct Etat {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub chiffreur: Chiffreur,
    pub durees: DureesSession,
    /// Échecs de connexion par identifiant (5 en 15 min ⇒ 429).
    pub echecs: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    pub premier_compte: Arc<Mutex<Option<JetonPremierCompte>>>,
}

impl Etat {
    pub fn new(pool: PgPool, config: Config, durees: DureesSession) -> Self {
        Self {
            pool,
            chiffreur: Chiffreur::new(&config.cle),
            config: Arc::new(config),
            durees,
            echecs: Default::default(),
            premier_compte: Default::default(),
        }
    }
}
