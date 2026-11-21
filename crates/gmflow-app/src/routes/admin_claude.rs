//! Jeton Claude : collage, test, séance setup-token. Le jeton n'est jamais renvoyé.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::{file, journal, parametre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

pub async fn etat(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Value>> {
    let v = parametre::lire(&etat.pool, parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or_else(|| json!({ "etat": "absent" }));
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct EntreeJeton {
    jeton: String,
}

pub async fn coller(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeJeton>,
) -> Resultat<StatusCode> {
    let jeton = e.jeton.trim();
    if !jeton.starts_with("sk-ant-") || jeton.len() < 20 || jeton.chars().any(char::is_whitespace) {
        return Err(ErreurApi::Donnees("jeton attendu : sk-ant-…".into()));
    }
    parametre::enregistrer_jeton_claude(&etat.pool, &etat.chiffreur, jeton).await?;
    file::pousser_test_jeton(&etat.pool).await?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "jeton_colle",
        "claude",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn supprimer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<StatusCode> {
    let mut tx = etat.pool.begin().await?;
    parametre::supprimer(&mut *tx, parametre::CLAUDE_JETON).await?;
    parametre::ecrire(
        &mut *tx,
        parametre::CLAUDE_ETAT,
        &json!({ "etat": "absent", "message": "" }),
    )
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "jeton_supprime",
        "claude",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tester(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>) -> Resultat<StatusCode> {
    file::pousser_test_jeton(&etat.pool).await?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test_jeton",
        "claude",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct SeanceCreee {
    id: Uuid,
}

pub async fn nouvelle_seance(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<Json<SeanceCreee>> {
    let mut tx = etat.pool.begin().await?;
    // Une séance jamais prise par le worker (arrêté) ne bloque pas les suivantes au-delà de son délai.
    sqlx::query!(
        "update seance_claude set etat = 'expiree', message = 'délai dépassé', code_chiffre = null
         where etat in ('demarrage', 'attente_code', 'code_recu') and expire_le < now()"
    )
    .execute(&mut *tx)
    .await?;
    let id =
        sqlx::query_scalar!("insert into seance_claude (etat) values ('demarrage') returning id")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| match ErreurApi::from(e) {
                ErreurApi::Conflit(_) => ErreurApi::Conflit("une séance est déjà en cours".into()),
                autre => autre,
            })?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "seance_setup_token",
        "claude",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    file::pousser_seance(&etat.pool, id).await?;
    Ok(Json(SeanceCreee { id }))
}

#[derive(Serialize)]
pub struct Seance {
    etat: String,
    url: Option<String>,
    message: Option<String>,
}

pub async fn seance(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<Uuid>,
) -> Resultat<Json<Seance>> {
    let s = sqlx::query_as!(
        Seance,
        "select etat, url, message from seance_claude where id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    Ok(Json(s))
}

#[derive(Deserialize)]
pub struct EntreeCode {
    code: String,
}

pub async fn code(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<Uuid>,
    Json(e): Json<EntreeCode>,
) -> Resultat<StatusCode> {
    let code = e.code.trim();
    if code.is_empty() || code.len() > 500 {
        return Err(ErreurApi::Donnees("code invalide".into()));
    }
    let chiffre = etat.chiffreur.chiffrer(code.as_bytes());
    let n = sqlx::query!(
        "update seance_claude set etat = 'code_recu', code_chiffre = $2
         where id = $1 and etat = 'attente_code' and expire_le > now()",
        id,
        chiffre
    )
    .execute(&etat.pool)
    .await?
    .rows_affected();
    if n == 0 {
        let existe = sqlx::query_scalar!(
            "select exists (select 1 from seance_claude where id = $1) as \"ok!\"",
            id
        )
        .fetch_one(&etat.pool)
        .await?;
        return Err(if existe {
            ErreurApi::Conflit("la séance n'attend pas de code".into())
        } else {
            ErreurApi::Absent
        });
    }
    Ok(StatusCode::NO_CONTENT)
}
