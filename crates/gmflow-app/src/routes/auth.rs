use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::Session;

use crate::acces::{Acces, CLE_SESSION, SessionUtilisateur, maintenant_unix};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
use crate::freinage::{AdresseClient, ESSAIS_LIBRES_ADRESSE, ESSAIS_LIBRES_COMPTE};
use crate::mdp;
#[cfg(feature = "ts")]
use gmflow_core::vocabulaire as v;

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct EntreeConnexion {
    identifiant: String,
    mot_de_passe: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Moi {
    id: i64,
    identifiant: String,
    #[cfg_attr(feature = "ts", ts(as = "v::Role"))]
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

/// `POST /api/connexion`. Les échecs freinent l'adresse IP qui les fait (`freinage`), jamais
/// le compte lui-même : un tiers ne peut pas empêcher le vrai utilisateur de se connecter.
pub async fn connexion(
    State(etat): State<Etat>,
    AdresseClient(ip): AdresseClient,
    session: Session,
    Json(e): Json<EntreeConnexion>,
) -> Resultat<Json<Moi>> {
    let identifiant = e.identifiant.trim().to_string();
    let cle = identifiant.to_lowercase();
    let reseau = crate::freinage::cle_reseau(&ip);
    let (cle_compte, cle_adresse) = (format!("{reseau}|{cle}"), format!("{reseau}|*"));
    // L'essai compte d'avance comme un échec (essais en parallèle), rendu s'il réussit.
    if etat
        .freinage
        .essayer(
            &[
                (&cle_compte, ESSAIS_LIBRES_COMPTE),
                (&cle_adresse, ESSAIS_LIBRES_ADRESSE),
            ],
            Instant::now(),
        )
        .is_err()
    {
        return Err(ErreurApi::TropDEssais);
    }
    let ligne = sqlx::query!(
        "select id, identifiant, hash_mdp, role, societe_id, actif, generation_session
         from utilisateur where lower(identifiant) = $1",
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
            gmflow_core::journal::ecrire(
                &etat.pool,
                autre.as_ref().map(|u| u.id),
                "connexion_refusee",
                "utilisateur",
                autre.as_ref().map(|u| u.id.to_string()),
                json!({ "identifiant": identifiant, "adresse": ip }),
            )
            .await?;
            return Err(ErreurApi::NonConnecte);
        }
    };
    etat.freinage.reussite(&cle_compte);
    etat.freinage.rendre(&cle_adresse, ESSAIS_LIBRES_ADRESSE);
    session.cycle_id().await?;
    session
        .insert(
            CLE_SESSION,
            SessionUtilisateur {
                id: u.id,
                connecte_le: maintenant_unix(),
                generation: u.generation_session,
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

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct ChangementMdp {
    ancien: String,
    nouveau: String,
}

/// Garde la session en cours valable après un changement de mot de passe (les autres sont
/// coupées par la nouvelle génération) : nouvel identifiant de session, même connexion.
pub async fn garder_session(session: &Session, generation: i32) -> Resultat<()> {
    if let Some(mut su) = session.get::<SessionUtilisateur>(CLE_SESSION).await? {
        su.generation = generation;
        session.cycle_id().await?;
        session.insert(CLE_SESSION, su).await?;
    }
    Ok(())
}

/// `POST /api/moi/mot-de-passe` : l'utilisateur connecté change son mot de passe (ancien
/// exigé, mêmes règles qu'à la création). Ses autres sessions sont coupées.
pub async fn changer_mot_de_passe(
    acces: Acces,
    State(etat): State<Etat>,
    session: Session,
    Json(c): Json<ChangementMdp>,
) -> Resultat<StatusCode> {
    let cle = format!("mdp|{}", acces.utilisateur_id);
    if etat
        .freinage
        .essayer(&[(&cle, ESSAIS_LIBRES_COMPTE)], Instant::now())
        .is_err()
    {
        return Err(ErreurApi::TropDEssais);
    }
    let hash = sqlx::query_scalar!(
        "select hash_mdp from utilisateur where id = $1",
        acces.utilisateur_id
    )
    .fetch_one(&etat.pool)
    .await?;
    if !mdp::verifier_async(c.ancien.clone(), hash).await {
        gmflow_core::journal::ecrire(
            &etat.pool,
            Some(acces.utilisateur_id),
            "mot_de_passe_refuse",
            "utilisateur",
            Some(acces.utilisateur_id.to_string()),
            json!({}),
        )
        .await?;
        return Err(ErreurApi::Champ {
            champ: "ancien",
            message: "le mot de passe actuel n'est pas le bon".into(),
        });
    }
    etat.freinage.reussite(&cle);
    if let Err(ErreurApi::Donnees(m)) = mdp::valider(&c.nouveau) {
        return Err(ErreurApi::Champ {
            champ: "nouveau",
            message: m,
        });
    }
    if c.nouveau == c.ancien {
        return Err(ErreurApi::Champ {
            champ: "nouveau",
            message: "le nouveau mot de passe doit être différent de l'actuel".into(),
        });
    }
    let hash = mdp::hacher_async(c.nouveau).await?;
    let mut tx = etat.pool.begin().await?;
    let generation = sqlx::query_scalar!(
        "update utilisateur set hash_mdp = $2, generation_session = generation_session + 1
         where id = $1 returning generation_session",
        acces.utilisateur_id,
        hash
    )
    .fetch_one(&mut *tx)
    .await?;
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "mot_de_passe_change",
        "utilisateur",
        Some(acces.utilisateur_id.to_string()),
        json!({ "par": "lui-meme", "autres_sessions_coupees": true }),
    )
    .await?;
    tx.commit().await?;
    garder_session(&session, generation).await?;
    Ok(StatusCode::NO_CONTENT)
}
