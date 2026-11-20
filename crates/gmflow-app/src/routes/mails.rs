use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

pub const PAR_PAGE: i64 = 50;

#[derive(Serialize)]
pub struct Page<T> {
    pub total: i64,
    pub page: i64,
    pub elements: Vec<T>,
}

pub fn decalage(page: Option<i64>) -> (i64, i64) {
    let p = page.unwrap_or(1).max(1);
    (p, (p - 1) * PAR_PAGE)
}

#[derive(Serialize)]
pub struct SocieteVisible {
    id: i64,
    raison_sociale: String,
}

pub async fn societes(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<Vec<SocieteVisible>>> {
    let v = sqlx::query_as!(
        SocieteVisible,
        "select id, raison_sociale from societe where ($1::bigint is null or id = $1) order by raison_sociale",
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}

#[derive(Serialize)]
pub struct BoiteVisible {
    id: i64,
    libelle: String,
    societe_id: i64,
}

pub async fn boites(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<Vec<BoiteVisible>>> {
    let v = sqlx::query_as!(
        BoiteVisible,
        "select id, libelle, societe_id from boite_mail where ($1::bigint is null or societe_id = $1) order by libelle",
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct Filtres {
    societe: Option<i64>,
    boite: Option<i64>,
    du: Option<String>,
    au: Option<String>,
    q: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct LigneMail {
    id: i64,
    expediteur: String,
    objet: String,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
    nb_pieces: i64,
    boites: Vec<String>,
}

fn date(v: &Option<String>) -> Resultat<Option<time::Date>> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => time::Date::parse(s, &format)
            .map(Some)
            .map_err(|_| ErreurApi::Donnees(format!("date invalide : {s} (AAAA-MM-JJ)"))),
    }
}

/// Motif ILIKE : les caractères spéciaux de la saisie sont échappés.
fn motif(q: &Option<String>) -> Option<String> {
    let q = q.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    let echappe = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    Some(format!("%{echappe}%"))
}

pub async fn lister(acces: Acces, State(etat): State<Etat>, Query(f): Query<Filtres>) -> Resultat<Json<Page<LigneMail>>> {
    let (du, au) = (date(&f.du)?, date(&f.au)?);
    let q = motif(&f.q);
    let (page, decalage) = decalage(f.page);
    let acces_societe = acces.societe_filtre();

    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from mail m
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2)
                           and ($3::bigint is null or b.id = $3))
             and ($4::date is null or coalesce(m.date_mail, m.recu_le) >= ($4::date::timestamp at time zone 'Europe/Paris'))
             and ($5::date is null or coalesce(m.date_mail, m.recu_le) < (($5::date + 1)::timestamp at time zone 'Europe/Paris'))
             and ($6::text is null or m.expediteur ilike $6 or m.objet ilike $6)"#,
        acces_societe,
        f.societe,
        f.boite,
        du,
        au,
        q
    )
    .fetch_one(&etat.pool)
    .await?;

    let elements = sqlx::query_as!(
        LigneMail,
        r#"select m.id, m.expediteur, m.objet, m.date_mail,
               (select count(*) from piece_jointe p where p.mail_id = m.id) as "nb_pieces!",
               array(select distinct b.libelle from mail_boite mb join boite_mail b on b.id = mb.boite_id
                     where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1)
                     order by b.libelle) as "boites!"
           from mail m
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2)
                           and ($3::bigint is null or b.id = $3))
             and ($4::date is null or coalesce(m.date_mail, m.recu_le) >= ($4::date::timestamp at time zone 'Europe/Paris'))
             and ($5::date is null or coalesce(m.date_mail, m.recu_le) < (($5::date + 1)::timestamp at time zone 'Europe/Paris'))
             and ($6::text is null or m.expediteur ilike $6 or m.objet ilike $6)
           order by coalesce(m.date_mail, m.recu_le) desc, m.id desc
           limit $7 offset $8"#,
        acces_societe,
        f.societe,
        f.boite,
        du,
        au,
        q,
        PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Page { total, page, elements }))
}

#[derive(Serialize)]
pub struct PieceVue {
    id: i64,
    nom: String,
    type_mime: String,
    taille: i64,
}

#[derive(Serialize)]
pub struct BoiteDuMail {
    boite_id: i64,
    libelle: String,
    dossier: String,
}

#[derive(Serialize)]
pub struct FicheMail {
    id: i64,
    message_id: String,
    expediteur: String,
    destinataires: Vec<String>,
    objet: String,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
    corps_texte: String,
    pieces: Vec<PieceVue>,
    boites: Vec<BoiteDuMail>,
}

/// Vrai si le mail a au moins une ligne `mail_boite` visible par cet accès.
pub async fn mail_visible(etat: &Etat, acces: &Acces, mail_id: i64) -> Resultat<bool> {
    Ok(sqlx::query_scalar!(
        r#"select exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                          where mb.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)) as "ok!""#,
        mail_id,
        acces.societe_filtre()
    )
    .fetch_one(&etat.pool)
    .await?)
}

pub async fn fiche(acces: Acces, State(etat): State<Etat>, Path(id): Path<i64>) -> Resultat<Json<FicheMail>> {
    if !mail_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let m = sqlx::query!(
        "select id, message_id, expediteur, destinataires, objet, date_mail, corps_texte from mail where id = $1",
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    let pieces = sqlx::query_as!(
        PieceVue,
        "select id, nom, type_mime, taille from piece_jointe where mail_id = $1 order by rang",
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    let boites = sqlx::query_as!(
        BoiteDuMail,
        "select distinct b.id as boite_id, b.libelle, mb.dossier
         from mail_boite mb join boite_mail b on b.id = mb.boite_id
         where mb.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)
         order by b.libelle, mb.dossier",
        id,
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(FicheMail {
        id: m.id,
        message_id: m.message_id,
        expediteur: m.expediteur,
        destinataires: m.destinataires,
        objet: m.objet,
        date_mail: m.date_mail,
        corps_texte: m.corps_texte,
        pieces,
        boites,
    }))
}
