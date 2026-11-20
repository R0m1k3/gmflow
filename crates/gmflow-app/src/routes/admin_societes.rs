use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Societe {
    id: i64,
    raison_sociale: String,
    siret: Option<String>,
    tva: Option<String>,
    adresses: Value,
    outil_depot: Option<String>,
    adresse_depot: Option<String>,
    archivee: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Entree {
    raison_sociale: String,
    #[serde(default)]
    siret: Option<String>,
    #[serde(default)]
    tva: Option<String>,
    #[serde(default = "tableau_vide")]
    adresses: Value,
    #[serde(default)]
    outil_depot: Option<String>,
    #[serde(default)]
    adresse_depot: Option<String>,
    #[serde(default)]
    archivee: bool,
}

fn tableau_vide() -> Value {
    json!([])
}

fn vide_en_null(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Entree {
    fn nettoyer(mut self) -> Resultat<Self> {
        self.raison_sociale = self.raison_sociale.trim().to_string();
        if self.raison_sociale.is_empty() {
            return Err(ErreurApi::Donnees("raison sociale obligatoire".into()));
        }
        self.siret = vide_en_null(self.siret);
        self.tva = vide_en_null(self.tva);
        self.outil_depot = vide_en_null(self.outil_depot);
        self.adresse_depot = vide_en_null(self.adresse_depot);
        if let Some(o) = &self.outil_depot
            && o != "dext"
            && o != "yooz"
        {
            return Err(ErreurApi::Donnees("outil de dépôt : dext ou yooz".into()));
        }
        if !self.adresses.is_array() {
            return Err(ErreurApi::Donnees("adresses : liste attendue".into()));
        }
        Ok(self)
    }
}

pub async fn lister(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vec<Societe>>> {
    let v = sqlx::query_as!(
        Societe,
        "select id, raison_sociale, siret, tva, adresses, outil_depot, adresse_depot, archivee from societe order by raison_sociale"
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}

pub async fn creer(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>, Json(e): Json<Entree>) -> Resultat<Json<Societe>> {
    let e = e.nettoyer()?;
    let mut tx = etat.pool.begin().await?;
    let s = sqlx::query_as!(
        Societe,
        "insert into societe (raison_sociale, siret, tva, adresses, outil_depot, adresse_depot, archivee)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning id, raison_sociale, siret, tva, adresses, outil_depot, adresse_depot, archivee",
        e.raison_sociale,
        e.siret,
        e.tva,
        e.adresses,
        e.outil_depot,
        e.adresse_depot,
        e.archivee
    )
    .fetch_one(&mut *tx)
    .await?;
    gmflow_core::journal::ecrire(&mut *tx, Some(a.utilisateur_id), "creation", "societe", Some(s.id.to_string()), json!(e)).await?;
    tx.commit().await?;
    Ok(Json(s))
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Societe>> {
    let e = e.nettoyer()?;
    let mut tx = etat.pool.begin().await?;
    let s = sqlx::query_as!(
        Societe,
        "update societe set raison_sociale = $2, siret = $3, tva = $4, adresses = $5, outil_depot = $6,
             adresse_depot = $7, archivee = $8
         where id = $1
         returning id, raison_sociale, siret, tva, adresses, outil_depot, adresse_depot, archivee",
        id,
        e.raison_sociale,
        e.siret,
        e.tva,
        e.adresses,
        e.outil_depot,
        e.adresse_depot,
        e.archivee
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ErreurApi::Absent)?;
    gmflow_core::journal::ecrire(&mut *tx, Some(a.utilisateur_id), "modification", "societe", Some(id.to_string()), json!(e)).await?;
    tx.commit().await?;
    Ok(Json(s))
}
