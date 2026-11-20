//! Documents lus dans les pièces jointes : liste, fiche, vérification (type et société) et
//! bilan de justesse (critère du lot 2). Un responsable ne voit que les documents des mails
//! reçus par une boîte de sa structure.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use gmflow_core::lecture::extraction::TYPES;
use gmflow_core::{file, journal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::mails::{Page, date, decalage, mail_visible, motif};
use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

#[derive(Deserialize)]
pub struct Filtres {
    societe: Option<i64>,
    r#type: Option<String>,
    etat: Option<String>,
    voie: Option<String>,
    du: Option<String>,
    au: Option<String>,
    q: Option<String>,
    /// `oui` : vérifiés seulement ; `non` : à vérifier seulement.
    verifie: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct LigneDocument {
    id: i64,
    mail_id: i64,
    nom_fichier: String,
    etat: String,
    voie: Option<String>,
    r#type: Option<String>,
    societe: Option<String>,
    fournisseur_nom: Option<String>,
    numero: Option<String>,
    #[serde(with = "date_iso::option")]
    date_document: Option<time::Date>,
    montant_ttc: Option<f64>,
    message: Option<String>,
    verifie: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
}

fn vide(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub async fn lister(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Page<LigneDocument>>> {
    let (du, au) = (date(&f.du)?, date(&f.au)?);
    let q = motif(&f.q);
    let (page, decalage) = decalage(f.page);
    let verifie = match f.verifie.as_deref() {
        Some("oui") => Some(true),
        Some("non") => Some(false),
        _ => None,
    };
    let (t, e, v) = (vide(&f.r#type), vide(&f.etat), vide(&f.voie));
    // Par défaut, les pièces ignorées (logos, signatures) ne sont pas listées.
    let sans_ignores = e.is_none();
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from document d join mail m on m.id = d.mail_id
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1))
             and ($2::bigint is null or d.societe_id = $2)
             and ($3::text is null or d.type = $3)
             and ($4::text is null or d.etat = $4)
             and (not $5 or d.etat <> 'ignore')
             and ($6::text is null or d.voie = $6)
             and ($7::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $7)
             and ($8::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) <= $8)
             and ($9::text is null or d.fournisseur_nom ilike $9 or d.numero ilike $9 or m.objet ilike $9)
             and ($10::bool is null or (d.verifie_le is not null) = $10)"#,
        acces.societe_filtre(),
        f.societe,
        t,
        e,
        sans_ignores,
        v,
        du,
        au,
        q,
        verifie
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        LigneDocument,
        r#"select d.id, d.mail_id, p.nom as nom_fichier, d.etat, d.voie, d.type,
                  s.raison_sociale as "societe?", d.fournisseur_nom, d.numero, d.date_document,
                  d.montant_ttc::float8 as montant_ttc, d.message,
                  d.verifie_le is not null as "verifie!", m.date_mail
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join societe s on s.id = d.societe_id
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1))
             and ($2::bigint is null or d.societe_id = $2)
             and ($3::text is null or d.type = $3)
             and ($4::text is null or d.etat = $4)
             and (not $5 or d.etat <> 'ignore')
             and ($6::text is null or d.voie = $6)
             and ($7::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $7)
             and ($8::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) <= $8)
             and ($9::text is null or d.fournisseur_nom ilike $9 or d.numero ilike $9 or m.objet ilike $9)
             and ($10::bool is null or (d.verifie_le is not null) = $10)
           order by m.date_mail desc nulls last, d.id desc
           limit $11 offset $12"#,
        acces.societe_filtre(),
        f.societe,
        t,
        e,
        sans_ignores,
        v,
        du,
        au,
        q,
        verifie,
        super::mails::PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Page {
        total,
        page,
        elements,
    }))
}

#[derive(Serialize)]
pub struct FicheDocument {
    id: i64,
    mail_id: i64,
    mail_objet: String,
    mail_expediteur: String,
    piece_id: i64,
    nom_fichier: String,
    type_mime: String,
    etat: String,
    voie: Option<String>,
    message: Option<String>,
    r#type: Option<String>,
    societe_id: Option<i64>,
    societe: Option<String>,
    fournisseur_nom: Option<String>,
    fournisseur_siret: Option<String>,
    fournisseur_tva: Option<String>,
    numero: Option<String>,
    #[serde(with = "date_iso::option")]
    date_document: Option<time::Date>,
    numero_commande: Option<String>,
    montant_ht: Option<f64>,
    montant_tva: Option<f64>,
    montant_ttc: Option<f64>,
    confiance: Value,
    texte: Option<String>,
    modele: Option<String>,
    effort: Option<String>,
    copie_de: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    lu_le: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    verifie_le: Option<time::OffsetDateTime>,
    verifie_par: Option<String>,
    type_corrige: bool,
    societe_corrigee: bool,
}

async fn mail_du_document(etat: &Etat, id: i64) -> Resultat<i64> {
    sqlx::query_scalar!("select mail_id from document where id = $1", id)
        .fetch_optional(&etat.pool)
        .await?
        .ok_or(ErreurApi::Absent)
}

/// 404 si le document n'existe pas ou n'est pas visible par cet accès.
async fn controler_visible(etat: &Etat, acces: &Acces, id: i64) -> Resultat<()> {
    let mail_id = mail_du_document(etat, id).await?;
    if !mail_visible(etat, acces, mail_id).await? {
        return Err(ErreurApi::Absent);
    }
    Ok(())
}

pub async fn fiche(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<FicheDocument>> {
    controler_visible(&etat, &acces, id).await?;
    let d = sqlx::query_as!(
        FicheDocument,
        r#"select d.id, d.mail_id, m.objet as mail_objet, m.expediteur as mail_expediteur,
                  p.id as piece_id, p.nom as nom_fichier, p.type_mime, d.etat, d.voie, d.message,
                  d.type, d.societe_id, s.raison_sociale as "societe?", d.fournisseur_nom,
                  d.fournisseur_siret, d.fournisseur_tva, d.numero, d.date_document,
                  d.numero_commande, d.montant_ht::float8 as montant_ht,
                  d.montant_tva::float8 as montant_tva, d.montant_ttc::float8 as montant_ttc,
                  d.confiance, d.texte, d.modele, d.effort, d.copie_de, d.lu_le, d.verifie_le,
                  u.identifiant as "verifie_par?", d.type_corrige, d.societe_corrigee
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join societe s on s.id = d.societe_id
           left join utilisateur u on u.id = d.verifie_par
           where d.id = $1"#,
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    Ok(Json(d))
}

#[derive(Deserialize)]
pub struct Verification {
    r#type: String,
    societe_id: Option<i64>,
}

/// Vérification humaine : le type et la société sont confirmés ou corrigés. Une correction
/// reste comptée même si le document est vérifié de nouveau.
pub async fn verifier(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(v): Json<Verification>,
) -> Resultat<StatusCode> {
    controler_visible(&etat, &acces, id).await?;
    if !TYPES.contains(&v.r#type.as_str()) {
        return Err(ErreurApi::Donnees(format!("type inconnu : {}", v.r#type)));
    }
    if let Some(s) = v.societe_id {
        let visible = sqlx::query_scalar!(
            r#"select exists (select 1 from societe where id = $1 and ($2::bigint is null or id = $2)) as "ok!""#,
            s,
            acces.societe_filtre()
        )
        .fetch_one(&etat.pool)
        .await?;
        if !visible {
            return Err(ErreurApi::Donnees("structure inconnue".into()));
        }
    }
    let mut tx = etat.pool.begin().await?;
    let avant = sqlx::query!(
        "select etat, type, societe_id from document where id = $1 for update",
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if avant.etat != "lu" {
        return Err(ErreurApi::Conflit(
            "seul un document lu peut être vérifié".into(),
        ));
    }
    let type_change = avant.r#type.as_deref() != Some(v.r#type.as_str());
    let societe_change = avant.societe_id != v.societe_id;
    sqlx::query!(
        "update document set type = $2, societe_id = $3, verifie_le = now(), verifie_par = $4,
            type_corrige = type_corrige or $5, societe_corrigee = societe_corrigee or $6
         where id = $1",
        id,
        v.r#type,
        v.societe_id,
        acces.utilisateur_id,
        type_change,
        societe_change
    )
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        if type_change || societe_change {
            "correction"
        } else {
            "verification"
        },
        "document",
        Some(id.to_string()),
        json!({
            "type": { "avant": avant.r#type, "apres": v.r#type },
            "societe_id": { "avant": avant.societe_id, "apres": v.societe_id }
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Relecture forcée (administrateur) : nouvel appel, même si un document identique est lu.
pub async fn relire(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let n = sqlx::query!(
        "update document set etat = 'a_lire', planifie_le = now(), message = null,
            verifie_le = null, verifie_par = null, type_corrige = false, societe_corrigee = false
         where id = $1 and etat <> 'en_cours'",
        id
    )
    .execute(&etat.pool)
    .await?
    .rows_affected();
    if n == 0 {
        mail_du_document(&etat, id).await?;
        return Err(ErreurApi::Conflit("lecture en cours".into()));
    }
    file::pousser_lecture(&etat.pool, id, true)
        .await
        .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "relecture",
        "document",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct Compte {
    cle: Option<String>,
    n: i64,
}

#[derive(Serialize)]
pub struct Bilan {
    par_etat: Vec<Compte>,
    par_voie: Vec<Compte>,
    par_type: Vec<Compte>,
    verifies: i64,
    type_justes: i64,
    societe_justes: i64,
}

/// Bilan des documents visibles : volumes, et justesse du type et de la société sur les
/// documents vérifiés (critère du lot : plus de 95 % sur 50 documents).
pub async fn bilan(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<Bilan>> {
    let s = acces.societe_filtre();
    let compter = |colonne: &'static str| {
        let pool = etat.pool.clone();
        async move {
            sqlx::query_as::<_, (Option<String>, i64)>(sqlx::AssertSqlSafe(format!(
                "select d.{colonne}, count(*) from document d
                 where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                               where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
                 group by 1 order by 2 desc"
            )))
            .bind(s)
            .fetch_all(&pool)
            .await
            .map(|v| v.into_iter().map(|(cle, n)| Compte { cle, n }).collect::<Vec<_>>())
        }
    };
    let par_etat = compter("etat").await?;
    let par_voie = compter("voie").await?;
    let par_type = compter("type").await?;
    let j = sqlx::query!(
        r#"select count(*) as "verifies!",
                  count(*) filter (where not type_corrige) as "type_justes!",
                  count(*) filter (where not societe_corrigee) as "societe_justes!"
           from document d
           where verifie_le is not null
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))"#,
        s
    )
    .fetch_one(&etat.pool)
    .await?;
    Ok(Json(Bilan {
        par_etat,
        par_voie,
        par_type,
        verifies: j.verifies,
        type_justes: j.type_justes,
        societe_justes: j.societe_justes,
    }))
}
