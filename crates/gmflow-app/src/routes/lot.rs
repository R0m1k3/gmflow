//! Validation par lot (audit U2) : seules les factures et avoirs sans aucune alerte passent ;
//! chaque pièce est validée par la même fonction que la validation unitaire
//! (`documents::valider`), dans sa propre transaction, droits et société vérifiés pour chacune.
//! Les envois du lot partent ensemble après le délai d'annulation, et s'annulent ensemble.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;

use super::documents::{self, ContexteFile, DemandeValidation};
use super::retour::Annulable;
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

/// Pièces au plus par appel.
pub const MAX_LOT: usize = 100;

/// Seuils de confiance de la lecture sous lesquels un champ est « peu sûr » (mêmes valeurs que
/// le surlignage de la fiche, `web/src/lib/ui/DocOutils.ts`).
const SEUIL_CONFIANCE: f64 = 0.7;
const SEUIL_CONFIANCE_SOCIETE: f64 = 0.9;

struct EtatLot {
    id: i64,
    etat: String,
    r#type: Option<String>,
    statut_depot: Option<String>,
    valide: bool,
    verifie: bool,
    n_controles: i64,
    message: Option<String>,
    confiance: Value,
    ecarts: i64,
}

/// Pourquoi une pièce ne peut pas être validée avec les autres ; `None` : elle le peut.
fn raison(d: &EtatLot) -> Option<String> {
    if d.etat != "lu" || !matches!(d.r#type.as_deref(), Some("facture" | "avoir")) {
        return Some("Pas une facture ni un avoir lus".into());
    }
    if d.valide || matches!(d.statut_depot.as_deref(), Some("en_envoi" | "depose")) {
        return Some("Déjà validée".into());
    }
    match d.statut_depot.as_deref() {
        Some("doublon") => return Some("Doublon possible : à ouvrir".into()),
        Some("erreur") => return Some("Envoi échoué : à ouvrir".into()),
        _ => {}
    }
    let peu_sur = !d.verifie
        && [
            "type",
            "societe",
            "fournisseur",
            "numero",
            "date",
            "montants",
        ]
        .iter()
        .any(|k| {
            let seuil = if *k == "societe" {
                SEUIL_CONFIANCE_SOCIETE
            } else {
                SEUIL_CONFIANCE
            };
            d.confiance[*k].as_f64().is_some_and(|c| c < seuil)
        });
    let n = d.n_controles
        + i64::from(d.ecarts > 0)
        + i64::from(!d.verifie && d.message.is_some())
        + i64::from(peu_sur);
    if n > 0 {
        return Some(if n == 1 {
            "1 alerte à lire".into()
        } else {
            format!("{n} alertes à lire")
        });
    }
    if d.statut_depot.as_deref() != Some("pret") {
        return Some("À ouvrir".into());
    }
    None
}

/// États utiles au lot. `societe` : filtre d'un responsable (pièces visibles et modifiables
/// seulement), `None` sans filtre.
async fn lire(
    c: &mut PgConnection,
    ids: &[i64],
    filtre: bool,
    societe: Option<i64>,
) -> sqlx::Result<Vec<EtatLot>> {
    let r = sqlx::query!(
        r#"select d.id, d.etat, d.type, d.statut_depot, d.valide_le is not null as "valide!",
                  d.verifie_le is not null as "verifie!",
                  coalesce(case when jsonb_typeof(d.controles) = 'array'
                                then jsonb_array_length(d.controles) end, 0)::bigint as "n_controles!",
                  d.message, coalesce(d.confiance, '{}') as "confiance!",
                  (select count(*) from rapprochement r cross join jsonb_array_elements(r.ecarts) e
                   where not r.rejete and (r.facture_id = d.id or r.bl_id = d.id)) as "ecarts!"
           from document d
           where d.id = any($1)
             and (not $2 or (exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                     where mb.mail_id = d.mail_id and ($3::bigint is null or b.societe_id = $3))
                             and ($3::bigint is null or d.societe_id is null or d.societe_id = $3)))"#,
        ids,
        filtre,
        societe
    )
    .fetch_all(&mut *c)
    .await?;
    Ok(r.into_iter()
        .map(|r| EtatLot {
            id: r.id,
            etat: r.etat,
            r#type: r.r#type,
            statut_depot: r.statut_depot,
            valide: r.valide,
            verifie: r.verifie,
            n_controles: r.n_controles,
            message: r.message,
            confiance: r.confiance,
            ecarts: r.ecarts,
        })
        .collect())
}

/// Dans la transaction de la validation (document verrouillé, avant toute écriture) : la raison
/// qui écarte la pièce du lot, ou rien.
pub(crate) async fn raison_hors_lot(c: &mut PgConnection, id: i64) -> sqlx::Result<Option<String>> {
    Ok(lire(c, &[id], false, None)
        .await?
        .first()
        .map_or(Some("Introuvable".into()), raison))
}

#[derive(Deserialize)]
pub struct DemandeLot {
    ids: Vec<i64>,
}

fn ids_nets(ids: Vec<i64>) -> Resultat<Vec<i64>> {
    let mut vus = std::collections::HashSet::new();
    let ids: Vec<i64> = ids.into_iter().filter(|i| vus.insert(*i)).collect();
    if ids.is_empty() {
        return Err(ErreurApi::Donnees("aucune pièce choisie".into()));
    }
    if ids.len() > MAX_LOT {
        return Err(ErreurApi::Donnees(format!(
            "{MAX_LOT} pièces au plus à la fois"
        )));
    }
    Ok(ids)
}

#[derive(Serialize)]
pub struct Eligibilite {
    id: i64,
    /// Rien n'empêche de la valider avec les autres.
    possible: bool,
    /// Sinon pourquoi : « 2 alertes à lire », « Doublon possible : à ouvrir »…
    raison: Option<String>,
}

/// Quelles pièces de la liste peuvent être cochées (même règle que la validation par lot).
/// Les pièces hors du périmètre de l'utilisateur ne sont pas rendues.
pub async fn eligibilite(
    acces: Acces,
    State(etat): State<Etat>,
    Json(d): Json<DemandeLot>,
) -> Resultat<Json<Vec<Eligibilite>>> {
    let ids = ids_nets(d.ids)?;
    let mut c = etat.pool.acquire().await?;
    let etats = lire(&mut c, &ids, true, acces.societe_filtre()).await?;
    Ok(Json(
        etats
            .iter()
            .map(|e| {
                let r = raison(e);
                Eligibilite {
                    id: e.id,
                    possible: r.is_none(),
                    raison: r,
                }
            })
            .collect(),
    ))
}

#[derive(Serialize)]
pub struct ResultatPiece {
    id: i64,
    /// Validée, envoi demandé.
    validee: bool,
    /// Laissée à contrôler : pourquoi.
    raison: Option<String>,
}

#[derive(Serialize)]
pub struct ResultatLot {
    validees: usize,
    laissees: usize,
    pieces: Vec<ResultatPiece>,
    /// Envois du lot, annulables ensemble pendant le délai.
    #[serde(skip_serializing_if = "Option::is_none")]
    annulation: Option<Annulable>,
    /// Outil de dépôt des pièces validées (`dext`, `yooz`), s'il n'y en a qu'un.
    outil: Option<String>,
}

fn phrase_refus(e: ErreurApi) -> String {
    match e {
        ErreurApi::Absent => "Introuvable".into(),
        ErreurApi::Interdit => "Pas dans votre société".into(),
        ErreurApi::ConflitCode { message, .. }
        | ErreurApi::Conflit(message)
        | ErreurApi::Donnees(message)
        | ErreurApi::Champ { message, .. } => message,
        e => {
            tracing::error!(erreur = ?e, "validation par lot : pièce en erreur");
            "Erreur : réessayez seule".into()
        }
    }
}

/// « Valider N pièces » : chaque pièce passe par `documents::valider` (mêmes contrôles, même
/// transaction par pièce), en exigeant qu'elle n'ait aucune alerte. Résultat par pièce.
pub async fn valider(
    acces: Acces,
    State(etat): State<Etat>,
    Json(d): Json<DemandeLot>,
) -> Resultat<Json<ResultatLot>> {
    let ids = ids_nets(d.ids)?;
    let lot = uuid::Uuid::new_v4();
    let mut r = ResultatLot {
        validees: 0,
        laissees: 0,
        pieces: Vec::with_capacity(ids.len()),
        annulation: None,
        outil: None,
    };
    for id in ids {
        let dv = DemandeValidation::pour_lot(lot);
        let issue = documents::valider(
            acces.clone(),
            State(etat.clone()),
            Path(id),
            Query(ContexteFile::default()),
            Json(dv),
        )
        .await;
        match issue {
            Ok(Json(v)) if v.envoi => {
                r.validees += 1;
                if r.annulation.is_none() {
                    r.annulation = v.annulation;
                }
                r.pieces.push(ResultatPiece {
                    id: v.id,
                    validee: true,
                    raison: None,
                });
            }
            Ok(Json(v)) => {
                // Pas une facture ni un avoir (refusé avant par `raison_hors_lot`) : par sûreté.
                r.laissees += 1;
                r.pieces.push(ResultatPiece {
                    id: v.id,
                    validee: false,
                    raison: Some("Validée sans envoi".into()),
                });
            }
            Err(e) => {
                r.laissees += 1;
                r.pieces.push(ResultatPiece {
                    id,
                    validee: false,
                    raison: Some(phrase_refus(e)),
                });
            }
        }
    }
    let validees: Vec<i64> = r
        .pieces
        .iter()
        .filter(|p| p.validee)
        .map(|p| p.id)
        .collect();
    if !validees.is_empty() {
        let outils = sqlx::query_scalar!(
            r#"select distinct s.outil_depot as "o!" from document d join societe s on s.id = d.societe_id
               where d.id = any($1) and s.outil_depot is not null"#,
            &validees
        )
        .fetch_all(&etat.pool)
        .await?;
        if let [o] = outils.as_slice() {
            r.outil = Some(o.clone());
        }
    }
    Ok(Json(r))
}
