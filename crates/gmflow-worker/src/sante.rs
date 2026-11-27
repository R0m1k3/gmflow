//! Port interne 8081 du worker (jamais publié) : `/health` (base joignable, battement du
//! planificateur de moins de 10 minutes et chacune des six files qui bat depuis moins de
//! 5 minutes) et `/metrics` (format Prometheus, `gmflow_core::sante::metriques`).
//!
//! Garde-fou : un worker bloqué ne se relève pas seul, et Docker ne relance jamais un
//! conteneur « unhealthy ». [`surveiller`] tourne sur son propre fil (indépendant du moteur
//! asynchrone du worker, qui peut être celui qui est bloqué) et arrête le processus avec un
//! code d'erreur quand la santé reste mauvaise `GMFLOW_WORKER_ARRET_APRES_MIN` minutes
//! (15 par défaut) : `restart: unless-stopped` le relance.

use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use sqlx::PgPool;

pub use gmflow_core::sante::{BATTEMENT_FILE_MAX, BATTEMENT_MAX};

/// Santé mauvaise pendant ce temps : le worker s'arrête pour être relancé.
pub const ARRET_APRES: Duration = Duration::from_secs(15 * 60);
/// Intervalle entre deux contrôles du garde-fou.
pub const INTERVALLE_GARDE: Duration = Duration::from_secs(60);

pub async fn controle(pool: &PgPool) -> bool {
    gmflow_core::sante::worker_vivant(pool).await
}

async fn health(State(pool): State<PgPool>) -> (StatusCode, &'static str) {
    if controle(&pool).await {
        (StatusCode::OK, "ok")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "base injoignable, planificateur ou file arrêté",
        )
    }
}

async fn metrics(State(pool): State<PgPool>) -> axum::response::Response {
    match gmflow_core::sante::metriques(&pool).await {
        Ok(t) => (
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            t,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(erreur = %e, "métriques indisponibles");
            (StatusCode::SERVICE_UNAVAILABLE, "base injoignable").into_response()
        }
    }
}

pub fn routeur(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .with_state(pool)
}

pub async fn serveur(pool: PgPool) {
    match tokio::net::TcpListener::bind(("0.0.0.0", 8081)).await {
        Ok(l) => {
            if let Err(e) = axum::serve(l, routeur(pool)).await {
                tracing::error!(erreur = %e, "serveur /health arrêté");
            }
        }
        Err(e) => tracing::error!(erreur = %e, "port 8081 indisponible"),
    }
}

/// Décide de l'arrêt à partir des contrôles successifs.
#[derive(Debug)]
pub struct Garde {
    arret_apres: Duration,
    mauvais_depuis: Option<Instant>,
}

impl Garde {
    pub fn new(arret_apres: Duration) -> Self {
        Self {
            arret_apres,
            mauvais_depuis: None,
        }
    }

    /// Note un contrôle ; vrai s'il faut arrêter le worker (santé mauvaise depuis assez longtemps).
    pub fn observer(&mut self, ok: bool, maintenant: Instant) -> bool {
        if ok {
            self.mauvais_depuis = None;
            return false;
        }
        let depuis = *self.mauvais_depuis.get_or_insert(maintenant);
        maintenant.duration_since(depuis) >= self.arret_apres
    }
}

/// `GMFLOW_WORKER_ARRET_APRES_MIN` (minutes, 0 : garde-fou coupé), sinon `ARRET_APRES`.
pub fn arret_apres_depuis_env() -> Option<Duration> {
    match std::env::var("GMFLOW_WORKER_ARRET_APRES_MIN")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(0) => None,
        Some(m) => Some(Duration::from_secs(m * 60)),
        None => Some(ARRET_APRES),
    }
}

/// Lance le garde-fou sur un fil à part, avec sa propre connexion à la base.
pub fn surveiller(database_url: String) {
    let Some(arret_apres) = arret_apres_depuis_env() else {
        tracing::info!("garde-fou du worker coupé (GMFLOW_WORKER_ARRET_APRES_MIN=0)");
        return;
    };
    let lancement = std::thread::Builder::new()
        .name("garde-sante".into())
        .spawn(move || {
            let moteur = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!(erreur = %e, "garde-fou du worker non lancé");
                    return;
                }
            };
            moteur.block_on(async move {
                let mut garde = Garde::new(arret_apres);
                loop {
                    tokio::time::sleep(INTERVALLE_GARDE).await;
                    let ok = match tokio::time::timeout(Duration::from_secs(30), async {
                        let pool = sqlx::postgres::PgPoolOptions::new()
                            .max_connections(1)
                            .acquire_timeout(Duration::from_secs(10))
                            .connect(&database_url)
                            .await
                            .ok()?;
                        let ok = controle(&pool).await;
                        pool.close().await;
                        Some(ok)
                    })
                    .await
                    {
                        Ok(Some(ok)) => ok,
                        _ => false,
                    };
                    if garde.observer(ok, Instant::now()) {
                        tracing::error!(
                            minutes = arret_apres.as_secs() / 60,
                            "worker bloqué (planificateur ou file arrêtés) : arrêt pour être relancé par Docker"
                        );
                        std::process::exit(3);
                    }
                }
            });
        });
    if let Err(e) = lancement {
        tracing::error!(erreur = %e, "garde-fou du worker non lancé");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arret_seulement_apres_une_panne_durable() {
        let t0 = Instant::now();
        let mut g = Garde::new(Duration::from_secs(900));
        assert!(!g.observer(false, t0));
        assert!(!g.observer(false, t0 + Duration::from_secs(600)));
        // Un contrôle bon remet le compte à zéro.
        assert!(!g.observer(true, t0 + Duration::from_secs(700)));
        assert!(!g.observer(false, t0 + Duration::from_secs(800)));
        assert!(!g.observer(false, t0 + Duration::from_secs(1600)));
        assert!(g.observer(false, t0 + Duration::from_secs(1700)));
    }
}
