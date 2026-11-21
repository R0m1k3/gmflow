//! Qui fait la requête, et ce qu'il peut voir.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::erreur::ErreurApi;
use crate::etat::Etat;

pub const CLE_SESSION: &str = "utilisateur";

/// Contenu de la session : l'id et la date de connexion (le reste est relu en base).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUtilisateur {
    pub id: i64,
    pub connecte_le: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Admin,
    Responsable,
}

#[derive(Debug, Clone)]
pub struct Acces {
    pub utilisateur_id: i64,
    pub identifiant: String,
    pub role: Role,
    pub societe_id: Option<i64>,
}

impl Acces {
    pub fn est_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Société à laquelle limiter les lectures : `None` pour un administrateur.
    /// S'utilise dans les requêtes sous la forme `($n::bigint is null or <condition sur $n>)`.
    pub fn societe_filtre(&self) -> Option<i64> {
        match self.role {
            Role::Admin => None,
            Role::Responsable => self.societe_id,
        }
    }

    pub fn role_texte(&self) -> &'static str {
        match self.role {
            Role::Admin => "admin",
            Role::Responsable => "responsable",
        }
    }
}

pub fn maintenant_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

impl FromRequestParts<Etat> for Acces {
    type Rejection = ErreurApi;

    async fn from_request_parts(parts: &mut Parts, etat: &Etat) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, etat)
            .await
            .map_err(|_| ErreurApi::NonConnecte)?;
        let Some(su) = session.get::<SessionUtilisateur>(CLE_SESSION).await? else {
            return Err(ErreurApi::NonConnecte);
        };
        if maintenant_unix() - su.connecte_le > etat.durees.absolue.as_secs() as i64 {
            session.flush().await?;
            return Err(ErreurApi::NonConnecte);
        }
        let ligne = sqlx::query!(
            "select identifiant, role, societe_id from utilisateur where id = $1 and actif",
            su.id
        )
        .fetch_optional(&etat.pool)
        .await?;
        let Some(u) = ligne else {
            session.flush().await?;
            return Err(ErreurApi::NonConnecte);
        };
        let role = if u.role == "admin" {
            Role::Admin
        } else {
            Role::Responsable
        };
        Ok(Acces {
            utilisateur_id: su.id,
            identifiant: u.identifiant,
            role,
            societe_id: u.societe_id,
        })
    }
}

/// Accès réservé aux administrateurs (403 sinon).
pub struct AccesAdmin(pub Acces);

impl FromRequestParts<Etat> for AccesAdmin {
    type Rejection = ErreurApi;

    async fn from_request_parts(parts: &mut Parts, etat: &Etat) -> Result<Self, Self::Rejection> {
        let a = Acces::from_request_parts(parts, etat).await?;
        if !a.est_admin() {
            return Err(ErreurApi::Interdit);
        }
        Ok(AccesAdmin(a))
    }
}
