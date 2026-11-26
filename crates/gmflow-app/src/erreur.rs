use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ErreurApi {
    #[error("{0}")]
    Donnees(String),
    /// Donnée refusée, avec le champ en faute (`{"erreur": "...", "champ": "siret"}`) : le
    /// formulaire affiche le message sous ce champ.
    #[error("{message}")]
    Champ {
        champ: &'static str,
        message: String,
    },
    #[error("non connecté")]
    NonConnecte,
    #[error("accès refusé")]
    Interdit,
    #[error("introuvable")]
    Absent,
    #[error("{0}")]
    Conflit(String),
    /// Conflit avec un code stable pour l'interface (`{"erreur": "...", "code": "doublon"}`).
    #[error("{message}")]
    ConflitCode { code: &'static str, message: String },
    #[error("trop d'essais, réessayez dans 15 minutes")]
    TropDEssais,
    #[error("erreur interne")]
    Interne(#[from] anyhow::Error),
}

pub type Resultat<T> = Result<T, ErreurApi>;

impl From<sqlx::Error> for ErreurApi {
    /// `RowNotFound` (un `fetch_one` sans ligne) reste un 404 pour l'utilisateur, mais il est
    /// écrit dans les logs avec l'endroit du `?` : une requête qui devait toujours rendre une
    /// ligne (bug) ne passe plus inaperçue derrière un « introuvable ».
    #[track_caller]
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::RowNotFound = e {
            let ou = std::panic::Location::caller();
            tracing::warn!(
                fichier = ou.file(),
                ligne = ou.line(),
                "aucune ligne (fetch_one) : réponse 404"
            );
            return ErreurApi::Absent;
        }
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return ErreurApi::Conflit("cet élément existe déjà".into());
            }
            if db.is_foreign_key_violation() {
                return ErreurApi::Conflit("élément encore utilisé ou référence inconnue".into());
            }
            if db.is_check_violation() {
                return ErreurApi::Donnees("valeur refusée".into());
            }
        }
        ErreurApi::Interne(e.into())
    }
}

impl From<tower_sessions::session::Error> for ErreurApi {
    fn from(e: tower_sessions::session::Error) -> Self {
        ErreurApi::Interne(anyhow::anyhow!("session : {e}"))
    }
}

impl IntoResponse for ErreurApi {
    fn into_response(self) -> Response {
        let code = match &self {
            ErreurApi::Donnees(_) | ErreurApi::Champ { .. } => StatusCode::BAD_REQUEST,
            ErreurApi::NonConnecte => StatusCode::UNAUTHORIZED,
            ErreurApi::Interdit => StatusCode::FORBIDDEN,
            ErreurApi::Absent => StatusCode::NOT_FOUND,
            ErreurApi::Conflit(_) | ErreurApi::ConflitCode { .. } => StatusCode::CONFLICT,
            ErreurApi::TropDEssais => StatusCode::TOO_MANY_REQUESTS,
            ErreurApi::Interne(e) => {
                tracing::error!(erreur = ?e, "erreur interne");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let corps = match &self {
            ErreurApi::ConflitCode { code, message } => json!({ "erreur": message, "code": code }),
            ErreurApi::Champ { champ, message } => json!({ "erreur": message, "champ": champ }),
            _ => json!({ "erreur": self.to_string() }),
        };
        (code, Json(corps)).into_response()
    }
}
