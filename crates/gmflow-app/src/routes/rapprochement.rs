//! Rapprochement des BL et des factures : vue par fournisseur et par période (BL sans
//! facture, factures sans BL, écarts), détail d'un document (lignes, liens, candidats), liens
//! manuels et « pas de correspondance attendue ». Un responsable ne voit et ne modifie que sa
//! structure.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use gmflow_core::rapprochement::{self as rap, ErreurLien, SEUIL_MONTANT};
use gmflow_core::{file, journal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::documents::{controler_modifiable, controler_visible};
use super::mails::date;
use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

/// Documents examinés au plus par la vue (les plus récents).
const DOCUMENTS_MAX: i64 = 5000;
/// Éléments rendus au plus par liste.
const LISTE_MAX: usize = 300;

#[derive(Deserialize)]
pub struct Filtres {
    societe: Option<i64>,
    fournisseur: Option<i64>,
    du: Option<String>,
    au: Option<String>,
}

#[derive(Serialize, Clone, sqlx::FromRow)]
pub struct DocRap {
    id: i64,
    r#type: String,
    numero: Option<String>,
    #[serde(with = "date_iso::option")]
    date_document: Option<time::Date>,
    numero_commande: Option<String>,
    montant_ht: Option<f64>,
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    societe: Option<String>,
    nom_fichier: String,
}

#[derive(Serialize)]
pub struct BlLie {
    lien_id: i64,
    methode: String,
    #[serde(flatten)]
    bl: DocRap,
}

#[derive(Serialize)]
pub struct Rapproche {
    facture: DocRap,
    bls: Vec<BlLie>,
    ecart_montant: Option<f64>,
    ecarts: Value,
}

#[derive(Serialize, Default)]
pub struct ResumeFournisseur {
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    bls: i64,
    factures: i64,
    bl_sans_facture: i64,
    factures_sans_bl: i64,
    factures_avec_ecart: i64,
}

#[derive(Serialize)]
pub struct Vue {
    resume: Vec<ResumeFournisseur>,
    bl_sans_facture: Vec<DocRap>,
    factures_sans_bl: Vec<DocRap>,
    avec_ecart: Vec<Rapproche>,
    sans_ecart: Vec<Rapproche>,
    /// Plus de `DOCUMENTS_MAX` documents : seuls les plus récents sont examinés.
    tronque: bool,
    /// BL lus avant le lot 4, sans lignes (à relire pour comparer les quantités).
    bl_sans_lignes: i64,
}

struct Lien {
    id: i64,
    facture_id: i64,
    bl_id: i64,
    methode: String,
    ecart_montant: Option<f64>,
    ecarts: Value,
}

fn avec_ecart(ecart: Option<f64>, ecarts: &Value) -> bool {
    ecart.is_some_and(|e| e.abs() > SEUIL_MONTANT)
        || ecarts.as_array().is_some_and(|a| !a.is_empty())
}

/// Vue d'ensemble du rapprochement sur une période.
pub async fn vue(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Vue>> {
    let (du, au) = (date(&f.du)?, date(&f.au)?);
    let docs = sqlx::query_as!(
        DocRap,
        r#"select d.id, d.type as "type!", d.numero, d.date_document, d.numero_commande,
                  d.montant_ht::float8 as montant_ht, d.fournisseur_id, fo.nom as "fournisseur?",
                  s.raison_sociale as "societe?", p.nom as nom_fichier
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.etat = 'lu' and d.type in ('facture', 'bl') and not d.rapprochement_ignore
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2)
             and ($3::bigint is null or d.fournisseur_id = $3)
             and ($4::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $4)
             and ($5::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) <= $5)
           order by coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) desc, d.id desc
           limit $6"#,
        acces.societe_filtre(),
        f.societe,
        f.fournisseur,
        du,
        au,
        DOCUMENTS_MAX + 1
    )
    .fetch_all(&etat.pool)
    .await?;
    let tronque = docs.len() as i64 > DOCUMENTS_MAX;
    let docs: Vec<DocRap> = docs.into_iter().take(DOCUMENTS_MAX as usize).collect();
    let ids: Vec<i64> = docs.iter().map(|d| d.id).collect();
    let liens: Vec<Lien> = sqlx::query_as!(
        Lien,
        "select id, facture_id, bl_id, methode, ecart_montant::float8 as ecart_montant, ecarts
         from rapprochement where not rejete and (facture_id = any($1) or bl_id = any($1))
         order by facture_id, bl_id",
        &ids
    )
    .fetch_all(&etat.pool)
    .await?;
    let par_id: HashMap<i64, &DocRap> = docs.iter().map(|d| (d.id, d)).collect();
    let mut lies_bl: HashMap<i64, usize> = HashMap::new();
    let mut par_facture: BTreeMap<i64, Vec<&Lien>> = BTreeMap::new();
    for l in &liens {
        *lies_bl.entry(l.bl_id).or_default() += 1;
        par_facture.entry(l.facture_id).or_default().push(l);
    }
    // Résumé par fournisseur, dans l'ordre des noms.
    let mut resume: BTreeMap<(String, Option<i64>), ResumeFournisseur> = BTreeMap::new();
    let mut bl_sans_facture = Vec::new();
    let mut factures_sans_bl = Vec::new();
    let (mut avec, mut sans) = (Vec::new(), Vec::new());
    for d in &docs {
        let cle = (
            d.fournisseur.clone().unwrap_or_default().to_lowercase(),
            d.fournisseur_id,
        );
        let r = resume.entry(cle).or_insert_with(|| ResumeFournisseur {
            fournisseur_id: d.fournisseur_id,
            fournisseur: d.fournisseur.clone(),
            ..Default::default()
        });
        if d.r#type == "bl" {
            r.bls += 1;
            if !lies_bl.contains_key(&d.id) {
                r.bl_sans_facture += 1;
                bl_sans_facture.push(d.clone());
            }
            continue;
        }
        r.factures += 1;
        let Some(ls) = par_facture.get(&d.id) else {
            r.factures_sans_bl += 1;
            factures_sans_bl.push(d.clone());
            continue;
        };
        let (ecart, ecarts) = (ls[0].ecart_montant, ls[0].ecarts.clone());
        let bls = ls
            .iter()
            .map(|l| BlLie {
                lien_id: l.id,
                methode: l.methode.clone(),
                // BL hors période ou hors filtre : rendu avec le minimum.
                bl: par_id
                    .get(&l.bl_id)
                    .map(|b| (*b).clone())
                    .unwrap_or(DocRap {
                        id: l.bl_id,
                        r#type: "bl".into(),
                        numero: None,
                        date_document: None,
                        numero_commande: None,
                        montant_ht: None,
                        fournisseur_id: d.fournisseur_id,
                        fournisseur: d.fournisseur.clone(),
                        societe: None,
                        nom_fichier: String::new(),
                    }),
            })
            .collect();
        let element = Rapproche {
            facture: d.clone(),
            bls,
            ecart_montant: ecart,
            ecarts,
        };
        if avec_ecart(ecart, &element.ecarts) {
            r.factures_avec_ecart += 1;
            avec.push(element);
        } else {
            sans.push(element);
        }
    }
    let bl_sans_lignes = sqlx::query_scalar!(
        r#"select count(*) as "n!" from document d
           where d.type = 'bl' and d.etat = 'lu' and not d.lignes_lues
             and ($1::bigint is null or d.societe_id = $1)"#,
        acces.societe_filtre()
    )
    .fetch_one(&etat.pool)
    .await?;
    bl_sans_facture.truncate(LISTE_MAX);
    factures_sans_bl.truncate(LISTE_MAX);
    avec.truncate(LISTE_MAX);
    sans.truncate(LISTE_MAX);
    Ok(Json(Vue {
        resume: resume.into_values().collect(),
        bl_sans_facture,
        factures_sans_bl,
        avec_ecart: avec,
        sans_ecart: sans,
        tronque,
        bl_sans_lignes,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct LigneVue {
    rang: i32,
    reference: Option<String>,
    designation: Option<String>,
    quantite: Option<f64>,
    unite: Option<String>,
    prix_unitaire_ht: Option<f64>,
    montant_ht: Option<f64>,
}

#[derive(Serialize)]
pub struct LienVue {
    id: i64,
    methode: String,
    ecart_montant: Option<f64>,
    ecarts: Value,
    auteur: Option<String>,
    autre: DocRap,
}

#[derive(Serialize)]
pub struct Detail {
    lignes: Vec<LigneVue>,
    lignes_lues: bool,
    numeros_bl: Vec<String>,
    ignore: bool,
    liens: Vec<LienVue>,
    /// Documents du même fournisseur qu'on peut relier à la main.
    candidats: Vec<DocRap>,
}

/// Lignes et rapprochement d'un document.
pub async fn detail(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Detail>> {
    controler_visible(&etat, &acces, id).await?;
    let d = sqlx::query!(
        "select type, fournisseur_id, lignes_lues, numeros_bl, rapprochement_ignore
         from document where id = $1",
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    let lignes = sqlx::query_as!(
        LigneVue,
        "select rang, reference, designation, quantite::float8 as quantite, unite,
                prix_unitaire_ht::float8 as prix_unitaire_ht, montant_ht::float8 as montant_ht
         from ligne_document where document_id = $1 order by rang",
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    let lignes_liens = sqlx::query!(
        r#"select r.id, r.methode, r.ecart_montant::float8 as ecart_montant, r.ecarts,
                  u.identifiant as "auteur?",
                  case when r.facture_id = $1 then r.bl_id else r.facture_id end as "autre!"
           from rapprochement r left join utilisateur u on u.id = r.auteur_id
           where (r.facture_id = $1 or r.bl_id = $1) and not r.rejete
           order by r.id"#,
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    let autres: Vec<i64> = lignes_liens.iter().map(|l| l.autre).collect();
    let docs = documents(&etat, &acces, &autres).await?;
    let liens = lignes_liens
        .into_iter()
        .filter_map(|l| {
            docs.iter().find(|d| d.id == l.autre).map(|autre| LienVue {
                id: l.id,
                methode: l.methode,
                ecart_montant: l.ecart_montant,
                ecarts: l.ecarts,
                auteur: l.auteur,
                autre: autre.clone(),
            })
        })
        .collect();
    let oppose = match d.r#type.as_deref() {
        Some("facture") => Some("bl"),
        Some("bl") => Some("facture"),
        _ => None,
    };
    let candidats = match (oppose, d.fournisseur_id) {
        (Some(t), Some(f)) => {
            sqlx::query_as!(
                DocRap,
                r#"select d.id, d.type as "type!", d.numero, d.date_document, d.numero_commande,
                          d.montant_ht::float8 as montant_ht, d.fournisseur_id,
                          fo.nom as "fournisseur?", s.raison_sociale as "societe?",
                          p.nom as nom_fichier
                   from document d
                   join piece_jointe p on p.id = d.piece_jointe_id
                   left join fournisseur fo on fo.id = d.fournisseur_id
                   left join societe s on s.id = d.societe_id
                   where d.etat = 'lu' and d.type = $1 and d.fournisseur_id = $2 and d.id <> $3
                     and not exists (select 1 from rapprochement r where not r.rejete
                                     and ((r.facture_id = $3 and r.bl_id = d.id)
                                          or (r.bl_id = $3 and r.facture_id = d.id)))
                     and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                 where mb.mail_id = d.mail_id and ($4::bigint is null or b.societe_id = $4))
                   order by d.date_document desc nulls last, d.id desc
                   limit 50"#,
                t,
                f,
                id,
                acces.societe_filtre()
            )
            .fetch_all(&etat.pool)
            .await?
        }
        _ => Vec::new(),
    };
    Ok(Json(Detail {
        lignes,
        lignes_lues: d.lignes_lues,
        numeros_bl: d.numeros_bl,
        ignore: d.rapprochement_ignore,
        liens,
        candidats,
    }))
}

/// Documents visibles parmi `ids`.
async fn documents(etat: &Etat, acces: &Acces, ids: &[i64]) -> Resultat<Vec<DocRap>> {
    Ok(sqlx::query_as!(
        DocRap,
        r#"select d.id, d.type as "type!", d.numero, d.date_document, d.numero_commande,
                  d.montant_ht::float8 as montant_ht, d.fournisseur_id, fo.nom as "fournisseur?",
                  s.raison_sociale as "societe?", p.nom as nom_fichier
           from document d
           join piece_jointe p on p.id = d.piece_jointe_id
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.id = any($1)
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($2::bigint is null or b.societe_id = $2))"#,
        ids,
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?)
}

fn erreur_lien(e: ErreurLien) -> ErreurApi {
    match e {
        ErreurLien::Types => ErreurApi::Donnees(e.to_string()),
        ErreurLien::Absent => ErreurApi::Absent,
        ErreurLien::Base(e) => ErreurApi::Interne(e.into()),
    }
}

#[derive(Deserialize)]
pub struct NouveauLien {
    facture_id: i64,
    bl_id: i64,
}

/// Lien manuel entre une facture et un BL (les deux modifiables par l'utilisateur).
pub async fn lier(
    acces: Acces,
    State(etat): State<Etat>,
    Json(l): Json<NouveauLien>,
) -> Resultat<StatusCode> {
    controler_modifiable(&etat, &acces, l.facture_id).await?;
    controler_modifiable(&etat, &acces, l.bl_id).await?;
    let defaits = rap::lier(&etat.pool, l.facture_id, l.bl_id, acces.utilisateur_id)
        .await
        .map_err(erreur_lien)?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "rapprochement",
        "document",
        Some(l.facture_id.to_string()),
        json!({ "bl_id": l.bl_id, "liens_defaits_vers": defaits }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Défait un lien. Un lien automatique défait n'est pas recréé ; le BL libéré peut être
/// rapproché d'une autre facture.
pub async fn delier(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let r = sqlx::query!(
        "select facture_id, bl_id from rapprochement where id = $1 and not rejete",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    controler_modifiable(&etat, &acces, r.facture_id).await?;
    controler_modifiable(&etat, &acces, r.bl_id).await?;
    let (facture, bl) = rap::delier(&etat.pool, id).await.map_err(erreur_lien)?;
    rap::recalculer(&etat.pool, bl).await?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "rapprochement_defait",
        "document",
        Some(facture.to_string()),
        json!({ "bl_id": bl }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Ignore {
    ignore: bool,
}

/// « Pas de correspondance attendue » pour un BL ou une facture.
pub async fn ignorer(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(i): Json<Ignore>,
) -> Resultat<StatusCode> {
    controler_modifiable(&etat, &acces, id).await?;
    rap::ignorer(&etat.pool, id, i.ignore).await?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        if i.ignore {
            "rapprochement_ignore"
        } else {
            "rapprochement_repris"
        },
        "document",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct Relectures {
    n: usize,
}

/// Relit les BL lus avant le lot 4, sans lignes (administrateur). Le quota du jour s'applique :
/// au-delà, les BL attendent le lendemain.
pub async fn relire_bl(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<Json<Relectures>> {
    let ids = sqlx::query_scalar!(
        "update document set etat = 'a_lire', planifie_le = now(), message = null
         where type = 'bl' and etat = 'lu' and not lignes_lues
         returning id"
    )
    .fetch_all(&etat.pool)
    .await?;
    for id in &ids {
        file::pousser_lecture(&etat.pool, *id, true)
            .await
            .map_err(ErreurApi::Interne)?;
    }
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "relecture_bl",
        "document",
        None,
        json!({ "documents": ids.len() }),
    )
    .await?;
    Ok(Json(Relectures { n: ids.len() }))
}
