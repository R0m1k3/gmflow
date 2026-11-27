//! Lignes articles corrigées à la main (audit L16) : modifier, ajouter, supprimer une ligne d'une
//! facture, d'un avoir ou d'un BL lus. Chaque changement est vérifié (nombres positifs, gencod
//! avec une clé GS1 juste), journalisé avant/après, et la ligne est marquée corrigée. Tout ce qui
//! dépend des lignes est recalculé sans Claude : montant de la ligne, totaux calculés d'un BL,
//! contrôle « somme des lignes », écarts facture/BL, recherche des articles au magasin (export
//! Phénix). Une relecture remplace les lignes, corrigées comprises (la fiche le dit avant).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::lecture::extraction::{self, Ligne};
use gmflow_core::lecture::xml::gencod_valide;
use gmflow_core::montant::{self, Decimal, dec};
use gmflow_core::{articles_magasin, depot, journal, parametre, rapprochement, serveur_api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::documents::{controler_modifiable, controler_visible};
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

const TEXTE_MAX: usize = 500;
/// Écart toléré entre le montant d'une ligne et quantité × prix (arrondis du document).
const TOLERANCE: Decimal = dec!(0.011);

/// Ligne telle que la fiche l'affiche et la corrige.
#[derive(Debug, Serialize)]
pub struct LigneEdit {
    id: i64,
    rang: i32,
    reference: Option<String>,
    designation: Option<String>,
    quantite: Option<Decimal>,
    unite: Option<String>,
    prix_unitaire_ht: Option<Decimal>,
    montant_ht: Option<Decimal>,
    gencod: Option<String>,
    colis: Option<Decimal>,
    pcb: Option<Decimal>,
    remise_taux: Option<Decimal>,
    taux_tva: Option<Decimal>,
    deee_ht: Option<Decimal>,
    ecomob_ht: Option<Decimal>,
    /// Corrigée à la main (le, par).
    corrigee_le: Option<String>,
    corrigee_par: Option<String>,
}

#[derive(Serialize)]
pub struct LignesDocument {
    /// Les lignes peuvent être corrigées par cet utilisateur maintenant.
    modifiable: bool,
    /// Sinon pourquoi (« déjà envoyée à Dext »…).
    raison: Option<String>,
    r#type: Option<String>,
    lignes: Vec<LigneEdit>,
}

/// Valeurs saisies d'une ligne (toutes : la ligne est remplacée telle quelle).
#[derive(Debug, Deserialize, Default, Clone)]
pub struct LigneSaisie {
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    designation: Option<String>,
    #[serde(default)]
    quantite: Option<Decimal>,
    #[serde(default)]
    unite: Option<String>,
    #[serde(default)]
    prix_unitaire_ht: Option<Decimal>,
    #[serde(default)]
    montant_ht: Option<Decimal>,
    #[serde(default)]
    gencod: Option<String>,
    #[serde(default)]
    colis: Option<Decimal>,
    #[serde(default)]
    pcb: Option<Decimal>,
    #[serde(default)]
    remise_taux: Option<Decimal>,
    #[serde(default)]
    taux_tva: Option<Decimal>,
    #[serde(default)]
    deee_ht: Option<Decimal>,
    #[serde(default)]
    ecomob_ht: Option<Decimal>,
}

fn champ(champ: &'static str, message: impl Into<String>) -> ErreurApi {
    ErreurApi::Champ {
        champ,
        message: message.into(),
    }
}

fn texte(v: Option<String>) -> Option<String> {
    v.map(|t| t.trim().chars().take(TEXTE_MAX).collect::<String>())
        .filter(|t| !t.is_empty())
}

/// Nombre saisi positif, au plus `max`, arrondi à la précision de sa colonne (`colonne` :
/// `montant::quantite`, `montant::prix`…) ; trop grand pour la colonne : refusé.
fn positif(
    nom: &'static str,
    v: Option<Decimal>,
    max: Option<Decimal>,
    colonne: fn(Decimal) -> Option<Decimal>,
) -> Resultat<Option<Decimal>> {
    match v {
        Some(x) if x.is_sign_negative() && !x.is_zero() => {
            Err(champ(nom, "nombre positif attendu"))
        }
        Some(x) if max.is_some_and(|m| x > m) => Err(champ(
            nom,
            format!("{} au plus", max.unwrap_or_default().normalize()),
        )),
        Some(x) => colonne(x)
            .map(Some)
            .ok_or_else(|| champ(nom, "nombre trop grand")),
        None => Ok(None),
    }
}

impl LigneSaisie {
    /// Valeurs nettoyées et vérifiées ; erreur sur le premier champ en faute.
    fn verifier(self) -> Resultat<Self> {
        let gencod = texte(self.gencod).map(|g| g.split_whitespace().collect::<String>());
        if let Some(g) = &gencod
            && !gencod_valide(g)
        {
            return Err(champ(
                "gencod",
                "gencod invalide : 8, 12, 13 ou 14 chiffres avec une clé juste, ou vide",
            ));
        }
        let l = Self {
            reference: texte(self.reference),
            designation: texte(self.designation),
            quantite: positif("quantite", self.quantite, None, montant::quantite)?,
            unite: texte(self.unite),
            prix_unitaire_ht: positif(
                "prix_unitaire_ht",
                self.prix_unitaire_ht,
                None,
                montant::prix,
            )?,
            montant_ht: positif("montant_ht", self.montant_ht, None, montant::montant)?,
            gencod,
            colis: positif("colis", self.colis, None, montant::quantite)?,
            pcb: positif("pcb", self.pcb, None, montant::quantite)?,
            remise_taux: positif(
                "remise_taux",
                self.remise_taux,
                Some(Decimal::ONE_HUNDRED),
                montant::taux_ligne,
            )?,
            taux_tva: positif(
                "taux_tva",
                self.taux_tva,
                Some(Decimal::ONE_HUNDRED),
                montant::taux,
            )?,
            deee_ht: positif("deee_ht", self.deee_ht, None, montant::prix)?,
            ecomob_ht: positif("ecomob_ht", self.ecomob_ht, None, montant::prix)?,
        };
        if l.reference.is_none() && l.designation.is_none() {
            return Err(champ("designation", "désignation ou référence attendue"));
        }
        Ok(l)
    }

    fn ligne(&self) -> Ligne {
        Ligne {
            reference: self.reference.clone(),
            designation: self.designation.clone(),
            quantite: self.quantite,
            unite: self.unite.clone(),
            prix_unitaire_ht: self.prix_unitaire_ht,
            montant_ht: self.montant_ht,
            gencod: self.gencod.clone(),
            colis: self.colis,
            pcb: self.pcb,
            remise_taux: self.remise_taux,
            taux_tva: self.taux_tva,
            deee_ht: self.deee_ht,
            ecomob_ht: self.ecomob_ht,
            pv_conseille: None,
        }
    }
}

/// Montant attendu d'une ligne : quantité (sinon colis × PCB) × prix, moins la remise.
fn montant_calcule(l: &Ligne) -> Option<Decimal> {
    let q = l.quantite.or(match (l.colis, l.pcb) {
        (Some(c), Some(p)) => Some(c * p),
        _ => None,
    })?;
    let remise = l
        .remise_taux
        .filter(|r| *r > Decimal::ZERO && *r < Decimal::ONE_HUNDRED)
        .unwrap_or_default();
    montant::montant(q * l.prix_unitaire_ht? * (Decimal::ONE - remise / Decimal::ONE_HUNDRED))
}

/// Quantité, prix, colis, PCB ou remise changés sans toucher au montant : le montant suit, s'il
/// était absent ou cohérent avec les anciennes valeurs (sinon il était imprimé autrement, on le
/// garde).
fn montant_suivi(avant: &Ligne, apres: &mut Ligne) {
    let base_changee = avant.quantite != apres.quantite
        || avant.prix_unitaire_ht != apres.prix_unitaire_ht
        || avant.colis != apres.colis
        || avant.pcb != apres.pcb
        || avant.remise_taux != apres.remise_taux;
    if !base_changee || avant.montant_ht != apres.montant_ht {
        return;
    }
    let coherent = match (avant.montant_ht, montant_calcule(avant)) {
        (None, _) => true,
        (Some(m), Some(c)) => (m - c).abs() <= TOLERANCE,
        (Some(_), None) => avant.prix_unitaire_ht.is_none(),
    };
    if coherent && let Some(m) = montant_calcule(apres) {
        apres.montant_ht = Some(m);
    }
}

struct Piece {
    r#type: String,
    bl: bool,
}

/// Le document peut-il voir ses lignes corrigées ? Document verrouillé par l'appelant quand
/// `c` est dans une transaction.
async fn raison_figee(c: &mut PgConnection, id: i64) -> Resultat<(Option<String>, Piece)> {
    let d = sqlx::query!(
        r#"select etat, type, statut_depot,
                  exists (select 1 from envoi_prevu p where p.document_id = d.id
                          and p.etat = 'attente') as "attente!"
           from document d where id = $1"#,
        id
    )
    .fetch_one(&mut *c)
    .await?;
    let t = d.r#type.clone().unwrap_or_default();
    let piece = Piece {
        bl: t == "bl",
        r#type: t,
    };
    let raison = if d.etat != "lu" || !["facture", "avoir", "bl"].contains(&piece.r#type.as_str()) {
        Some("seules les lignes d'une facture, d'un avoir ou d'un BL lus se corrigent".into())
    } else if matches!(d.statut_depot.as_deref(), Some("depose" | "en_envoi")) {
        Some("pièce déjà envoyée : ses lignes ne changent plus".into())
    } else if d.attente {
        Some("envoi en cours de préparation : annulez la validation pour corriger".into())
    } else {
        None
    };
    Ok((raison, piece))
}

async fn lire_lignes(c: &mut PgConnection, id: i64) -> sqlx::Result<Vec<LigneEdit>> {
    let r = sqlx::query!(
        r#"select l.id, l.rang, l.reference, l.designation, l.quantite, l.unite,
                  l.prix_unitaire_ht as prix, l.montant_ht as montant, l.gencod, l.colis, l.pcb,
                  l.remise_taux as remise, l.taux_tva as tva, l.deee_ht as deee,
                  l.ecomob_ht as ecomob,
                  to_char(l.corrigee_le at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') as corrigee_le,
                  u.identifiant as "corrigee_par?"
           from ligne_document l left join utilisateur u on u.id = l.corrigee_par
           where l.document_id = $1 order by l.rang"#,
        id
    )
    .fetch_all(&mut *c)
    .await?;
    Ok(r.into_iter()
        .map(|l| LigneEdit {
            id: l.id,
            rang: l.rang,
            reference: l.reference,
            designation: l.designation,
            quantite: l.quantite,
            unite: l.unite,
            prix_unitaire_ht: l.prix,
            montant_ht: l.montant,
            gencod: l.gencod,
            colis: l.colis,
            pcb: l.pcb,
            remise_taux: l.remise,
            taux_tva: l.tva,
            deee_ht: l.deee,
            ecomob_ht: l.ecomob,
            corrigee_le: l.corrigee_le,
            corrigee_par: l.corrigee_par,
        })
        .collect())
}

fn vers_ligne(l: &LigneEdit) -> Ligne {
    Ligne {
        reference: l.reference.clone(),
        designation: l.designation.clone(),
        quantite: l.quantite,
        unite: l.unite.clone(),
        prix_unitaire_ht: l.prix_unitaire_ht,
        montant_ht: l.montant_ht,
        gencod: l.gencod.clone(),
        colis: l.colis,
        pcb: l.pcb,
        remise_taux: l.remise_taux,
        taux_tva: l.taux_tva,
        deee_ht: l.deee_ht,
        ecomob_ht: l.ecomob_ht,
        pv_conseille: None,
    }
}

fn instantane(l: &Ligne) -> Value {
    json!({
        "reference": l.reference, "designation": l.designation, "quantite": l.quantite,
        "unite": l.unite, "prix_unitaire_ht": l.prix_unitaire_ht, "montant_ht": l.montant_ht,
        "gencod": l.gencod, "colis": l.colis, "pcb": l.pcb, "remise_taux": l.remise_taux,
        "taux_tva": l.taux_tva, "deee_ht": l.deee_ht, "ecomob_ht": l.ecomob_ht,
    })
}

/// `GET /api/documents/{id}/lignes`.
pub async fn liste(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<LignesDocument>> {
    controler_visible(&etat, &acces, id).await?;
    let droits = controler_modifiable(&etat, &acces, id).await;
    let mut c = etat.pool.acquire().await?;
    let (raison, piece) = raison_figee(&mut c, id).await?;
    let raison = match droits {
        Err(_) => Some("pièce d'une autre société".into()),
        Ok(()) => raison,
    };
    Ok(Json(LignesDocument {
        modifiable: raison.is_none(),
        raison,
        r#type: Some(piece.r#type).filter(|t| !t.is_empty()),
        lignes: lire_lignes(&mut c, id).await?,
    }))
}

#[derive(Serialize)]
pub struct Totaux {
    montant_ht: Option<Decimal>,
    montant_tva: Option<Decimal>,
    montant_ttc: Option<Decimal>,
    /// Totaux du BL calculés depuis les lignes (« ht », « tva », « ttc »).
    montants_calcules: Vec<String>,
}

#[derive(Serialize)]
pub struct ResultatLigne {
    /// La ligne après la correction (absente après une suppression).
    ligne: Option<LigneEdit>,
    totaux: Totaux,
    /// Recherche des articles au magasin relancée (export Phénix).
    magasin_relance: bool,
}

enum Changement {
    Modifier(i64, LigneSaisie),
    Ajouter(LigneSaisie),
    Supprimer(i64),
}

/// BL dont les totaux viennent des lignes (ou manquent) : HT, TVA et TTC calculés de nouveau
/// (`extraction::completer_montants_bl`), montants des lignes sans montant compris. Rien pour
/// un BL aux totaux imprimés : le contrôle « somme des lignes » dira l'écart.
async fn totaux_bl(c: &mut PgConnection, id: i64) -> sqlx::Result<()> {
    let d = sqlx::query!(
        r#"select montant_ht as ht, montant_tva as tva, montant_ttc as ttc,
                  coalesce(confiance->'montants_calcules', '[]') as "calcules!",
                  coalesce((confiance->>'lignes_tronquees')::boolean, false) as "tronque!"
           from document where id = $1"#,
        id
    )
    .fetch_one(&mut *c)
    .await?;
    let calcules: Vec<String> = serde_json::from_value(d.calcules).unwrap_or_default();
    let manque = d.ht.is_none() || d.tva.is_none() || d.ttc.is_none();
    if d.tronque || (calcules.is_empty() && !manque) {
        return Ok(());
    }
    let lignes = lire_lignes(c, id).await?;
    let mut l: Vec<Ligne> = lignes.iter().map(vers_ligne).collect();
    let garde = |nom: &str, v: Option<Decimal>| v.filter(|_| !calcules.iter().any(|x| x == nom));
    let (mut ht, mut tva, mut ttc) = (garde("ht", d.ht), garde("tva", d.tva), garde("ttc", d.ttc));
    let nouveaux = extraction::completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc);
    for (avant, apres) in lignes.iter().zip(&l) {
        if avant.montant_ht.is_none() && apres.montant_ht.is_some() {
            sqlx::query!(
                "update ligne_document set montant_ht = $2::numeric(14, 2) where id = $1",
                avant.id,
                apres.montant_ht
            )
            .execute(&mut *c)
            .await?;
        }
    }
    sqlx::query!(
        r#"update document set montant_ht = $2::numeric(14, 2),
               montant_tva = $3::numeric(14, 2), montant_ttc = $4::numeric(14, 2),
               confiance = coalesce(confiance, '{}'::jsonb) || jsonb_build_object('montants_calcules', $5::jsonb)
           where id = $1"#,
        id,
        ht,
        tva,
        ttc,
        json!(nouveaux)
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn appliquer(
    acces: Acces,
    etat: Etat,
    id: i64,
    ch: Changement,
) -> Resultat<(StatusCode, Json<ResultatLigne>)> {
    controler_modifiable(&etat, &acces, id).await?;
    let mut tx = etat.pool.begin().await?;
    sqlx::query!("select id from document where id = $1 for update", id)
        .fetch_one(&mut *tx)
        .await?;
    let (raison, piece) = raison_figee(&mut tx, id).await?;
    if let Some(r) = raison {
        return Err(ErreurApi::ConflitCode {
            code: "etat",
            message: r,
        });
    }
    let avant_lignes = lire_lignes(&mut tx, id).await?;
    let trouver = |ligne_id: i64| {
        avant_lignes
            .iter()
            .find(|l| l.id == ligne_id)
            .ok_or(ErreurApi::Absent)
    };
    // Identité de l'article changée : la recherche au magasin est à refaire.
    let supprimee = match &ch {
        Changement::Supprimer(i) => Some(*i),
        _ => None,
    };
    let (ligne_id, rang, action, avant, apres, identite) = match ch {
        Changement::Modifier(ligne_id, s) => {
            let a = vers_ligne(trouver(ligne_id)?);
            let mut n = s.verifier()?.ligne();
            montant_suivi(&a, &mut n);
            let identite = a.gencod != n.gencod
                || a.reference != n.reference
                || a.designation != n.designation;
            sqlx::query!(
                r#"update ligne_document set reference = $2, designation = $3,
                       quantite = $4::numeric(14, 3), unite = $5,
                       prix_unitaire_ht = $6::numeric(14, 4),
                       montant_ht = $7::numeric(14, 2), gencod = $8,
                       colis = $9::numeric(14, 3), pcb = $10::numeric(14, 3),
                       remise_taux = $11::numeric(8, 4), taux_tva = $12::numeric(6, 3),
                       deee_ht = $13::numeric(14, 4), ecomob_ht = $14::numeric(14, 4),
                       magasin = case when $15 then null else magasin end,
                       corrigee_le = now(), corrigee_par = $16
                   where id = $1"#,
                ligne_id,
                n.reference,
                n.designation,
                n.quantite,
                n.unite,
                n.prix_unitaire_ht,
                n.montant_ht,
                n.gencod,
                n.colis,
                n.pcb,
                n.remise_taux,
                n.taux_tva,
                n.deee_ht,
                n.ecomob_ht,
                identite,
                acces.utilisateur_id
            )
            .execute(&mut *tx)
            .await?;
            let rang = trouver(ligne_id)?.rang;
            (
                Some(ligne_id),
                rang,
                "ligne_corrigee",
                Some(a),
                Some(n),
                identite,
            )
        }
        Changement::Ajouter(s) => {
            let mut n = s.verifier()?.ligne();
            if n.montant_ht.is_none() {
                n.montant_ht = montant_calcule(&n);
            }
            let rang = avant_lignes
                .iter()
                .map(|l| l.rang)
                .max()
                .map_or(0, |r| r + 1);
            let ligne_id = sqlx::query_scalar!(
                r#"insert into ligne_document (document_id, rang, reference, designation, quantite,
                       unite, prix_unitaire_ht, montant_ht, gencod, colis, pcb, remise_taux,
                       taux_tva, deee_ht, ecomob_ht, corrigee_le, corrigee_par)
                   values ($1, $2, $3, $4, $5::numeric(14, 3), $6,
                           $7::numeric(14, 4), $8::numeric(14, 2), $9,
                           $10::numeric(14, 3), $11::numeric(14, 3),
                           $12::numeric(8, 4), $13::numeric(6, 3),
                           $14::numeric(14, 4), $15::numeric(14, 4), now(), $16)
                   returning id"#,
                id,
                rang,
                n.reference,
                n.designation,
                n.quantite,
                n.unite,
                n.prix_unitaire_ht,
                n.montant_ht,
                n.gencod,
                n.colis,
                n.pcb,
                n.remise_taux,
                n.taux_tva,
                n.deee_ht,
                n.ecomob_ht,
                acces.utilisateur_id
            )
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query!("update document set lignes_lues = true where id = $1", id)
                .execute(&mut *tx)
                .await?;
            (Some(ligne_id), rang, "ligne_ajoutee", None, Some(n), true)
        }
        Changement::Supprimer(ligne_id) => {
            let l = trouver(ligne_id)?;
            let (a, rang) = (vers_ligne(l), l.rang);
            sqlx::query!("delete from ligne_document where id = $1", ligne_id)
                .execute(&mut *tx)
                .await?;
            (None, rang, "ligne_supprimee", Some(a), None, true)
        }
    };
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        action,
        "document",
        Some(id.to_string()),
        json!({
            "ligne": ligne_id.or(supprimee), "rang": rang,
            "avant": avant.as_ref().map(instantane), "apres": apres.as_ref().map(instantane),
        }),
    )
    .await?;
    if piece.bl {
        totaux_bl(&mut tx, id).await?;
    }
    tx.commit().await?;

    // Ce qui dépend des lignes, sans Claude.
    if !piece.bl {
        depot::evaluer(&etat.pool, id)
            .await
            .map_err(ErreurApi::Interne)?;
    }
    rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    let magasin_relance = piece.bl
        && identite
        && parametre::lire(&etat.pool, serveur_api::CLE)
            .await?
            .is_some();
    if magasin_relance {
        articles_magasin::demander(&etat.pool, id)
            .await
            .map_err(ErreurApi::Interne)?;
    }

    let mut c = etat.pool.acquire().await?;
    let ligne = match ligne_id {
        Some(i) => lire_lignes(&mut c, id)
            .await?
            .into_iter()
            .find(|l| l.id == i),
        None => None,
    };
    let t = sqlx::query!(
        r#"select montant_ht as ht, montant_tva as tva, montant_ttc as ttc,
                  coalesce(confiance->'montants_calcules', '[]') as "calcules!"
           from document where id = $1"#,
        id
    )
    .fetch_one(&mut *c)
    .await?;
    let code = if matches!(action, "ligne_ajoutee") {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        code,
        Json(ResultatLigne {
            ligne,
            totaux: Totaux {
                montant_ht: t.ht,
                montant_tva: t.tva,
                montant_ttc: t.ttc,
                montants_calcules: serde_json::from_value(t.calcules).unwrap_or_default(),
            },
            magasin_relance,
        }),
    ))
}

/// `PUT /api/documents/{id}/lignes/{ligne}` : la ligne prend les valeurs saisies.
pub async fn modifier(
    acces: Acces,
    State(etat): State<Etat>,
    Path((id, ligne)): Path<(i64, i64)>,
    Json(s): Json<LigneSaisie>,
) -> Resultat<(StatusCode, Json<ResultatLigne>)> {
    appliquer(acces, etat, id, Changement::Modifier(ligne, s)).await
}

/// `POST /api/documents/{id}/lignes` : ligne ajoutée à la fin.
pub async fn ajouter(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(s): Json<LigneSaisie>,
) -> Resultat<(StatusCode, Json<ResultatLigne>)> {
    appliquer(acces, etat, id, Changement::Ajouter(s)).await
}

/// `DELETE /api/documents/{id}/lignes/{ligne}`.
pub async fn supprimer(
    acces: Acces,
    State(etat): State<Etat>,
    Path((id, ligne)): Path<(i64, i64)>,
) -> Resultat<(StatusCode, Json<ResultatLigne>)> {
    appliquer(acces, etat, id, Changement::Supprimer(ligne)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_montant_suit_la_quantite_s_il_etait_coherent() {
        let avant = Ligne {
            quantite: Some(dec!(2.0)),
            prix_unitaire_ht: Some(dec!(5.0)),
            montant_ht: Some(dec!(10.0)),
            ..Default::default()
        };
        let mut apres = Ligne {
            quantite: Some(dec!(3.0)),
            ..avant.clone()
        };
        montant_suivi(&avant, &mut apres);
        assert_eq!(apres.montant_ht, Some(dec!(15.0)));
        // Montant imprimé autrement (prix au colis) : gardé.
        let avant = Ligne {
            montant_ht: Some(dec!(60.0)),
            ..avant
        };
        let mut apres = Ligne {
            quantite: Some(dec!(3.0)),
            ..avant.clone()
        };
        montant_suivi(&avant, &mut apres);
        assert_eq!(apres.montant_ht, Some(dec!(60.0)));
    }

    #[test]
    fn saisie_verifiee() {
        let ok = LigneSaisie {
            designation: Some(" Seau ".into()),
            gencod: Some("3 532156 987418".into()),
            quantite: Some(dec!(2.0)),
            ..Default::default()
        }
        .verifier()
        .unwrap();
        assert_eq!(ok.designation.as_deref(), Some("Seau"));
        assert_eq!(ok.gencod.as_deref(), Some("3532156987418"));
        let faux = LigneSaisie {
            designation: Some("Seau".into()),
            gencod: Some("3532156987419".into()),
            ..Default::default()
        };
        assert!(matches!(
            faux.verifier(),
            Err(ErreurApi::Champ {
                champ: "gencod",
                ..
            })
        ));
        let negatif = LigneSaisie {
            designation: Some("Seau".into()),
            pcb: Some(dec!(-1.0)),
            ..Default::default()
        };
        assert!(matches!(
            negatif.verifier(),
            Err(ErreurApi::Champ { champ: "pcb", .. })
        ));
    }

    #[test]
    fn saisie_arrondie_a_la_colonne_et_montant_exact() {
        let l = LigneSaisie {
            designation: Some("Seau".into()),
            quantite: Some(dec!(1.23456)),
            prix_unitaire_ht: Some(dec!(3.33333)),
            ..Default::default()
        }
        .verifier()
        .unwrap();
        assert_eq!(l.quantite, Some(dec!(1.235)));
        assert_eq!(l.prix_unitaire_ht, Some(dec!(3.3333)));
        let trop = LigneSaisie {
            designation: Some("Seau".into()),
            montant_ht: Some(dec!(1000000000000)),
            ..Default::default()
        };
        assert!(matches!(
            trop.verifier(),
            Err(ErreurApi::Champ {
                champ: "montant_ht",
                ..
            })
        ));
        // 3 × 3,335 = 10,005 → 10,01 (10,00 en f64).
        let l = Ligne {
            quantite: Some(dec!(3)),
            prix_unitaire_ht: Some(dec!(3.335)),
            ..Default::default()
        };
        assert_eq!(montant_calcule(&l), Some(dec!(10.01)));
    }
}
