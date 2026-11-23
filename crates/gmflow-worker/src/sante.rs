//! `/health` du worker (port 8081) : base joignable, battement du planificateur de moins de
//! 10 minutes et chacune des six files qui bat depuis moins de 5 minutes.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use gmflow_core::{parametre, traitements};
use sqlx::PgPool;

pub const BATTEMENT_MAX: time::Duration = time::Duration::minutes(10);
/// Une file bat toutes les 30 s ; une file relancée attend d'abord la fin des tâches de son
/// incarnation précédente, d'où une marge plus large que pour l'orchestre (3 min).
pub const BATTEMENT_FILE_MAX: time::Duration = time::Duration::minutes(5);

pub async fn controle(pool: &PgPool) -> bool {
    planificateur_vivant(pool).await && files_vivantes(pool).await
}

async fn planificateur_vivant(pool: &PgPool) -> bool {
    let Ok(Some(v)) = parametre::lire(pool, parametre::WORKER_BATTEMENT).await else {
        return false;
    };
    let Some(quand) = v.get("quand").and_then(|q| q.as_str()) else {
        return false;
    };
    match time::OffsetDateTime::parse(quand, &time::format_description::well_known::Rfc3339) {
        Ok(t) => time::OffsetDateTime::now_utc() - t < BATTEMENT_MAX,
        Err(_) => false,
    }
}

async fn files_vivantes(pool: &PgPool) -> bool {
    let Ok(b) = traitements::battements(pool).await else {
        return false;
    };
    let maintenant = time::OffsetDateTime::now_utc();
    traitements::FILES.iter().all(|f| {
        b.get(*f)
            .is_some_and(|t| maintenant - *t < BATTEMENT_FILE_MAX)
    })
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

pub fn routeur(pool: PgPool) -> Router {
    Router::new().route("/health", get(health)).with_state(pool)
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
