//! Création du premier administrateur par un lien à usage unique écrit dans les logs.

use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::erreur::{ErreurApi, Resultat};
use crate::etat::{Etat, JetonPremierCompte};
use crate::mdp;

pub const VALIDITE: Duration = Duration::from_secs(3600);
/// Verrou de transaction de la création du premier compte ("GMPC").
const VERROU: i64 = 0x474D_5043;

async fn aucun_utilisateur(etat: &Etat) -> sqlx::Result<bool> {
    sqlx::query_scalar!("select not exists (select 1 from utilisateur) as \"vide!\"")
        .fetch_one(&etat.pool)
        .await
}

/// Au démarrage : si aucun utilisateur n'existe, crée un jeton et écrit l'URL dans les logs.
pub async fn annoncer_si_besoin(etat: &Etat) -> anyhow::Result<Option<String>> {
    if !aucun_utilisateur(etat).await? {
        return Ok(None);
    }
    let jeton = hex::encode(rand::random::<[u8; 32]>());
    *etat.premier_compte.lock().unwrap() = Some(JetonPremierCompte {
        jeton: jeton.clone(),
        cree: Instant::now(),
    });
    let url = format!(
        "{}/premier-compte?jeton={jeton}",
        etat.config.url_publique.trim_end_matches('/')
    );
    tracing::warn!(url = %url, "Aucun compte : ouvrez ce lien (valable 1 h, usage unique) pour créer l'administrateur");
    Ok(Some(jeton))
}

fn jeton_valide(etat: &Etat, jeton: &str) -> bool {
    let garde = etat.premier_compte.lock().unwrap();
    match garde.as_ref() {
        Some(j) => {
            j.cree.elapsed() < VALIDITE
                && j.jeton.len() == jeton.len()
                && j.jeton
                    .bytes()
                    .zip(jeton.bytes())
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    == 0
        }
        None => false,
    }
}

pub async fn verifier(State(etat): State<Etat>, Path(jeton): Path<String>) -> Resultat<StatusCode> {
    if jeton_valide(&etat, &jeton) && aucun_utilisateur(&etat).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErreurApi::Absent)
    }
}

#[derive(Deserialize)]
pub struct Entree {
    jeton: String,
    identifiant: String,
    mot_de_passe: String,
}

pub async fn creer(
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<(StatusCode, Json<serde_json::Value>)> {
    if !jeton_valide(&etat, &e.jeton) {
        return Err(ErreurApi::Absent);
    }
    let identifiant = e.identifiant.trim().to_string();
    if identifiant.is_empty() {
        return Err(ErreurApi::Donnees("identifiant obligatoire".into()));
    }
    mdp::valider(&e.mot_de_passe)?;
    let hash = mdp::hacher_async(e.mot_de_passe).await?;

    let mut tx = etat.pool.begin().await?;
    sqlx::query!("select pg_advisory_xact_lock($1)", VERROU)
        .execute(&mut *tx)
        .await?;
    let vide = sqlx::query_scalar!("select not exists (select 1 from utilisateur) as \"vide!\"")
        .fetch_one(&mut *tx)
        .await?;
    if !vide {
        return Err(ErreurApi::Absent);
    }
    let id = sqlx::query_scalar!(
        "insert into utilisateur (identifiant, hash_mdp, role) values ($1, $2, 'admin') returning id",
        identifiant,
        hash
    )
    .fetch_one(&mut *tx)
    .await?;
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(id),
        "premier_compte",
        "utilisateur",
        Some(id.to_string()),
        json!({ "identifiant": identifiant }),
    )
    .await?;
    tx.commit().await?;
    *etat.premier_compte.lock().unwrap() = None;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "identifiant": identifiant, "role": "admin", "societe_id": null })),
    ))
}
