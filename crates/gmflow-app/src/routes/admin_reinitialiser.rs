//! Réglages > Avancé > Zone dangereuse : tout reprendre de zéro en gardant la configuration
//! (voir `gmflow_core::reinitialisation`).

use axum::Json;
use axum::extract::State;
use gmflow_core::reinitialisation::{self, Bilan, Refus};
use serde::Deserialize;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

/// Mot à taper dans la confirmation de l'interface.
pub const CONFIRMATION: &str = "RÉINITIALISER";

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct DemandeReinitialisation {
    #[serde(default)]
    confirmation: String,
}

pub async fn reinitialiser(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(d): Json<DemandeReinitialisation>,
) -> Resultat<Json<Bilan>> {
    if d.confirmation.trim() != CONFIRMATION {
        return Err(ErreurApi::Donnees(format!(
            "tapez {CONFIRMATION} pour confirmer"
        )));
    }
    match reinitialisation::reinitialiser(&etat.pool, Some(a.utilisateur_id)).await {
        Ok(b) => Ok(Json(b)),
        Err(Refus::Base(e)) => Err(e.into()),
        Err(r @ (Refus::ReleveEnCours | Refus::EnvoiEnCours | Refus::Occupe)) => {
            Err(ErreurApi::Conflit(r.to_string()))
        }
    }
}
