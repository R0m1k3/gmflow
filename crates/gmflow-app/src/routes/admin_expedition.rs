//! Adresse d'expédition (dépôts Dext/Yooz, alertes) et destinataires des alertes.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use gmflow_core::expedition::{self, Expedition, adresse_valide};
use gmflow_core::{file, journal, parametre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Vue {
    expedition: Option<Expedition>,
    mot_de_passe_enregistre: bool,
    alerte_destinataires: Vec<String>,
    dernier_test: Option<Value>,
}

pub async fn lire(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vue>> {
    let expedition = parametre::lire(&etat.pool, expedition::CLE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok());
    Ok(Json(Vue {
        expedition,
        mot_de_passe_enregistre: parametre::lire_secret(&etat.pool, expedition::CLE)
            .await?
            .is_some(),
        alerte_destinataires: expedition::destinataires_alertes(&etat.pool).await?,
        dernier_test: parametre::lire(&etat.pool, expedition::CLE_TEST).await?,
    }))
}

#[derive(Deserialize)]
pub struct Entree {
    expedition: Expedition,
    /// Omis : inchangé ; chaîne vide : supprimé.
    #[serde(default)]
    mot_de_passe: Option<String>,
    #[serde(default)]
    alerte_destinataires: Vec<String>,
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(mut e): Json<Entree>,
) -> Resultat<StatusCode> {
    e.expedition.serveur = e.expedition.serveur.trim().to_string();
    e.expedition.adresse = e.expedition.adresse.trim().to_string();
    e.expedition.identifiant = e.expedition.identifiant.trim().to_string();
    e.expedition.controler().map_err(ErreurApi::Donnees)?;
    let alertes: Vec<String> = e
        .alerte_destinataires
        .iter()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .collect();
    if let Some(d) = alertes.iter().find(|d| !adresse_valide(d)) {
        return Err(ErreurApi::Donnees(format!(
            "adresse d'alerte « {d} » invalide"
        )));
    }
    let mut tx = etat.pool.begin().await?;
    parametre::ecrire(
        &mut *tx,
        expedition::CLE,
        &serde_json::to_value(&e.expedition).unwrap_or_default(),
    )
    .await?;
    match e.mot_de_passe.as_deref() {
        None => {}
        Some("") => {
            sqlx::query!(
                "update parametre set valeur_chiffree = null where cle = $1",
                expedition::CLE
            )
            .execute(&mut *tx)
            .await?;
        }
        Some(m) => {
            parametre::ecrire_secret(
                &mut *tx,
                expedition::CLE,
                &etat.chiffreur.chiffrer(m.as_bytes()),
            )
            .await?
        }
    }
    parametre::ecrire(&mut *tx, expedition::CLE_ALERTES, &json!(alertes)).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "expedition",
        None,
        json!({
            "expedition": e.expedition,
            "mot_de_passe_change": e.mot_de_passe.is_some(),
            "alerte_destinataires": alertes
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Test {
    destinataire: String,
}

pub async fn tester(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(t): Json<Test>,
) -> Resultat<StatusCode> {
    let d = t.destinataire.trim().to_string();
    if !adresse_valide(&d) {
        return Err(ErreurApi::Donnees("adresse de test invalide".into()));
    }
    parametre::supprimer(&etat.pool, expedition::CLE_TEST).await?;
    file::pousser_envoi(
        &etat.pool,
        file::TacheEnvoi::TestExpedition {
            destinataire: d.clone(),
        },
    )
    .await
    .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test",
        "expedition",
        None,
        json!({ "destinataire": d }),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}
