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

impl ErreurApi {
    /// Code stable de l'erreur, toujours présent dans la réponse (`{"erreur", "code"}`) : le
    /// front traduit ce code (`web/src/lib/erreurs.ts`), jamais la phrase. Les refus métier
    /// (`ConflitCode`) ont leur propre code (`doublon`, `expedition`…).
    pub fn code(&self) -> &'static str {
        match self {
            ErreurApi::Donnees(_) => "donnees",
            ErreurApi::Champ { .. } => "champ",
            ErreurApi::NonConnecte => "non_connecte",
            ErreurApi::Interdit => "interdit",
            ErreurApi::Absent => "absent",
            ErreurApi::Conflit(_) => "conflit",
            ErreurApi::ConflitCode { code, .. } => code,
            ErreurApi::TropDEssais => "trop_d_essais",
            ErreurApi::Interne(_) => "interne",
        }
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
            ErreurApi::Champ { champ, message } => {
                json!({ "erreur": message, "code": self.code(), "champ": champ })
            }
            _ => json!({ "erreur": self.to_string(), "code": self.code() }),
        };
        (code, Json(corps)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn corps(e: ErreurApi) -> (StatusCode, serde_json::Value) {
        let r = e.into_response();
        let statut = r.status();
        let octets = axum::body::to_bytes(r.into_body(), 1 << 16).await.unwrap();
        (statut, serde_json::from_slice(&octets).unwrap())
    }

    #[tokio::test]
    async fn chaque_erreur_a_un_code_stable() {
        let cas = [
            (ErreurApi::Donnees("x".into()), 400, "donnees"),
            (ErreurApi::NonConnecte, 401, "non_connecte"),
            (ErreurApi::Interdit, 403, "interdit"),
            (ErreurApi::Absent, 404, "absent"),
            (ErreurApi::Conflit("x".into()), 409, "conflit"),
            (ErreurApi::TropDEssais, 429, "trop_d_essais"),
            (
                ErreurApi::Interne(anyhow::anyhow!("secret")),
                500,
                "interne",
            ),
            (
                ErreurApi::ConflitCode {
                    code: "expedition",
                    message: "x".into(),
                },
                409,
                "expedition",
            ),
        ];
        for (e, statut, code) in cas {
            let (s, j) = corps(e).await;
            assert_eq!(s.as_u16(), statut);
            assert_eq!(j["code"], code, "{j}");
            assert!(j["erreur"].is_string());
        }
        let (_, j) = corps(ErreurApi::Champ {
            champ: "siret",
            message: "SIRET invalide".into(),
        })
        .await;
        assert_eq!(
            j,
            json!({"erreur": "SIRET invalide", "code": "champ", "champ": "siret"})
        );
        // Le détail d'une erreur interne ne sort jamais.
        let (_, j) = corps(ErreurApi::Interne(anyhow::anyhow!("mot de passe"))).await;
        assert_eq!(j["erreur"], "erreur interne");
    }
}
