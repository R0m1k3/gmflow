//! Surveillance de l'extérieur (constat C25) : sonde publique `/api/sante` (pour Uptime Kuma)
//! et métriques Prometheus `/api/metriques` (jeton `GMFLOW_METRIQUES_JETON` ou session admin).

use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::acces::AccesAdmin;
use crate::erreur::ErreurApi;
use crate::etat::Etat;

/// `GET /api/sante` : `{"etat": "ok"}` (200) ou `{"etat": "degrade", "causes": [...]}` (503)
/// quand la base ne répond pas, que le worker ne donne plus signe de vie ou qu'une relève est
/// en retard. Public, sans nom ni chiffre.
pub async fn sonde(State(etat): State<Etat>) -> Response {
    let s = gmflow_core::sante::sonde(&etat.pool).await;
    let code = if s.ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, [(header::CACHE_CONTROL, "no-store")], Json(s)).into_response()
}

fn empreinte(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// Jeton `Authorization: Bearer …` égal à `GMFLOW_METRIQUES_JETON` (comparaison des empreintes,
/// à temps constant pour la longueur).
fn jeton_valable(entetes: &HeaderMap, attendu: Option<&str>) -> bool {
    let Some(attendu) = attendu else {
        return false;
    };
    let Some(donne) = entetes
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    let (a, b) = (empreinte(donne.trim()), empreinte(attendu));
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Accès aux métriques : le bon jeton, sinon une session d'administrateur.
pub struct AccesMetriques;

impl FromRequestParts<Etat> for AccesMetriques {
    type Rejection = ErreurApi;

    async fn from_request_parts(parts: &mut Parts, etat: &Etat) -> Result<Self, Self::Rejection> {
        if jeton_valable(&parts.headers, etat.jeton_metriques.as_deref()) {
            return Ok(AccesMetriques);
        }
        AccesAdmin::from_request_parts(parts, etat).await?;
        Ok(AccesMetriques)
    }
}

/// `GET /api/metriques` : métriques au format Prometheus.
pub async fn metriques(_a: AccesMetriques, State(etat): State<Etat>) -> Response {
    match gmflow_core::sante::metriques(&etat.pool).await {
        Ok(t) => (
            [
                (
                    header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                ),
                (header::CACHE_CONTROL, "no-store"),
            ],
            t,
        )
            .into_response(),
        Err(e) => ErreurApi::Interne(e.into()).into_response(),
    }
}
