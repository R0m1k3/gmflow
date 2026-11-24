//! Connexion à l'API du serveur : adresse et jeton (chiffré, jamais renvoyé : seulement ses
//! 4 derniers caractères), essai fait par le worker.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use gmflow_core::serveur_api::{self, ServeurApi};
use gmflow_core::{file, journal, parametre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Vue {
    serveur: Option<ServeurApi>,
    jeton_enregistre: bool,
    dernier_test: Option<Value>,
}

pub async fn lire(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vue>> {
    let serveur = parametre::lire(&etat.pool, serveur_api::CLE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok());
    Ok(Json(Vue {
        serveur,
        jeton_enregistre: parametre::lire_secret(&etat.pool, serveur_api::CLE)
            .await?
            .is_some(),
        dernier_test: parametre::lire(&etat.pool, serveur_api::CLE_TEST).await?,
    }))
}

#[derive(Deserialize)]
pub struct Entree {
    adresse: String,
    /// Omis : inchangé ; chaîne vide : supprimé.
    #[serde(default)]
    jeton: Option<String>,
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<StatusCode> {
    let adresse = e.adresse.trim().to_string();
    serveur_api::controler_adresse(&adresse).map_err(ErreurApi::Donnees)?;
    let jeton = e.jeton.as_deref().map(str::trim);
    if jeton.is_some_and(|j| j.chars().any(char::is_whitespace)) {
        return Err(ErreurApi::Donnees(
            "le jeton ne doit pas contenir d'espace".into(),
        ));
    }
    let ancien: Option<ServeurApi> = parametre::lire(&etat.pool, serveur_api::CLE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok());
    let fin = match jeton {
        None => ancien.and_then(|s| s.fin),
        Some("") => None,
        Some(j) => Some(serveur_api::fin(j)),
    };
    let mut tx = etat.pool.begin().await?;
    parametre::ecrire(
        &mut *tx,
        serveur_api::CLE,
        &json!(ServeurApi {
            adresse: adresse.clone(),
            fin
        }),
    )
    .await?;
    match jeton {
        None => {}
        Some("") => {
            sqlx::query!(
                "update parametre set valeur_chiffree = null where cle = $1",
                serveur_api::CLE
            )
            .execute(&mut *tx)
            .await?;
        }
        Some(j) => {
            parametre::ecrire_secret(
                &mut *tx,
                serveur_api::CLE,
                &etat.chiffreur.chiffrer(j.as_bytes()),
            )
            .await?
        }
    }
    // L'ancien essai ne dit plus rien de la nouvelle configuration.
    parametre::supprimer(&mut *tx, serveur_api::CLE_TEST).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "serveur_api",
        None,
        json!({ "adresse": adresse, "jeton_change": jeton.is_some() }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn supprimer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<StatusCode> {
    let mut tx = etat.pool.begin().await?;
    parametre::supprimer(&mut *tx, serveur_api::CLE).await?;
    parametre::supprimer(&mut *tx, serveur_api::CLE_TEST).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "suppression",
        "serveur_api",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tester(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>) -> Resultat<StatusCode> {
    if parametre::lire(&etat.pool, serveur_api::CLE)
        .await?
        .is_none()
    {
        return Err(ErreurApi::Conflit(
            "adresse du serveur non configurée".into(),
        ));
    }
    parametre::supprimer(&etat.pool, serveur_api::CLE_TEST).await?;
    file::pousser_envoi(&etat.pool, file::TacheEnvoi::TestServeurApi)
        .await
        .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test",
        "serveur_api",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}
