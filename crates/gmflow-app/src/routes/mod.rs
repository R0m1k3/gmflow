pub mod admin_boites;
pub mod admin_journal;
pub mod admin_societes;
pub mod admin_utilisateurs;
pub mod auth;
pub mod fichiers;
pub mod mails;
pub mod tableau;

use std::path::PathBuf;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;

use crate::erreur::ErreurApi;
use crate::etat::Etat;
use crate::premier_compte;

/// Contre le CSRF : toute mutation doit porter `X-GMFlow: 1` (un formulaire d'un autre site
/// ne peut pas poser cet en-tête sans déclencher un contrôle CORS, que l'app refuse).
async fn exiger_entete(req: Request, next: Next) -> Response {
    let lecture = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !lecture && req.headers().get("x-gmflow").and_then(|v| v.to_str().ok()) != Some("1") {
        return ErreurApi::Interdit.into_response();
    }
    next.run(req).await
}

async fn sante(State(etat): State<Etat>) -> impl IntoResponse {
    match sqlx::query("select 1").execute(&etat.pool).await {
        Ok(_) => (StatusCode::OK, "ok"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "base injoignable"),
    }
}

async fn api_inconnue() -> ErreurApi {
    ErreurApi::Absent
}

pub fn routeur(etat: Etat, store: PostgresStore, web: Option<PathBuf>) -> Router {
    let inactivite = time::Duration::seconds(etat.durees.inactivite.as_secs() as i64);
    let sessions = SessionManagerLayer::new(store)
        .with_name("gmflow")
        .with_http_only(true)
        .with_secure(true)
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(inactivite))
        // Chaque requête prolonge la session : l'expiration compte l'inactivité.
        .with_always_save(true);

    let api = Router::new()
        .route("/connexion", post(auth::connexion))
        .route("/deconnexion", post(auth::deconnexion))
        .route("/moi", get(auth::moi))
        .route("/premier-compte/{jeton}", get(premier_compte::verifier))
        .route("/premier-compte", post(premier_compte::creer))
        .route("/societes", get(mails::societes))
        .route("/boites", get(mails::boites))
        .route("/mails", get(mails::lister))
        .route("/mails/{id}", get(mails::fiche))
        .route("/pieces/{id}", get(fichiers::piece))
        .route("/tableau-de-bord", get(tableau::tableau))
        .route("/admin/boites", get(admin_boites::lister).post(admin_boites::creer))
        .route("/admin/boites/{id}", put(admin_boites::modifier))
        .route("/admin/boites/{id}/tester", post(admin_boites::tester))
        .route("/admin/boites/{id}/activer", post(admin_boites::activer))
        .route("/admin/boites/{id}/pause", post(admin_boites::pause))
        .route("/admin/boites/{id}/relever", post(admin_boites::relever))
        .route("/admin/boites/{id}/compteurs", get(admin_boites::compteurs))
        .route("/admin/journal", get(admin_journal::lister))
        .route("/admin/societes", get(admin_societes::lister).post(admin_societes::creer))
        .route("/admin/societes/{id}", put(admin_societes::modifier))
        .route("/admin/utilisateurs", get(admin_utilisateurs::lister).post(admin_utilisateurs::creer))
        .route("/admin/utilisateurs/{id}", put(admin_utilisateurs::modifier))
        .fallback(api_inconnue)
        .layer(middleware::from_fn(exiger_entete))
        .layer(sessions);

    let mut app = Router::new().nest("/api", api).route("/health", get(sante));
    if let Some(web) = web {
        let index = web.join("index.html");
        app = app.fallback_service(ServeDir::new(web).fallback(ServeFile::new(index)));
    }
    app.layer(tower_http::trace::TraceLayer::new_for_http()).with_state(etat)
}
