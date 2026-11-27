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
    /// Refus métier avec son propre code pour l'interface (`{"erreur": "...", "code": "doublon"}`).
    #[error("{message}")]
    ConflitCode { code: Refus, message: String },
    #[error("trop d'essais : patientez quelques minutes avant de réessayer")]
    TropDEssais,
    #[error("erreur interne")]
    Interne(#[from] anyhow::Error),
}

pub type Resultat<T> = Result<T, ErreurApi>;

/// Codes des refus métier (409, `ErreurApi::ConflitCode`), chacun traduit par le front
/// (`web/src/lib/erreurs.ts`, vérifié par `erreurs.test.ts` et `chaque_code_a_sa_traduction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, rename_all = "snake_case")
)]
pub enum Refus {
    /// La pièce a des alertes à lire avant d'être validée.
    Alertes,
    /// Fichier découpé en plusieurs pièces : on ne le remet pas en lecture d'un bloc.
    Decoupe,
    /// Pièce déjà envoyée, ou en cours d'envoi.
    DejaEnvoye,
    /// Facture déjà reçue (même fournisseur et numéro).
    Doublon,
    /// L'envoi précédent a échoué : seul un administrateur la renvoie.
    ErreurEnvoi,
    /// Essai de connexion d'une boîte absent, expiré ou fait avec d'autres paramètres.
    Essai,
    /// La pièce n'est plus dans l'état attendu.
    Etat,
    /// Adresse d'expédition vers Dext/Yooz non réglée.
    Expedition,
    LogiflowAbsent,
    /// Magasin sans adresse Dext/Yooz.
    SansAdresse,
    ServeurMagasinAbsent,
    SmtpAbsent,
}

impl Refus {
    pub const TOUS: [Refus; 12] = [
        Refus::Alertes,
        Refus::Decoupe,
        Refus::DejaEnvoye,
        Refus::Doublon,
        Refus::ErreurEnvoi,
        Refus::Essai,
        Refus::Etat,
        Refus::Expedition,
        Refus::LogiflowAbsent,
        Refus::SansAdresse,
        Refus::ServeurMagasinAbsent,
        Refus::SmtpAbsent,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Refus::Alertes => "alertes",
            Refus::Decoupe => "decoupe",
            Refus::DejaEnvoye => "deja_envoye",
            Refus::Doublon => "doublon",
            Refus::ErreurEnvoi => "erreur_envoi",
            Refus::Essai => "essai",
            Refus::Etat => "etat",
            Refus::Expedition => "expedition",
            Refus::LogiflowAbsent => "logiflow_absent",
            Refus::SansAdresse => "sans_adresse",
            Refus::ServeurMagasinAbsent => "serveur_magasin_absent",
            Refus::SmtpAbsent => "smtp_absent",
        }
    }
}

/// Tous les codes d'erreur de l'API (`code` du corps d'erreur), pour le front.
#[cfg(feature = "ts")]
#[derive(ts_rs::TS)]
#[ts(export, rename_all = "snake_case")]
#[allow(dead_code)]
pub enum CodeErreur {
    Donnees,
    Champ,
    NonConnecte,
    Interdit,
    Absent,
    Conflit,
    TropDEssais,
    Interne,
    #[ts(untagged)]
    Refus(Refus),
}

/// Corps d'une réponse d'erreur.
#[cfg(feature = "ts")]
#[derive(ts_rs::TS)]
#[ts(export)]
#[allow(dead_code)]
pub struct CorpsErreur {
    /// Phrase du serveur, en français.
    erreur: String,
    code: CodeErreur,
    /// Champ en faute (`code` = `champ`).
    #[ts(optional)]
    champ: Option<String>,
}

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
            ErreurApi::ConflitCode { code, .. } => code.code(),
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
                    code: Refus::Expedition,
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

    /// Rang de chaque refus : un nouveau refus oblige à le ranger ici, donc dans `Refus::TOUS`.
    fn rang(r: Refus) -> usize {
        match r {
            Refus::Alertes => 0,
            Refus::Decoupe => 1,
            Refus::DejaEnvoye => 2,
            Refus::Doublon => 3,
            Refus::ErreurEnvoi => 4,
            Refus::Essai => 5,
            Refus::Etat => 6,
            Refus::Expedition => 7,
            Refus::LogiflowAbsent => 8,
            Refus::SansAdresse => 9,
            Refus::ServeurMagasinAbsent => 10,
            Refus::SmtpAbsent => 11,
        }
    }

    /// Chaque code que le serveur peut rendre a sa traduction dans `web/src/lib/erreurs.ts`
    /// (clé de `PAR_CODE`) : sinon le front retombe sur un message générique.
    #[test]
    fn chaque_code_a_sa_traduction() {
        for (i, r) in Refus::TOUS.iter().enumerate() {
            assert_eq!(rang(*r), i, "Refus::TOUS dans le désordre ou incomplet");
        }
        let front = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/src/lib/erreurs.ts"
        ))
        .unwrap();
        let par_code = front
            .split_once("const PAR_CODE")
            .expect("PAR_CODE dans erreurs.ts")
            .1;
        let generiques = [
            ErreurApi::Donnees(String::new()),
            ErreurApi::Champ {
                champ: "x",
                message: String::new(),
            },
            ErreurApi::NonConnecte,
            ErreurApi::Interdit,
            ErreurApi::Absent,
            ErreurApi::Conflit(String::new()),
            ErreurApi::TropDEssais,
            ErreurApi::Interne(anyhow::anyhow!("x")),
        ];
        let codes = generiques
            .iter()
            .map(|e| e.code())
            .chain(Refus::TOUS.iter().map(|r| r.code()));
        let manquants: Vec<&str> = codes
            .filter(|c| !par_code.contains(&format!("\n  {c}:")))
            .collect();
        assert!(
            manquants.is_empty(),
            "codes sans traduction dans web/src/lib/erreurs.ts : {manquants:?}"
        );
    }
}
