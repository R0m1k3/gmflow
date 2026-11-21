use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
use crate::mdp;

#[derive(Serialize)]
pub struct Utilisateur {
    id: i64,
    identifiant: String,
    role: String,
    societe_id: Option<i64>,
    actif: bool,
    #[serde(with = "time::serde::rfc3339")]
    cree_le: time::OffsetDateTime,
}

#[derive(Deserialize)]
pub struct Entree {
    identifiant: String,
    #[serde(default)]
    mot_de_passe: Option<String>,
    role: String,
    #[serde(default)]
    societe_id: Option<i64>,
    #[serde(default = "vrai")]
    actif: bool,
}

fn vrai() -> bool {
    true
}

impl Entree {
    fn nettoyer(mut self) -> Resultat<Self> {
        self.identifiant = self.identifiant.trim().to_string();
        if self.identifiant.is_empty() {
            return Err(ErreurApi::Donnees("identifiant obligatoire".into()));
        }
        match self.role.as_str() {
            "admin" => self.societe_id = None,
            "responsable" if self.societe_id.is_none() => {
                return Err(ErreurApi::Donnees(
                    "un responsable est rattaché à une structure".into(),
                ));
            }
            "responsable" => {}
            _ => return Err(ErreurApi::Donnees("rôle : admin ou responsable".into())),
        }
        self.mot_de_passe = self.mot_de_passe.filter(|m| !m.is_empty());
        if let Some(m) = &self.mot_de_passe {
            mdp::valider(m)?;
        }
        Ok(self)
    }

    fn journal(&self) -> serde_json::Value {
        json!({ "identifiant": self.identifiant, "role": self.role, "societe_id": self.societe_id,
                "actif": self.actif, "mot_de_passe_change": self.mot_de_passe.is_some() })
    }
}

pub async fn lister(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vec<Utilisateur>>> {
    let v = sqlx::query_as!(
        Utilisateur,
        "select id, identifiant, role, societe_id, actif, cree_le from utilisateur order by identifiant"
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}

pub async fn creer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Utilisateur>> {
    let e = e.nettoyer()?;
    let Some(m) = e.mot_de_passe.clone() else {
        return Err(ErreurApi::Donnees("mot de passe obligatoire".into()));
    };
    let hash = mdp::hacher_async(m).await?;
    let mut tx = etat.pool.begin().await?;
    let u = sqlx::query_as!(
        Utilisateur,
        "insert into utilisateur (identifiant, hash_mdp, role, societe_id, actif) values ($1, $2, $3, $4, $5)
         returning id, identifiant, role, societe_id, actif, cree_le",
        e.identifiant,
        hash,
        e.role,
        e.societe_id,
        e.actif
    )
    .fetch_one(&mut *tx)
    .await?;
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "creation",
        "utilisateur",
        Some(u.id.to_string()),
        e.journal(),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(u))
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Utilisateur>> {
    let e = e.nettoyer()?;
    if id == a.utilisateur_id && (e.role != "admin" || !e.actif) {
        return Err(ErreurApi::Conflit(
            "vous ne pouvez pas retirer vos propres droits d'administrateur".into(),
        ));
    }
    let hash = match e.mot_de_passe.clone() {
        Some(m) => Some(mdp::hacher_async(m).await?),
        None => None,
    };
    let mut tx = etat.pool.begin().await?;
    let u = sqlx::query_as!(
        Utilisateur,
        "update utilisateur set identifiant = $2, role = $3, societe_id = $4, actif = $5,
             hash_mdp = coalesce($6, hash_mdp)
         where id = $1
         returning id, identifiant, role, societe_id, actif, cree_le",
        id,
        e.identifiant,
        e.role,
        e.societe_id,
        e.actif,
        hash
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ErreurApi::Absent)?;
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "utilisateur",
        Some(id.to_string()),
        e.journal(),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(u))
}
