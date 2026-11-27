use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;

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
    identifiant_libre(&etat, &e.identifiant, None).await?;
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

/// La connexion ne distingue pas « Marie » de « marie » : deux comptes ne peuvent pas porter le
/// même identifiant à la casse près (l'index `utilisateur_identifiant_casse` le garantit aussi
/// quand la base n'avait pas déjà de tels doublons).
async fn identifiant_libre(etat: &Etat, identifiant: &str, sauf: Option<i64>) -> Resultat<()> {
    let pris = sqlx::query_scalar!(
        r#"select exists (select 1 from utilisateur
                          where lower(identifiant) = lower($1) and id is distinct from $2) as "e!""#,
        identifiant,
        sauf
    )
    .fetch_one(&etat.pool)
    .await?;
    if pris {
        return Err(ErreurApi::Champ {
            champ: "identifiant",
            message: "cet identifiant existe déjà (majuscules et minuscules confondues)".into(),
        });
    }
    Ok(())
}

/// `PUT /api/admin/utilisateurs/{id}`. Un nouveau mot de passe coupe toutes les sessions de
/// ce compte (sauf celle de l'administrateur s'il change le sien).
pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    session: Session,
    Path(id): Path<i64>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Utilisateur>> {
    let e = e.nettoyer()?;
    if id == a.utilisateur_id && (e.role != "admin" || !e.actif) {
        return Err(ErreurApi::Conflit(
            "vous ne pouvez pas retirer vos propres droits d'administrateur".into(),
        ));
    }
    identifiant_libre(&etat, &e.identifiant, Some(id)).await?;
    let hash = match e.mot_de_passe.clone() {
        Some(m) => Some(mdp::hacher_async(m).await?),
        None => None,
    };
    let mut tx = etat.pool.begin().await?;
    let ligne = sqlx::query!(
        "update utilisateur set identifiant = $2, role = $3, societe_id = $4, actif = $5,
             hash_mdp = coalesce($6, hash_mdp),
             generation_session = generation_session + case when $6::text is null then 0 else 1 end
         where id = $1
         returning id, identifiant, role, societe_id, actif, cree_le, generation_session",
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
    let u = Utilisateur {
        id: ligne.id,
        identifiant: ligne.identifiant,
        role: ligne.role,
        societe_id: ligne.societe_id,
        actif: ligne.actif,
        cree_le: ligne.cree_le,
    };
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
    if hash.is_some() && id == a.utilisateur_id {
        crate::routes::auth::garder_session(&session, ligne.generation_session).await?;
    }
    Ok(Json(u))
}
