//! Export d'un BL vers BO Phénix : état de la recherche des articles au magasin, alertes,
//! et les deux fichiers (matrice d'intégration de la commande, trame des fiches produits).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use gmflow_core::export_phenix::{self, Alerte, Colonnes, Export, LigneBl};
use gmflow_core::{articles_magasin, serveur_api};
use serde::Serialize;
use time::OffsetDateTime;

use super::documents::controler_visible;
use super::fichiers::{CSP_TELECHARGEMENT, disposition};
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Fichier {
    pub rangees: usize,
    pub alertes: Vec<Alerte>,
}

#[derive(Serialize)]
pub struct EtatExport {
    /// Serveur du magasin configuré : sans lui, ni lots détaillés ni codes article.
    pub serveur: bool,
    pub en_cours: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub verifie_le: Option<OffsetDateTime>,
    pub erreur: Option<String>,
    /// Les fichiers peuvent être téléchargés (recherche faite sans erreur, ou pas de serveur).
    pub pret: bool,
    pub code_fournisseur: Option<String>,
    /// Colonnes d'identification reprises du BL (réglage de la fiche fournisseur).
    pub colonnes: Colonnes,
    pub lignes: usize,
    /// Lignes cherchées au magasin : celles qui ressemblent à un lot.
    pub cherchees: usize,
    pub trouves: usize,
    pub lots: usize,
    pub composants: usize,
    /// Aucun gencod ni colisage lu : BL lu avant l'export, à relire.
    pub a_relire: bool,
    pub matrice: Fichier,
    pub trame: Fichier,
}

struct Donnees {
    numero: Option<String>,
    fournisseur: Option<String>,
    lignes: Vec<LigneBl>,
    code: Option<String>,
    colonnes: Colonnes,
    etat: EtatExport,
}

fn fichier(e: &Export) -> Fichier {
    Fichier {
        rangees: e.rangees.len(),
        alertes: e.alertes.clone(),
    }
}

async fn charger(etat: &Etat, acces: &Acces, id: i64) -> Resultat<Donnees> {
    controler_visible(etat, acces, id).await?;
    let d = sqlx::query!(
        r#"select d.type, d.numero, coalesce(f.nom, d.fournisseur_nom) as fournisseur,
                  d.magasin_demande_le, d.magasin_verifie_le, d.magasin_erreur,
                  coalesce(f.colonnes_export, g.colonnes_export) as colonnes,
                  coalesce((d.confiance->>'lignes_tronquees')::boolean, false) as "tronque!"
           from document d left join fournisseur f on f.id = d.fournisseur_id
           left join fournisseur g on g.id = f.groupe_id
           where d.id = $1"#,
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    if d.r#type.as_deref() != Some("bl") {
        return Err(ErreurApi::Donnees(
            "l'export vers Phénix ne concerne que les bons de livraison".into(),
        ));
    }
    let serveur = gmflow_core::parametre::lire(&etat.pool, serveur_api::CLE)
        .await?
        .is_some();
    let lignes = export_phenix::lignes(&etat.pool, id).await?;
    let code = articles_magasin::codes_du_document(&etat.pool, id)
        .await?
        .into_iter()
        .next();
    let en_cours = d
        .magasin_demande_le
        .is_some_and(|dem| d.magasin_verifie_le.is_none_or(|v| v < dem));
    let pret =
        !serveur || (!en_cours && d.magasin_verifie_le.is_some() && d.magasin_erreur.is_none());
    let colonnes = Colonnes::depuis(d.colonnes.as_deref());
    let mut m = export_phenix::matrice(&lignes, colonnes);
    let mut t = export_phenix::trame(&lignes, code.as_deref(), colonnes);
    if d.tronque {
        let a = Alerte {
            rang: None,
            article: None,
            message: "BL trop long pour être lu en entier : des articles manquent dans les fichiers, à compléter dans Phénix".into(),
        };
        m.alertes.insert(0, a.clone());
        t.alertes.insert(0, a);
    }
    if !serveur {
        let a = Alerte {
            rang: None,
            article: None,
            message: "serveur du magasin non configuré (Réglages > Serveur du magasin) : lots non détaillés, codes article absents".into(),
        };
        m.alertes.insert(0, a.clone());
        t.alertes.insert(0, a);
    }
    let a_relire = !lignes.is_empty()
        && lignes
            .iter()
            .all(|l| l.gencod.is_none() && l.pcb.is_none() && l.colis.is_none());
    let etat_export = EtatExport {
        serveur,
        en_cours,
        verifie_le: d.magasin_verifie_le,
        erreur: d.magasin_erreur,
        pret,
        code_fournisseur: code.clone(),
        colonnes,
        lignes: lignes.len(),
        cherchees: lignes
            .iter()
            .filter(|l| articles_magasin::semble_lot(l.designation.as_deref()))
            .count(),
        trouves: lignes.iter().filter(|l| l.magasin.is_some()).count(),
        lots: lignes
            .iter()
            .filter(|l| l.magasin.as_ref().is_some_and(|a| !a.composants.is_empty()))
            .count(),
        composants: lignes
            .iter()
            .filter_map(|l| l.magasin.as_ref())
            .map(|a| a.composants.len())
            .sum(),
        a_relire,
        matrice: fichier(&m),
        trame: fichier(&t),
    };
    Ok(Donnees {
        numero: d.numero,
        fournisseur: d.fournisseur,
        lignes,
        code,
        colonnes,
        etat: etat_export,
    })
}

pub async fn etat(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<EtatExport>> {
    Ok(Json(charger(&etat, &acces, id).await?.etat))
}

/// Relance la recherche des articles au magasin.
pub async fn verifier(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    charger(&etat, &acces, id).await?;
    if gmflow_core::parametre::lire(&etat.pool, serveur_api::CLE)
        .await?
        .is_none()
    {
        return Err(ErreurApi::Donnees(
            "serveur du magasin non configuré (Réglages > Serveur du magasin)".into(),
        ));
    }
    articles_magasin::demander(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(StatusCode::ACCEPTED)
}

/// Nom de fichier : modèle, fournisseur et n° du BL, sans caractère gênant.
fn nom(modele: &str, d: &Donnees) -> String {
    let mut n = modele.to_string();
    for p in [
        d.fournisseur.as_deref(),
        d.numero.as_deref().map(|x| x.trim()),
    ]
    .into_iter()
    .flatten()
    {
        n.push_str(" - ");
        n.push_str(p);
    }
    let n: String = n
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    format!("{}.xlsx", n.trim())
}

pub(crate) fn telecharger(nom: String, octets: Vec<u8>) -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
            ),
            (header::CONTENT_DISPOSITION, disposition(&nom)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                CSP_TELECHARGEMENT.to_string(),
            ),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
        ],
        octets,
    )
        .into_response()
}

async fn pret(etat: &Etat, acces: &Acces, id: i64) -> Resultat<Donnees> {
    let d = charger(etat, acces, id).await?;
    if !d.etat.pret {
        return Err(ErreurApi::Conflit(
            if d.etat.en_cours {
                "recherche des articles au magasin en cours, réessayer dans un instant"
            } else {
                "les articles du BL n'ont pas encore été retrouvés au magasin (lots non détaillés)"
            }
            .into(),
        ));
    }
    if d.lignes.is_empty() {
        return Err(ErreurApi::Conflit(
            "aucune ligne article lue sur ce BL".into(),
        ));
    }
    Ok(d)
}

pub async fn matrice(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Response> {
    let d = pret(&etat, &acces, id).await?;
    let e = export_phenix::matrice(&d.lignes, d.colonnes);
    let octets = export_phenix::xlsx_matrice(&e).map_err(|e| ErreurApi::Interne(e.into()))?;
    Ok(telecharger(
        nom("MATRICE INTEGRATION BACKOFFICE", &d),
        octets,
    ))
}

pub async fn trame(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Response> {
    let d = pret(&etat, &acces, id).await?;
    let e = export_phenix::trame(&d.lignes, d.code.as_deref(), d.colonnes);
    let octets = export_phenix::xlsx_trame(&e).map_err(|e| ErreurApi::Interne(e.into()))?;
    Ok(telecharger(nom("TRAME BO PHENIX IMPORT", &d), octets))
}
