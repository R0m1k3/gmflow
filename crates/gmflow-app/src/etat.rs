use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmflow_core::config::Config;
use gmflow_core::crypto::Chiffreur;
use sqlx::PgPool;

use crate::freinage::{Freinage, ProxysDeConfiance};

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
    /// Échecs de connexion par adresse IP et compte (`freinage`), sans verrou de compte.
    pub freinage: Arc<Freinage>,
    /// Proxys dont les en-têtes `X-Real-IP` / `X-Forwarded-For` sont crus.
    pub proxys: Arc<ProxysDeConfiance>,
    /// Jeton qui ouvre `/api/metriques` sans session (`GMFLOW_METRIQUES_JETON`) ; sans lui,
    /// seules les sessions d'administrateur y ont accès.
    pub jeton_metriques: Option<Arc<str>>,
    pub premier_compte: Arc<Mutex<Option<JetonPremierCompte>>>,
}

impl Etat {
    pub fn new(pool: PgPool, config: Config, durees: DureesSession) -> Self {
        Self {
            pool,
            chiffreur: Chiffreur::new(&config.cle),
            config: Arc::new(config),
            durees,
            freinage: Default::default(),
            proxys: Arc::new(ProxysDeConfiance::depuis_env()),
            jeton_metriques: std::env::var("GMFLOW_METRIQUES_JETON")
                .ok()
                .map(|j| j.trim().to_string())
                .filter(|j| !j.is_empty())
                .map(Into::into),
            premier_compte: Default::default(),
        }
    }
}
