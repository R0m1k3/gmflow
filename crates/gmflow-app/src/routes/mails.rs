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

pub async fn societes(
    acces: Acces,
    State(etat): State<Etat>,
) -> Resultat<Json<Vec<SocieteVisible>>> {
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
    /// Une catégorie, `hors_publicite` (toutes sauf les publicités) ou `non_trie`.
    categorie: Option<String>,
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
    categorie: Option<String>,
    statut_reponse: Option<String>,
}

pub fn filtre_categorie(v: &Option<String>) -> Resultat<Option<String>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(c)
            if c == "hors_publicite"
                || c == "non_trie"
                || gmflow_core::tri::CATEGORIES.contains(&c) =>
        {
            Ok(Some(c.to_string()))
        }
        Some(c) => Err(ErreurApi::Donnees(format!("catégorie inconnue : {c}"))),
    }
}

pub fn date(v: &Option<String>) -> Resultat<Option<time::Date>> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => time::Date::parse(s, &format)
            .map(Some)
            .map_err(|_| ErreurApi::Donnees(format!("date invalide : {s} (AAAA-MM-JJ)"))),
    }
}

/// Motif ILIKE : les caractères spéciaux de la saisie sont échappés.
pub fn motif(q: &Option<String>) -> Option<String> {
    let q = q.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    let echappe = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    Some(format!("%{echappe}%"))
}

pub async fn lister(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Page<LigneMail>>> {
    let (du, au) = (date(&f.du)?, date(&f.au)?);
    let q = motif(&f.q);
    let (page, decalage) = decalage(f.page);
    let acces_societe = acces.societe_filtre();
    let categorie = filtre_categorie(&f.categorie)?;

    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from mail m
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2)
                           and ($3::bigint is null or b.id = $3))
             and ($4::date is null or coalesce(m.date_mail, m.recu_le) >= ($4::date::timestamp at time zone 'Europe/Paris'))
             and ($5::date is null or coalesce(m.date_mail, m.recu_le) < (($5::date + 1)::timestamp at time zone 'Europe/Paris'))
             and ($6::text is null or m.expediteur ilike $6 or m.objet ilike $6)
             and ($7::text is null
                  or ($7 = 'hors_publicite' and m.categorie is distinct from 'publicite')
                  or ($7 = 'non_trie' and m.categorie is null)
                  or m.categorie = $7)"#,
        acces_societe,
        f.societe,
        f.boite,
        du,
        au,
        q,
        categorie
    )
    .fetch_one(&etat.pool)
    .await?;

    let elements = sqlx::query_as!(
        LigneMail,
        r#"select m.id, m.expediteur, m.objet, m.date_mail,
               (select count(*) from piece_jointe p where p.mail_id = m.id) as "nb_pieces!",
               array(select distinct b.libelle from mail_boite mb join boite_mail b on b.id = mb.boite_id
                     where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1)
                     order by b.libelle) as "boites!",
               m.categorie, m.statut_reponse
           from mail m
           where exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2)
                           and ($3::bigint is null or b.id = $3))
             and ($4::date is null or coalesce(m.date_mail, m.recu_le) >= ($4::date::timestamp at time zone 'Europe/Paris'))
             and ($5::date is null or coalesce(m.date_mail, m.recu_le) < (($5::date + 1)::timestamp at time zone 'Europe/Paris'))
             and ($6::text is null or m.expediteur ilike $6 or m.objet ilike $6)
             and ($9::text is null
                  or ($9 = 'hors_publicite' and m.categorie is distinct from 'publicite')
                  or ($9 = 'non_trie' and m.categorie is null)
                  or m.categorie = $9)
           order by coalesce(m.date_mail, m.recu_le) desc, m.id desc
           limit $7 offset $8"#,
        acces_societe,
        f.societe,
        f.boite,
        du,
        au,
        q,
        PAR_PAGE,
        decalage,
        categorie
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
pub struct PieceVue {
    id: i64,
    nom: String,
    type_mime: String,
    taille: i64,
    document_id: Option<i64>,
    document_etat: Option<String>,
    document_type: Option<String>,
    document_voie: Option<String>,
    /// Résumé du document lu (pour la liste des pièces, sans ouvrir chaque fiche).
    document_statut_depot: Option<String>,
    document_verifie: Option<bool>,
    document_numero: Option<String>,
    document_montant_ttc: Option<f64>,
    document_fournisseur_id: Option<i64>,
    /// Fiche fournisseur, à défaut nom lu.
    document_fournisseur: Option<String>,
    document_societe_id: Option<i64>,
    document_societe: Option<String>,
    /// Outil de la société du document : `dext` ou `yooz`.
    document_outil: Option<String>,
    /// Dernier envoi réussi à Dext/Yooz.
    #[serde(with = "time::serde::rfc3339::option")]
    document_envoye_le: Option<time::OffsetDateTime>,
}

#[derive(Serialize)]
pub struct BoiteDuMail {
    boite_id: i64,
    libelle: String,
    dossier: String,
    /// Adresse de la boîte (identifiant IMAP) : « Envoyer depuis compta@… ».
    adresse: String,
    societe_id: i64,
    societe: String,
    /// Serveur SMTP configuré : la boîte peut envoyer des réponses.
    smtp: bool,
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
    /// Fiche fournisseur reconnue par le domaine de l'expéditeur (nom affiché, encadré du fil).
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    pieces: Vec<PieceVue>,
    boites: Vec<BoiteDuMail>,
    categorie: Option<String>,
    categorie_par: Option<String>,
    resume: Option<String>,
    priorite: Option<String>,
    statut_reponse: Option<String>,
    tri_etat: Option<String>,
    tri_message: Option<String>,
    brouillons: Vec<super::reponses::BrouillonVue>,
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

pub async fn fiche(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<FicheMail>> {
    if !mail_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let m = sqlx::query!(
        "select id, message_id, expediteur, destinataires, objet, date_mail, corps_texte, categorie,
                categorie_par, resume, priorite, statut_reponse, tri_etat, tri_message
         from mail where id = $1",
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    let fournisseur = sqlx::query!(
        "select f.id, f.nom from fournisseur f
         where f.fusionne_dans is null and lower(split_part($1, '@', 2)) = any(f.domaines)
         order by f.id limit 1",
        m.expediteur
    )
    .fetch_optional(&etat.pool)
    .await?;
    let pieces = sqlx::query_as!(
        PieceVue,
        r#"select p.id, p.nom, p.type_mime, p.taille, d.id as "document_id?",
                  d.etat as "document_etat?", d.type as "document_type?", d.voie as "document_voie?",
                  d.statut_depot as "document_statut_depot?",
                  (d.verifie_le is not null) as "document_verifie?",
                  d.numero as "document_numero?",
                  d.montant_ttc::float8 as "document_montant_ttc?",
                  d.fournisseur_id as "document_fournisseur_id?",
                  coalesce(fo.nom, d.fournisseur_nom) as "document_fournisseur?",
                  d.societe_id as "document_societe_id?",
                  s.raison_sociale as "document_societe?",
                  s.outil_depot as "document_outil?",
                  (select max(e.quand) from envoi_depot e
                   where e.document_id = d.id and e.etat = 'envoye') as "document_envoye_le?"
           from piece_jointe p
           left join document d on d.piece_jointe_id = p.id
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where p.mail_id = $1 order by p.rang"#,
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    let boites = sqlx::query_as!(
        BoiteDuMail,
        r#"select distinct b.id as boite_id, b.libelle, mb.dossier, b.identifiant as adresse,
                  b.societe_id, s.raison_sociale as societe,
                  (b.smtp_serveur is not null) as "smtp!"
           from mail_boite mb join boite_mail b on b.id = mb.boite_id
           join societe s on s.id = b.societe_id
           where mb.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)
           order by b.libelle, mb.dossier"#,
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
        fournisseur_id: fournisseur.as_ref().map(|f| f.id),
        fournisseur: fournisseur.map(|f| f.nom),
        pieces,
        boites,
        categorie: m.categorie,
        categorie_par: m.categorie_par,
        resume: m.resume,
        priorite: m.priorite,
        statut_reponse: m.statut_reponse,
        tri_etat: m.tri_etat,
        tri_message: m.tri_message,
        brouillons: super::reponses::brouillons_du_mail(&etat, &acces, id).await?,
    }))
}
