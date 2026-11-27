pub mod a_traiter;
pub mod admin_boites;
pub mod admin_claude;
pub mod admin_expedition;
pub mod admin_journal;
pub mod admin_logiflow;
pub mod admin_reinitialiser;
pub mod admin_serveur_api;
pub mod admin_societes;
pub mod admin_traitements;
pub mod admin_utilisateurs;
pub mod auth;
pub mod bilan;
pub mod depots;
pub mod documents;
pub mod export_phenix;
pub mod fichiers;
pub mod fournisseurs;
pub mod iban;
pub mod lignes;
pub mod lot;
pub mod mails;
pub mod mise_en_route;
pub mod qualite;
pub mod rapprochement;
pub mod recherche;
pub mod reponses;
pub mod retour;
pub mod tableau;

use std::path::PathBuf;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
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

/// CSP de toute réponse qui n'a pas la sienne (pages du front, JSON). Modeste à dessein :
/// l'`index.html` de SvelteKit démarre par un script en ligne et l'interface pose des styles en
/// ligne, et les aperçus (PDF en `iframe`, mail HTML) ont leur propre CSP plus stricte.
pub const CSP_PAR_DEFAUT: &str = "frame-ancestors 'self'; base-uri 'self'; form-action 'self'";

/// En-têtes de sécurité, sans écraser ceux posés par une route.
async fn entetes_securite(req: Request, next: Next) -> Response {
    use axum::http::{HeaderValue, header};
    let mut r = next.run(req).await;
    let h = r.headers_mut();
    for (nom, valeur) in [
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "same-origin"),
        (header::X_FRAME_OPTIONS, "SAMEORIGIN"),
        (header::CONTENT_SECURITY_POLICY, CSP_PAR_DEFAUT),
    ] {
        if !h.contains_key(&nom) {
            h.insert(nom, HeaderValue::from_static(valeur));
        }
    }
    r
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
        .route("/moi/mot-de-passe", post(auth::changer_mot_de_passe))
        .route("/premier-compte/{jeton}", get(premier_compte::verifier))
        .route("/premier-compte", post(premier_compte::creer))
        .route("/societes", get(mails::societes))
        .route("/boites", get(mails::boites))
        .route("/mails", get(mails::lister))
        .route("/mails/{id}", get(mails::fiche))
        .route("/mails/{id}/html", get(mails::html))
        .route(
            "/mails/{id}/hors-societe",
            post(mails::masquer).delete(mails::demasquer),
        )
        .route("/mails/{id}/categorie", put(reponses::changer_categorie))
        .route("/mails/{id}/statut-reponse", post(reponses::changer_statut))
        .route("/mails/{id}/brouillons", post(reponses::demander_brouillon))
        .route("/brouillons/{id}", put(reponses::modifier_brouillon))
        .route(
            "/brouillons/{id}/abandonner",
            post(reponses::abandonner_brouillon),
        )
        .route(
            "/brouillons/{id}/envoyer",
            post(reponses::envoyer_brouillon),
        )
        .route("/a-repondre", get(reponses::a_repondre))
        .route("/a-repondre/compte", get(reponses::compte))
        .route("/tri/semaines", get(reponses::semaines))
        .route(
            "/admin/expediteurs-marques",
            get(reponses::marques).post(reponses::marquer),
        )
        .route(
            "/admin/expediteurs-marques/{id}",
            delete(reponses::supprimer_marque),
        )
        .route("/pieces/{id}", get(fichiers::piece))
        .route("/pieces/{id}/tableau", get(fichiers::tableau))
        .route("/tableau-de-bord", get(tableau::tableau))
        .route("/documents", get(documents::lister))
        .route("/documents/bilan", get(documents::bilan))
        .route("/documents/{id}/verifier", post(documents::verifier))
        .route(
            "/documents/{id}",
            get(documents::fiche).put(documents::corriger),
        )
        .route("/documents/{id}/controler", post(documents::controler))
        .route("/documents/{id}/deposer", post(documents::deposer))
        .route("/documents/{id}/valider", post(documents::valider))
        .route("/documents/{id}/ecarter", post(documents::ecarter))
        .route("/documents/{id}/logiflow", post(admin_logiflow::relancer))
        // Retour en arrière et validation par lot (routes/retour.rs, routes/lot.rs).
        .route("/documents/annuler-envoi", post(retour::annuler_envoi))
        .route("/documents/{id}/remettre", post(retour::remettre))
        .route("/documents/lot/eligibilite", post(lot::eligibilite))
        .route("/documents/lot/valider", post(lot::valider))
        // Lignes articles corrigées à la main (routes/lignes.rs).
        .route(
            "/documents/{id}/lignes",
            get(lignes::liste).post(lignes::ajouter),
        )
        .route(
            "/documents/{id}/lignes/{ligne}",
            put(lignes::modifier).delete(lignes::supprimer),
        )
        .route("/a-traiter", get(a_traiter::file))
        .route("/recherche", get(recherche::chercher))
        .route("/a-traiter/compteurs", get(a_traiter::compteurs))
        .route("/depots/suivi", get(depots::suivi))
        .route("/bilan", get(bilan::bilan))
        .route(
            "/rapprochement",
            get(rapprochement::vue).post(rapprochement::lier),
        )
        .route("/rapprochement/{id}", delete(rapprochement::delier))
        .route("/documents/{id}/rapprochement", get(rapprochement::detail))
        .route("/documents/{id}/export-phenix", get(export_phenix::etat))
        .route(
            "/documents/{id}/export-phenix/verifier",
            post(export_phenix::verifier),
        )
        .route(
            "/documents/{id}/export-phenix/matrice",
            get(export_phenix::matrice),
        )
        .route(
            "/documents/{id}/export-phenix/trame",
            get(export_phenix::trame),
        )
        .route(
            "/documents/{id}/rapprochement-ignore",
            post(rapprochement::ignorer),
        )
        .route(
            "/admin/rapprochement/relire-bl",
            post(rapprochement::relire_bl),
        )
        .route("/fournisseurs", get(fournisseurs::lister))
        .route("/fournisseurs/{id}", get(fournisseurs::fiche))
        .route("/fournisseurs/{id}/fil", get(fournisseurs::fil))
        .route("/fournisseurs/{id}/marques", get(fournisseurs::marques))
        .route("/fournisseurs/{id}/ibans", get(iban::lister))
        .route(
            "/admin/fournisseurs/{id}/ibans/{iban_id}",
            delete(iban::retirer),
        )
        .route("/admin/qualite", get(qualite::qualite))
        .route(
            "/admin/documents/{id}/iban/confirmer",
            post(iban::confirmer),
        )
        .route(
            "/admin/fournisseurs/{id}/repartir",
            post(fournisseurs::repartir),
        )
        .route("/admin/fournisseurs", post(fournisseurs::creer))
        .route("/admin/fournisseurs/doublons", get(fournisseurs::doublons))
        .route(
            "/admin/fournisseurs/doublons/distinct",
            post(fournisseurs::distinct),
        )
        .route(
            "/admin/fournisseurs/{id}",
            put(fournisseurs::modifier).patch(fournisseurs::modifier_partiel),
        )
        .route(
            "/admin/fournisseurs/{id}/fusionner",
            post(fournisseurs::fusionner),
        )
        .route(
            "/admin/expedition",
            get(admin_expedition::lire).put(admin_expedition::modifier),
        )
        .route("/admin/expedition/test", post(admin_expedition::tester))
        .route(
            "/admin/serveur-api",
            get(admin_serveur_api::lire)
                .put(admin_serveur_api::modifier)
                .delete(admin_serveur_api::supprimer),
        )
        .route("/admin/serveur-api/test", post(admin_serveur_api::tester))
        .route(
            "/admin/serveur-api/synchro",
            post(admin_serveur_api::synchroniser),
        )
        .route(
            "/admin/logiflow",
            get(admin_logiflow::lire)
                .put(admin_logiflow::modifier)
                .delete(admin_logiflow::supprimer),
        )
        .route("/admin/logiflow/test", post(admin_logiflow::tester))
        .route("/admin/logiflow/suivi", post(admin_logiflow::suivre))
        .route(
            "/admin/fournisseurs/{id}/codes-magasin",
            get(admin_serveur_api::codes),
        )
        .route("/admin/documents/{id}/relire", post(documents::relire))
        .route(
            "/admin/claude/reglages",
            get(admin_claude::reglages).put(admin_claude::modifier_reglages),
        )
        .route("/admin/claude/appels", get(admin_claude::appels))
        .route(
            "/admin/boites",
            get(admin_boites::lister).post(admin_boites::creer),
        )
        .route(
            "/admin/boites/{id}",
            get(admin_boites::fiche).put(admin_boites::modifier),
        )
        .route("/admin/boites/tester", post(mise_en_route::tester))
        .route("/admin/boites/essais/{id}", get(mise_en_route::essai))
        .route(
            "/admin/boites/{id}/tester-smtp",
            post(mise_en_route::tester_smtp),
        )
        .route("/admin/mise-en-route", get(mise_en_route::etat))
        .route("/admin/boites/{id}/tester", post(admin_boites::tester))
        .route("/admin/boites/{id}/activer", post(admin_boites::activer))
        .route("/admin/boites/{id}/pause", post(admin_boites::pause))
        .route("/admin/boites/{id}/relever", post(admin_boites::relever))
        .route("/admin/boites/{id}/compteurs", get(admin_boites::compteurs))
        .route("/admin/journal", get(admin_journal::lister))
        .route(
            "/admin/reinitialiser",
            post(admin_reinitialiser::reinitialiser),
        )
        .route("/admin/traitements", get(admin_traitements::lister))
        .route(
            "/admin/traitements/tout-relire",
            post(admin_traitements::tout_relire),
        )
        .route(
            "/admin/traitements/{etape}/relancer",
            post(admin_traitements::relancer),
        )
        .route("/admin/claude", get(admin_claude::etat))
        .route(
            "/admin/claude/jeton",
            put(admin_claude::coller).delete(admin_claude::supprimer),
        )
        .route("/admin/claude/test", post(admin_claude::tester))
        .route(
            "/admin/claude/cle-api",
            put(admin_claude::enregistrer_cle).delete(admin_claude::supprimer_cle),
        )
        .route("/admin/claude/acces", put(admin_claude::choisir_acces))
        .route("/admin/claude/seances", post(admin_claude::nouvelle_seance))
        .route(
            "/admin/claude/seances/en-cours",
            get(admin_claude::seance_en_cours),
        )
        .route("/admin/claude/seances/{id}", get(admin_claude::seance))
        .route(
            "/admin/claude/seances/{id}/annuler",
            post(admin_claude::annuler_seance),
        )
        .route("/admin/claude/seances/{id}/code", post(admin_claude::code))
        .route(
            "/admin/societes",
            get(admin_societes::lister).post(admin_societes::creer),
        )
        .route("/admin/societes/{id}", put(admin_societes::modifier))
        .route(
            "/admin/utilisateurs",
            get(admin_utilisateurs::lister).post(admin_utilisateurs::creer),
        )
        .route(
            "/admin/utilisateurs/{id}",
            put(admin_utilisateurs::modifier),
        )
        .fallback(api_inconnue)
        .layer(middleware::from_fn(exiger_entete))
        .layer(sessions);

    let mut app = Router::new().nest("/api", api).route("/health", get(sante));
    if let Some(web) = web {
        let index = web.join("index.html");
        app = app.fallback_service(ServeDir::new(web).fallback(ServeFile::new(index)));
    }
    app.layer(middleware::from_fn(entetes_securite))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(etat)
}
