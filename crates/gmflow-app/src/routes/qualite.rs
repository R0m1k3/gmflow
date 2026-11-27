//! « Qualité de la lecture » (administrateur) : pièces validées du mois, part corrigée à la
//! main par champ, voie, modèle et fournisseur, et coût estimé des appels de lecture. Calcul
//! dans `gmflow_core::qualite`.

use axum::Json;
use axum::extract::{Query, State};
use gmflow_core::qualite::{self, Qualite};
use serde::{Deserialize, Serialize};

use super::depots::mois_choisi;
use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Deserialize)]
pub struct Filtre {
    /// AAAA-MM ; par défaut, le mois en cours (heure de Paris).
    mois: Option<String>,
    societe: Option<i64>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct QualiteLecture {
    mois: String,
    #[serde(flatten)]
    qualite: Qualite,
}

pub async fn qualite(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<QualiteLecture>> {
    let mois = mois_choisi(f.mois.as_deref())?;
    let debut = time::Date::parse(
        &format!("{mois}-01"),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ErreurApi::Donnees("mois invalide".into()))?;
    let societe = f.societe.or(acces.societe_filtre());
    let qualite = qualite::mesurer(&etat.pool, debut, societe).await?;
    Ok(Json(QualiteLecture { mois, qualite }))
}
