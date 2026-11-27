//! Revenir en arrière (audit U1, U23) : « Annuler » une validation pendant le délai avant
//! l'envoi à Dext/Yooz (`envoi_prevu`), et « Remettre à contrôler » une pièce écartée, avec le
//! motif de l'écartement.

use axum::Json;
use axum::extract::{Path, State};
use gmflow_core::envoi_prevu::{self, Issue};
use gmflow_core::journal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::documents::controler_modifiable;
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
#[cfg(feature = "ts")]
use gmflow_core::vocabulaire as v;

/// Envoi demandé, encore annulable : le message « Annuler » de l'interface compte `delai_s`
/// secondes (délai du serveur, jamais écrit en dur dans l'interface).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Annulable {
    pub lot: uuid::Uuid,
    pub delai_s: i64,
}

impl Annulable {
    pub fn pour(lot: uuid::Uuid) -> Self {
        Self {
            lot,
            delai_s: envoi_prevu::DELAI_ANNULATION_S,
        }
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct DemandeAnnulation {
    lot: uuid::Uuid,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct IssuePiece {
    id: i64,
    issue: Issue,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultatAnnulation {
    /// Validations annulées : rien n'est parti, pièces de nouveau à contrôler.
    annulees: usize,
    /// Trop tard : déjà envoyées (ou en cours d'envoi).
    deja_parties: usize,
    pieces: Vec<IssuePiece>,
}

/// « Annuler » : annule chaque envoi du lot pas encore pris par le worker. Droits vérifiés pour
/// chaque pièce (un lot ne contient que des pièces validées par cet utilisateur ou ses collègues
/// de la même société).
pub async fn annuler_envoi(
    acces: Acces,
    State(etat): State<Etat>,
    Json(d): Json<DemandeAnnulation>,
) -> Resultat<Json<ResultatAnnulation>> {
    let envois = envoi_prevu::du_lot(&etat.pool, d.lot).await?;
    if envois.is_empty() {
        return Err(ErreurApi::Absent);
    }
    let mut r = ResultatAnnulation {
        annulees: 0,
        deja_parties: 0,
        pieces: Vec::new(),
    };
    for (prevu, document) in envois {
        if controler_modifiable(&etat, &acces, document).await.is_err() {
            continue;
        }
        let issue = envoi_prevu::annuler(&etat.pool, prevu, Some(acces.utilisateur_id))
            .await
            .map_err(ErreurApi::Interne)?;
        match issue {
            Issue::Annulee => r.annulees += 1,
            Issue::DejaPartie => r.deja_parties += 1,
            Issue::RienAAnnuler => {}
        }
        r.pieces.push(IssuePiece {
            id: document,
            issue,
        });
    }
    if r.pieces.is_empty() {
        return Err(ErreurApi::Absent);
    }
    Ok(Json(r))
}

/// Motifs rapides d'écartement (U23).
pub const MOTIFS: &[&str] = &["doublon", "pas_pour_nous", "publicite", "autre"];
const TEXTE_MAX: usize = 500;

/// Corps facultatif de `POST /api/documents/{id}/ecarter`.
#[derive(Deserialize, Default, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, optional_fields = nullable))]
pub struct Motif {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<v::MotifEcartement>"))]
    pub(crate) motif: Option<String>,
    #[serde(default)]
    pub(crate) texte: Option<String>,
}

impl Motif {
    pub fn nettoyer(self) -> Resultat<Self> {
        let motif = self
            .motif
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty());
        if let Some(m) = &motif
            && !MOTIFS.contains(&m.as_str())
        {
            return Err(ErreurApi::Donnees(format!("motif inconnu : {m}")));
        }
        let texte = self
            .texte
            .map(|t| t.trim().chars().take(TEXTE_MAX).collect::<String>())
            .filter(|t| !t.is_empty());
        Ok(Self { motif, texte })
    }
}

/// Dans la transaction d'« Écarter », document verrouillé : note l'état d'avant (pour
/// « Remettre à contrôler »), qui, quand et pourquoi.
pub async fn noter_ecartement(
    tx: &mut sqlx::PgConnection,
    acces: &Acces,
    id: i64,
    m: &Motif,
) -> Resultat<()> {
    sqlx::query!(
        r#"update document d set ecartement = jsonb_build_object(
               'le', now(), 'par', $2::bigint,
               'par_nom', (select identifiant from utilisateur where id = $2),
               'motif', $3::text, 'texte', $4::text,
               'avant', jsonb_build_object('etat', d.etat, 'statut_depot', d.statut_depot,
                                           'controles', d.controles, 'verifie_le', d.verifie_le,
                                           'verifie_par', d.verifie_par))
           where d.id = $1"#,
        id,
        acces.utilisateur_id,
        m.motif,
        m.texte
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

fn refus(code: &'static str, message: &str) -> ErreurApi {
    ErreurApi::ConflitCode {
        code,
        message: message.into(),
    }
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RemiseEnLecture {
    id: i64,
    #[cfg_attr(feature = "ts", ts(as = "v::EtatDocument"))]
    etat: String,
    #[cfg_attr(feature = "ts", ts(as = "Option<v::StatutDepot>"))]
    statut_depot: Option<String>,
}

/// « Remettre à contrôler » : une pièce écartée (ou un doublon écarté) retrouve l'état noté à
/// l'écartement, puis ses alertes et ses liens sont recalculés. Journalisé avec le motif. Une
/// pièce écartée avant cette version (sans état noté) redevient une pièce lue à contrôler.
pub async fn remettre(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<RemiseEnLecture>> {
    controler_modifiable(&etat, &acces, id).await?;
    let mut tx = etat.pool.begin().await?;
    let a = sqlx::query!(
        r#"select etat, type, statut_depot, verifie_le is not null as "verifie!", lu_le is not null as "lu!",
                  ecartement
           from document where id = $1 for update"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    let doublon_ecarte =
        a.etat == "lu" && a.statut_depot.as_deref() == Some("doublon") && a.verifie;
    if a.etat != "ignore" && !doublon_ecarte {
        return Err(refus("etat", "cette pièce n'est pas écartée"));
    }
    // Origine d'un PDF découpé (`lecture::decoupage`) : ignorée par la lecture, jamais écartée
    // par un humain ; ses pièces se contrôlent chacune à part. La remettre en circuit doublerait
    // chaque pièce.
    let decoupe = sqlx::query_scalar!(
        r#"select exists (select 1 from piece_jointe p join document d on d.piece_jointe_id = p.source_id
                          where d.id = $1) as "e!""#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if decoupe {
        return Err(refus(
            "decoupe",
            "ce fichier a été découpé en plusieurs pièces : contrôlez chacune d'elles",
        ));
    }
    let avant: Value = match a.ecartement.as_ref().map(|e| e["avant"].clone()) {
        Some(v) if v.is_object() => v,
        // Écartée avant cette version : une pièce lue (facture, avoir, BL) redevient à contrôler.
        _ if a.etat == "ignore"
            && a.lu
            && a.r#type
                .as_deref()
                .is_some_and(|t| ["facture", "avoir", "bl"].contains(&t)) =>
        {
            json!({ "etat": "lu", "statut_depot": null, "controles": [], "verifie_le": null, "verifie_par": null })
        }
        _ => {
            return Err(refus(
                "etat",
                "état d'avant l'écartement inconnu : faites relire la pièce",
            ));
        }
    };
    let r = sqlx::query!(
        r#"update document set etat = coalesce(a.v->>'etat', 'lu'), statut_depot = a.v->>'statut_depot',
                controles = coalesce(a.v->'controles', '[]'),
                verifie_le = (a.v->>'verifie_le')::timestamptz,
                verifie_par = (a.v->>'verifie_par')::bigint,
                ecartement = null
           from (select $2::jsonb as v) a
           where id = $1 returning etat, statut_depot"#,
        id,
        avant
    )
    .fetch_one(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "ecartement_annule",
        "document",
        Some(id.to_string()),
        json!({ "ecartement": a.ecartement }),
    )
    .await?;
    tx.commit().await?;
    // Sans dépôt automatique : la pièce revient à un humain.
    let statut_depot = gmflow_core::depot::evaluer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?
        .or(r.statut_depot);
    gmflow_core::rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(Json(RemiseEnLecture {
        id,
        etat: r.etat,
        statut_depot,
    }))
}
