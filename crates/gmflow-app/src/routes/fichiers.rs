//! Téléchargement d'une pièce jointe, extraite du `.eml` et vérifiée par son SHA-256.

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
use crate::routes::mails::mail_visible;

fn type_sur(t: &str) -> String {
    let ok = !t.is_empty()
        && t.len() < 100
        && t.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/.+-_".contains(&c));
    if ok {
        t.to_string()
    } else {
        "application/octet-stream".into()
    }
}

/// Types affichés dans la page (aperçu de la fiche document) avec `?inline=1`. Jamais de HTML
/// ni de SVG (scripts possibles) : ceux-là restent en téléchargement.
const TYPES_APERCU: &[&str] = &[
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
];

/// CSP de la réponse d'aperçu : rien d'autre que le document lui-même, affichable seulement
/// dans une page de GMFlow. Le PDF n'est pas en `sandbox` : les lecteurs PDF de Chrome et de
/// Firefox refusent de s'ouvrir dans un document isolé.
const CSP_PDF: &str = "default-src 'none'; object-src 'self'; img-src 'self' data:; style-src 'unsafe-inline'; frame-ancestors 'self'";
const CSP_IMAGE: &str = "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'self'; sandbox";
/// Téléchargement : rien ne doit s'exécuter si le navigateur l'ouvrait quand même.
const CSP_TELECHARGEMENT: &str = "default-src 'none'; frame-ancestors 'none'; sandbox";

#[derive(Deserialize)]
pub struct Options {
    inline: Option<String>,
}

/// `Content-Disposition` avec un nom ASCII de secours et le nom UTF-8 (RFC 5987).
fn disposition(nom: &str) -> String {
    disposition_avec("attachment", nom)
}

fn disposition_avec(mode: &str, nom: &str) -> String {
    let ascii: String = nom
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encode = String::new();
    for b in nom.bytes() {
        if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
            encode.push(b as char);
        } else {
            encode.push_str(&format!("%{b:02X}"));
        }
    }
    format!("{mode}; filename=\"{ascii}\"; filename*=UTF-8''{encode}")
}

/// Pièce jointe : téléchargement, ou aperçu dans la page avec `?inline=1` (PDF et images
/// seulement ; tout autre type reste en téléchargement).
pub async fn piece(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Query(o): Query<Options>,
) -> Resultat<Response> {
    let p = sqlx::query!(
        "select p.mail_id, p.rang, p.nom, p.type_mime, p.sha256, m.chemin_eml
         from piece_jointe p join mail m on m.id = p.mail_id where p.id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    if !mail_visible(&etat, &acces, p.mail_id).await? {
        return Err(ErreurApi::Absent);
    }
    let chemin = gmflow_core::stockage::chemin_absolu(&etat.config.fichiers, &p.chemin_eml);
    let rang = p.rang;
    let octets = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let brut = std::fs::read(&chemin)?;
        Ok(gmflow_core::eml::extraire_piece(&brut, rang)?)
    })
    .await
    .map_err(|e| ErreurApi::Interne(e.into()))??;
    if gmflow_core::eml::sha256(&octets) != p.sha256 {
        gmflow_core::journal::ecrire(
            &etat.pool,
            Some(acces.utilisateur_id),
            "piece_alteree",
            "piece_jointe",
            Some(id.to_string()),
            json!({ "mail_id": p.mail_id }),
        )
        .await?;
        return Err(ErreurApi::Interne(anyhow::anyhow!(
            "SHA-256 de la pièce {id} différent de l'original"
        )));
    }
    let type_mime = type_sur(&p.type_mime.to_ascii_lowercase());
    let apercu = matches!(o.inline.as_deref(), Some("1" | "true" | "oui"))
        && TYPES_APERCU.contains(&type_mime.as_str());
    let (disp, csp) = if apercu {
        let csp = if type_mime == "application/pdf" {
            CSP_PDF
        } else {
            CSP_IMAGE
        };
        (disposition_avec("inline", &p.nom), csp)
    } else {
        (disposition(&p.nom), CSP_TELECHARGEMENT)
    };
    Ok((
        [
            (header::CONTENT_TYPE, type_mime),
            (header::CONTENT_DISPOSITION, disp),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::CONTENT_SECURITY_POLICY, csp.to_string()),
            (
                header::HeaderName::from_static("cross-origin-resource-policy"),
                "same-origin".to_string(),
            ),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
        ],
        octets,
    )
        .into_response())
}
