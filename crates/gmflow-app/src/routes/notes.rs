//! Notes internes sur une pièce (audit U23) : plusieurs notes horodatées et signées, écrites par
//! tout utilisateur qui voit la pièce (`documents::controler_visible`, filtre par société),
//! supprimées par leur auteur ou un administrateur. Ajout et suppression sont journalisés.
//! Jamais envoyées à l'extérieur : seules ces routes lisent `note_interne` (ni le dépôt
//! Dext/Yooz, ni la rédaction des réponses aux fournisseurs).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::journal;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::documents::controler_visible;
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

/// Longueur maximale d'une note (caractères), comme la contrainte de la table.
pub const TEXTE_MAX: usize = 2000;

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Note {
    id: i64,
    auteur_id: Option<i64>,
    auteur_nom: String,
    texte: String,
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    cree_le: time::OffsetDateTime,
    /// L'utilisateur peut la supprimer (son auteur, ou un administrateur).
    supprimable: bool,
}

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct NouvelleNote {
    texte: String,
}

/// Notes d'une pièce, la plus récente en premier.
pub async fn lister(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Vec<Note>>> {
    controler_visible(&etat, &acces, id).await?;
    let notes = sqlx::query_as!(
        Note,
        r#"select id, auteur_id, auteur_nom, texte, cree_le,
                  ($2 or auteur_id = $3) as "supprimable!"
           from note_interne where document_id = $1
           order by cree_le desc, id desc"#,
        id,
        acces.est_admin(),
        acces.utilisateur_id
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(notes))
}

/// Ajoute une note (texte nettoyé, 1 à `TEXTE_MAX` caractères).
pub async fn ajouter(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(n): Json<NouvelleNote>,
) -> Resultat<(StatusCode, Json<Note>)> {
    controler_visible(&etat, &acces, id).await?;
    let texte = n.texte.trim();
    if texte.is_empty() {
        return Err(ErreurApi::Champ {
            champ: "texte",
            message: "La note est vide.".into(),
        });
    }
    if texte.chars().count() > TEXTE_MAX {
        return Err(ErreurApi::Champ {
            champ: "texte",
            message: format!("{TEXTE_MAX} caractères au plus."),
        });
    }
    let mut tx = etat.pool.begin().await?;
    let note = sqlx::query_as!(
        Note,
        r#"insert into note_interne (document_id, auteur_id, auteur_nom, texte)
           values ($1, $2, $3, $4)
           returning id, auteur_id, auteur_nom, texte, cree_le, true as "supprimable!""#,
        id,
        acces.utilisateur_id,
        acces.identifiant,
        texte
    )
    .fetch_one(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "note_ajoutee",
        "document",
        Some(id.to_string()),
        json!({ "note_id": note.id, "texte": note.texte }),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(note)))
}

/// Supprime une note : son auteur ou un administrateur, sur une pièce qu'il voit.
pub async fn supprimer(
    acces: Acces,
    State(etat): State<Etat>,
    Path(note_id): Path<i64>,
) -> Resultat<StatusCode> {
    let n = sqlx::query!(
        "select document_id, auteur_id, texte from note_interne where id = $1",
        note_id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    controler_visible(&etat, &acces, n.document_id).await?;
    if !acces.est_admin() && n.auteur_id != Some(acces.utilisateur_id) {
        return Err(ErreurApi::Interdit);
    }
    let mut tx = etat.pool.begin().await?;
    sqlx::query!("delete from note_interne where id = $1", note_id)
        .execute(&mut *tx)
        .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "note_supprimee",
        "document",
        Some(n.document_id.to_string()),
        json!({ "note_id": note_id, "auteur_id": n.auteur_id, "texte": n.texte }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
