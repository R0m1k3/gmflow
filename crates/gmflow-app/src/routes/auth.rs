use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;

use crate::acces::{Acces, CLE_SESSION, SessionUtilisateur, maintenant_unix};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
use crate::mdp;

pub const ECHECS_MAX: usize = 5;
pub const FENETRE: Duration = Duration::from_secs(15 * 60);

#[derive(Deserialize)]
pub struct Entree {
    identifiant: String,
    mot_de_passe: String,
}

#[derive(Serialize)]
pub struct Moi {
    id: i64,
    identifiant: String,
    role: &'static str,
    societe_id: Option<i64>,
}

impl From<&Acces> for Moi {
    fn from(a: &Acces) -> Self {
        Moi {
            id: a.utilisateur_id,
            identifiant: a.identifiant.clone(),
            role: a.role_texte(),
            societe_id: a.societe_id,
        }
    }
}

fn trop_d_echecs(etat: &Etat, cle: &str) -> bool {
    let mut echecs = etat.echecs.lock().unwrap();
    let liste = echecs.entry(cle.to_string()).or_default();
    liste.retain(|t| t.elapsed() < FENETRE);
    liste.len() >= ECHECS_MAX
}

fn noter_echec(etat: &Etat, cle: &str) {
    let mut echecs = etat.echecs.lock().unwrap();
    echecs
        .entry(cle.to_string())
        .or_default()
        .push(Instant::now());
    // Évite que la table grossisse sans fin.
    if echecs.len() > 10_000 {
        echecs.retain(|_, v| v.iter().any(|t| t.elapsed() < FENETRE));
    }
}

pub async fn connexion(
    State(etat): State<Etat>,
    session: Session,
    Json(e): Json<Entree>,
) -> Resultat<Json<Moi>> {
    let identifiant = e.identifiant.trim().to_string();
    let cle = identifiant.to_lowercase();
    if trop_d_echecs(&etat, &cle) {
        return Err(ErreurApi::TropDEssais);
    }
    let ligne = sqlx::query!(
        "select id, identifiant, hash_mdp, role, societe_id, actif from utilisateur where lower(identifiant) = $1",
        cle
    )
    .fetch_optional(&etat.pool)
    .await?;
    let hash = ligne
        .as_ref()
        .map(|l| l.hash_mdp.clone())
        .unwrap_or_else(|| mdp::hash_factice().to_string());
    let bon = mdp::verifier_async(e.mot_de_passe, hash).await;
    let u = match ligne {
        Some(u) if bon && u.actif => u,
        autre => {
            noter_echec(&etat, &cle);
            gmflow_core::journal::ecrire(
                &etat.pool,
                autre.as_ref().map(|u| u.id),
                "connexion_refusee",
                "utilisateur",
                autre.as_ref().map(|u| u.id.to_string()),
                json!({ "identifiant": identifiant }),
            )
            .await?;
            return Err(ErreurApi::NonConnecte);
        }
    };
    etat.echecs.lock().unwrap().remove(&cle);
    session.cycle_id().await?;
    session
        .insert(
            CLE_SESSION,
            SessionUtilisateur {
                id: u.id,
                connecte_le: maintenant_unix(),
            },
        )
        .await?;
    gmflow_core::journal::ecrire(
        &etat.pool,
        Some(u.id),
        "connexion",
        "utilisateur",
        Some(u.id.to_string()),
        json!({}),
    )
    .await?;
    let role = if u.role == "admin" {
        "admin"
    } else {
        "responsable"
    };
    Ok(Json(Moi {
        id: u.id,
        identifiant: u.identifiant,
        role,
        societe_id: u.societe_id,
    }))
}

pub async fn deconnexion(session: Session) -> Resultat<StatusCode> {
    session.flush().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn moi(acces: Acces) -> Json<Moi> {
    Json(Moi::from(&acces))
}
