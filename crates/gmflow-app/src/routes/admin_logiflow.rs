//! Logiflow : adresse et clé de l'API externe (chiffrée, jamais renvoyée : seulement ses 4
//! derniers caractères), essai fait par le worker, et relance de l'écriture d'une facture dans
//! Logiflow depuis sa fiche.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::logiflow::{self, ServeurApi};
use gmflow_core::{file, journal, parametre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Refus, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LogiflowVue {
    serveur: Option<ServeurApi>,
    jeton_enregistre: bool,
    #[cfg_attr(feature = "ts", ts(as = "Option<super::formes::DernierTest>"))]
    dernier_test: Option<Value>,
    /// Dernière vérification régulière : `{"quand", "ok", "message"}` (`ok` null : en cours).
    #[cfg_attr(feature = "ts", ts(as = "Option<super::formes::SuiviLogiflow>"))]
    dernier_suivi: Option<Value>,
}

pub async fn lire(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<LogiflowVue>> {
    let serveur = parametre::lire(&etat.pool, logiflow::CLE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok());
    Ok(Json(LogiflowVue {
        serveur,
        jeton_enregistre: parametre::lire_secret(&etat.pool, logiflow::CLE)
            .await?
            .is_some(),
        dernier_test: parametre::lire(&etat.pool, logiflow::CLE_TEST).await?,
        dernier_suivi: parametre::lire(&etat.pool, logiflow::CLE_SUIVI).await?,
    }))
}

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct EntreeLogiflow {
    adresse: String,
    /// Omis : inchangé ; chaîne vide : supprimé.
    #[serde(default)]
    jeton: Option<String>,
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeLogiflow>,
) -> Resultat<StatusCode> {
    let adresse = e.adresse.trim().to_string();
    logiflow::controler_adresse(&adresse).map_err(|m| {
        ErreurApi::Donnees(m.replace("https://api.ffnancy.fr", "https://logiflow.exemple.fr"))
    })?;
    let jeton = e.jeton.as_deref().map(str::trim);
    if jeton.is_some_and(|j| j.chars().any(char::is_whitespace)) {
        return Err(ErreurApi::Donnees(
            "la clé ne doit pas contenir d'espace".into(),
        ));
    }
    let ancien: Option<ServeurApi> = parametre::lire(&etat.pool, logiflow::CLE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok());
    let fin = match jeton {
        None => ancien.and_then(|s| s.fin),
        Some("") => None,
        Some(j) => Some(logiflow::fin(j)),
    };
    let mut tx = etat.pool.begin().await?;
    parametre::ecrire(
        &mut *tx,
        logiflow::CLE,
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
                logiflow::CLE
            )
            .execute(&mut *tx)
            .await?;
        }
        Some(j) => {
            parametre::ecrire_secret(
                &mut *tx,
                logiflow::CLE,
                &etat.chiffreur.chiffrer(j.as_bytes()),
            )
            .await?
        }
    }
    parametre::supprimer(&mut *tx, logiflow::CLE_TEST).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "logiflow",
        None,
        json!({ "adresse": adresse, "jeton_change": jeton.is_some() }),
    )
    .await?;
    tx.commit().await?;
    // Nouvelle configuration : un essai tout de suite.
    file::pousser_envoi(&etat.pool, file::TacheEnvoi::TestLogiflow)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn supprimer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<StatusCode> {
    let mut tx = etat.pool.begin().await?;
    parametre::supprimer(&mut *tx, logiflow::CLE).await?;
    parametre::supprimer(&mut *tx, logiflow::CLE_TEST).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "suppression",
        "logiflow",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tester(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>) -> Resultat<StatusCode> {
    if parametre::lire(&etat.pool, logiflow::CLE).await?.is_none() {
        return Err(ErreurApi::ConflitCode {
            code: Refus::LogiflowAbsent,
            message: "adresse de Logiflow non configurée".into(),
        });
    }
    parametre::supprimer(&etat.pool, logiflow::CLE_TEST).await?;
    file::pousser_envoi(&etat.pool, file::TacheEnvoi::TestLogiflow)
        .await
        .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test",
        "logiflow",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

/// « Vérifier maintenant » : la vérification régulière part tout de suite.
pub async fn suivre(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>) -> Resultat<StatusCode> {
    if parametre::lire(&etat.pool, logiflow::CLE).await?.is_none() {
        return Err(ErreurApi::ConflitCode {
            code: Refus::LogiflowAbsent,
            message: "adresse de Logiflow non configurée".into(),
        });
    }
    parametre::ecrire(
        &etat.pool,
        logiflow::CLE_SUIVI,
        &json!({ "quand": time::OffsetDateTime::now_utc()
                     .format(&time::format_description::well_known::Rfc3339)
                     .unwrap_or_default(),
                 "ok": null, "message": "vérification demandée" }),
    )
    .await?;
    file::pousser_envoi(&etat.pool, file::TacheEnvoi::SuiviLogiflow)
        .await
        .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "suivi",
        "logiflow",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

/// « Relancer » sur la fiche d'une facture envoyée : la recherche dans Logiflow repart (BL saisi
/// entre-temps dans Logiflow, n° corrigé…). L'ancien résultat est effacé.
pub async fn relancer(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    super::documents::controler_visible(&etat, &acces, id).await?;
    if parametre::lire(&etat.pool, logiflow::CLE).await?.is_none() {
        return Err(ErreurApi::ConflitCode {
            code: Refus::LogiflowAbsent,
            message: "Logiflow n'est pas configuré".into(),
        });
    }
    let envoyee = sqlx::query_scalar!(
        r#"select coalesce(type = 'facture' and statut_depot = 'depose', false) as "e!"
           from document where id = $1"#,
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    if !envoyee {
        return Err(ErreurApi::Conflit(
            "seule une facture envoyée à Dext/Yooz est reportée dans Logiflow".into(),
        ));
    }
    sqlx::query!(
        "delete from logiflow_rapprochement where document_id = $1",
        id
    )
    .execute(&etat.pool)
    .await?;
    file::pousser_envoi(
        &etat.pool,
        file::TacheEnvoi::RapprocherLogiflow { document_id: id },
    )
    .await
    .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "relance",
        "logiflow",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}
