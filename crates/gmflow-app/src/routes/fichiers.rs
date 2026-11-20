//! Téléchargement d'une pièce jointe, extraite du `.eml` et vérifiée par son SHA-256.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
use crate::routes::mails::mail_visible;

fn type_sur(t: &str) -> String {
    let ok = !t.is_empty() && t.len() < 100 && t.bytes().all(|c| c.is_ascii_alphanumeric() || b"/.+-_".contains(&c));
    if ok { t.to_string() } else { "application/octet-stream".into() }
}

/// `Content-Disposition` avec un nom ASCII de secours et le nom UTF-8 (RFC 5987).
fn disposition(nom: &str) -> String {
    let ascii: String = nom
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' })
        .collect();
    let mut encode = String::new();
    for b in nom.bytes() {
        if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
            encode.push(b as char);
        } else {
            encode.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encode}")
}

pub async fn piece(acces: Acces, State(etat): State<Etat>, Path(id): Path<i64>) -> Resultat<Response> {
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
        gmflow_core::journal::ecrire(&etat.pool, Some(acces.utilisateur_id), "piece_alteree", "piece_jointe", Some(id.to_string()), json!({ "mail_id": p.mail_id })).await?;
        return Err(ErreurApi::Interne(anyhow::anyhow!("SHA-256 de la pièce {id} différent de l'original")));
    }
    Ok((
        [
            (header::CONTENT_TYPE, type_sur(&p.type_mime)),
            (header::CONTENT_DISPOSITION, disposition(&p.nom)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        octets,
    )
        .into_response())
}
