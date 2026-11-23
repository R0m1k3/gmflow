//! Documents lus dans les pièces jointes : liste, fiche, vérification (type et société),
//! corrections, validation et dépôt des factures, bilan de justesse. Un responsable ne voit
//! que les documents des mails reçus par une boîte de sa structure, et ne valide que ceux de sa
//! structure.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use gmflow_core::lecture::extraction::TYPES;
use gmflow_core::{depot, file, journal};
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
    statut_depot: Option<String>,
    fournisseur: Option<i64>,
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
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    statut_depot: Option<String>,
    controles: Value,
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
    let sd = vide(&f.statut_depot);
    // Par défaut, les pièces ignorées (logos, signatures) ne sont pas listées.
    let sans_ignores = e.is_none();
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from document d join mail m on m.id = d.mail_id
           where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
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
             and ($11::text is null or d.statut_depot = $11)
             and ($12::bigint is null or d.fournisseur_id = $12)"#,
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
        sd,
        f.fournisseur
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        LigneDocument,
        r#"select d.id, d.mail_id, p.nom as nom_fichier, d.etat, d.voie, d.type,
                  s.raison_sociale as "societe?", d.fournisseur_nom, d.numero, d.date_document,
                  d.montant_ttc::float8 as montant_ttc, d.message,
                  d.verifie_le is not null as "verifie!", m.date_mail,
                  d.fournisseur_id, fo.nom as "fournisseur?", d.statut_depot, d.controles
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join societe s on s.id = d.societe_id
           left join fournisseur fo on fo.id = d.fournisseur_id
           where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
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
             and ($11::text is null or d.statut_depot = $11)
             and ($12::bigint is null or d.fournisseur_id = $12)
           order by m.date_mail desc nulls last, d.id desc
           limit $13 offset $14"#,
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
        sd,
        f.fournisseur,
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
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    fournisseur_statut: Option<String>,
    statut_depot: Option<String>,
    controles: Value,
    #[serde(with = "time::serde::rfc3339::option")]
    valide_le: Option<time::OffsetDateTime>,
    valide_par: Option<String>,
    corrections: Value,
    societe_adresse_depot: Option<String>,
    societe_outil_depot: Option<String>,
    /// Réception du mail d'origine (date du mail, à défaut date de relève).
    #[serde(with = "time::serde::rfc3339")]
    recu_le: time::OffsetDateTime,
    /// Envoi en échec relancé il y a moins de 30 min, pas encore repris par le worker.
    envoi_relance: bool,
}

#[derive(Serialize)]
pub struct EnvoiVue {
    id: i64,
    #[serde(with = "time::serde::rfc3339")]
    quand: time::OffsetDateTime,
    adresse: String,
    outil: Option<String>,
    etat: String,
    message: Option<String>,
    auteur: Option<String>,
}

/// Boîte qui a reçu le mail d'origine (visible par cet accès).
#[derive(Serialize)]
pub struct BoiteReception {
    boite_id: i64,
    libelle: String,
    /// Adresse de la boîte (identifiant IMAP).
    adresse: String,
    societe_id: i64,
    societe: String,
}

#[derive(Serialize)]
pub struct FicheComplete {
    #[serde(flatten)]
    document: FicheDocument,
    envois: Vec<EnvoiVue>,
    /// Boîtes de réception du mail d'origine, par libellé.
    boites: Vec<BoiteReception>,
    /// Document pas encore lu (`a_lire`, `en_cours`) : pourquoi il attend.
    attente: Option<gmflow_core::attente::Attente>,
}

async fn mail_du_document(etat: &Etat, id: i64) -> Resultat<i64> {
    sqlx::query_scalar!("select mail_id from document where id = $1", id)
        .fetch_optional(&etat.pool)
        .await?
        .ok_or(ErreurApi::Absent)
}

/// 404 si le document n'existe pas ou n'est pas visible par cet accès.
pub(crate) async fn controler_visible(etat: &Etat, acces: &Acces, id: i64) -> Resultat<()> {
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
) -> Resultat<Json<FicheComplete>> {
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
                  u.identifiant as "verifie_par?", d.type_corrige, d.societe_corrigee,
                  d.fournisseur_id, fo.nom as "fournisseur?", fo.statut as "fournisseur_statut?",
                  d.statut_depot, d.controles, d.valide_le, uv.identifiant as "valide_par?",
                  d.corrections, s.adresse_depot as "societe_adresse_depot?",
                  s.outil_depot as "societe_outil_depot?",
                  coalesce(m.date_mail, m.recu_le) as "recu_le!",
                  coalesce(d.statut_depot = 'erreur'
                           and d.valide_le > (select max(e.quand) from envoi_depot e
                                              where e.document_id = d.id)
                           and d.valide_le > now() - interval '30 minutes', false) as "envoi_relance!"
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join societe s on s.id = d.societe_id
           left join utilisateur u on u.id = d.verifie_par
           left join utilisateur uv on uv.id = d.valide_par
           left join fournisseur fo on fo.id = d.fournisseur_id
           where d.id = $1"#,
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    let envois = sqlx::query_as!(
        EnvoiVue,
        r#"select e.id, e.quand, e.adresse, e.outil, e.etat, e.message, u.identifiant as "auteur?"
           from envoi_depot e left join utilisateur u on u.id = e.auteur_id
           where e.document_id = $1 order by e.quand desc"#,
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    let boites = sqlx::query_as!(
        BoiteReception,
        r#"select distinct b.id as boite_id, b.libelle, b.identifiant as adresse, b.societe_id,
                  s.raison_sociale as societe
           from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
           join societe s on s.id = b.societe_id
           where mb.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)
           order by b.libelle, b.id"#,
        d.mail_id,
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?;
    let attente = if d.etat == "a_lire" || d.etat == "en_cours" {
        gmflow_core::attente::expliquer(&etat.pool, id)
            .await
            .map_err(ErreurApi::Interne)?
    } else {
        None
    };
    Ok(Json(FicheComplete {
        document: d,
        envois,
        boites,
        attente,
    }))
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
        "select etat, type, societe_id, corrections from document where id = $1 for update",
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
    let mut changements = Vec::new();
    if type_change {
        changements.push(("type", json!(avant.r#type), json!(v.r#type)));
    }
    if societe_change {
        changements.push(("societe_id", json!(avant.societe_id), json!(v.societe_id)));
    }
    let corrections = fusionner_corrections(&avant.corrections, &changements);
    sqlx::query!(
        "update document set type = $2, societe_id = $3, verifie_le = now(), verifie_par = $4,
            type_corrige = type_corrige or $5, societe_corrigee = societe_corrigee or $6,
            corrections = $7, corrige_le = case when $5 or $6 then now() else corrige_le end
         where id = $1",
        id,
        v.r#type,
        v.societe_id,
        acces.utilisateur_id,
        type_change,
        societe_change,
        corrections
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
    depot::evaluer_et_deposer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    gmflow_core::rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
pub struct Relecture {
    /// « Relire en détail » : modèle plus fort et fichier d'origine joint dès le premier appel.
    #[serde(default)]
    approfondie: bool,
}

/// Relecture forcée (administrateur) : nouvel appel, même si un document identique est lu.
/// Sert aussi de « Lire maintenant » pour un document en attente : la tâche passe devant toute
/// la file de lecture et le quota du jour ne compte pas.
pub async fn relire(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    corps: Option<Json<Relecture>>,
) -> Resultat<StatusCode> {
    let r = corps.map(|Json(r)| r).unwrap_or_default();
    let n = sqlx::query!(
        "update document set etat = 'a_lire', planifie_le = now(), message = null,
            verifie_le = null, verifie_par = null, type_corrige = false, societe_corrigee = false
         where id = $1 and etat <> 'en_cours' and coalesce(statut_depot, '') <> 'en_envoi'",
        id
    )
    .execute(&etat.pool)
    .await?
    .rows_affected();
    if n == 0 {
        mail_du_document(&etat, id).await?;
        return Err(ErreurApi::Conflit("lecture ou dépôt en cours".into()));
    }
    if r.approfondie {
        file::pousser_relecture_approfondie(&etat.pool, id).await
    } else {
        file::pousser_lecture_maintenant(&etat.pool, id).await
    }
    .map_err(ErreurApi::Interne)?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "relecture",
        "document",
        Some(id.to_string()),
        json!({ "approfondie": r.approfondie }),
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
    semaines: Vec<Semaine>,
}

/// Documents vérifiés ou validés par un humain une semaine donnée (lundi), et combien sans
/// aucune correction.
#[derive(Serialize)]
pub struct Semaine {
    semaine: String,
    valides: i64,
    sans_correction: i64,
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
                 where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
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
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))"#,
        s
    )
    .fetch_one(&etat.pool)
    .await?;
    let semaines = sqlx::query_as!(
        Semaine,
        r#"select to_char(date_trunc('week', coalesce(d.verifie_le, d.valide_le) at time zone 'Europe/Paris'), 'YYYY-MM-DD') as "semaine!",
                  count(*) as "valides!",
                  count(*) filter (where d.corrige_le is null and not d.type_corrige
                                     and not d.societe_corrigee) as "sans_correction!"
           from document d
           where coalesce(d.verifie_le, d.valide_le) > now() - interval '8 weeks'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
           group by 1 order by 1"#,
        s
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Bilan {
        semaines,
        par_etat,
        par_voie,
        par_type,
        verifies: j.verifies,
        type_justes: j.type_justes,
        societe_justes: j.societe_justes,
    }))
}

/// Un responsable ne corrige ni ne valide que les documents de sa structure (ou sans
/// structure reconnue). 404 si le document n'est pas visible.
pub(crate) async fn controler_modifiable(etat: &Etat, acces: &Acces, id: i64) -> Resultat<()> {
    controler_visible(etat, acces, id).await?;
    if let Some(s) = acces.societe_filtre() {
        let societe = sqlx::query_scalar!("select societe_id from document where id = $1", id)
            .fetch_one(&etat.pool)
            .await?;
        if societe.is_some_and(|x| x != s) {
            return Err(ErreurApi::Interdit);
        }
    }
    Ok(())
}

/// Ajoute des corrections à l'historique du document. Une valeur déjà corrigée garde sa
/// valeur lue d'origine ; une correction annulée (retour à la valeur lue) disparaît.
fn fusionner_corrections(anciennes: &Value, changements: &[(&str, Value, Value)]) -> Value {
    let mut c = anciennes.as_object().cloned().unwrap_or_default();
    for (champ, avant, apres) in changements {
        let origine = c
            .get(*champ)
            .map(|v| v["avant"].clone())
            .unwrap_or_else(|| avant.clone());
        if &origine == apres {
            c.remove(*champ);
        } else {
            c.insert(
                champ.to_string(),
                json!({ "avant": origine, "apres": apres }),
            );
        }
    }
    Value::Object(c)
}

#[derive(Deserialize)]
pub struct Correction {
    r#type: String,
    societe_id: Option<i64>,
    fournisseur_id: Option<i64>,
    numero: Option<String>,
    date_document: Option<String>,
    numero_commande: Option<String>,
    montant_ht: Option<f64>,
    montant_tva: Option<f64>,
    montant_ttc: Option<f64>,
}

fn texte_net(v: &Option<String>) -> Option<String> {
    vide(v)
}

fn centimes(v: Option<f64>) -> Option<f64> {
    v.map(|x| (x * 100.0).round() / 100.0)
}

/// Correction dont les entrées ont été contrôlées (type, structure permise, fournisseur,
/// date), prête à appliquer.
pub(crate) struct CorrectionNette {
    r#type: String,
    societe_id: Option<i64>,
    fournisseur_id: Option<i64>,
    numero: Option<String>,
    date_document: Option<time::Date>,
    numero_commande: Option<String>,
    ht: Option<f64>,
    tva: Option<f64>,
    ttc: Option<f64>,
}

async fn nettoyer_correction(
    etat: &Etat,
    acces: &Acces,
    c: Correction,
) -> Resultat<CorrectionNette> {
    if !TYPES.contains(&c.r#type.as_str()) {
        return Err(ErreurApi::Donnees(format!("type inconnu : {}", c.r#type)));
    }
    if let Some(s) = c.societe_id {
        let permise = sqlx::query_scalar!(
            r#"select exists (select 1 from societe where id = $1 and ($2::bigint is null or id = $2)) as "ok!""#,
            s,
            acces.societe_filtre()
        )
        .fetch_one(&etat.pool)
        .await?;
        if !permise {
            return Err(ErreurApi::Donnees("structure inconnue".into()));
        }
    }
    if let Some(f) = c.fournisseur_id {
        let existe = sqlx::query_scalar!(
            r#"select exists (select 1 from fournisseur where id = $1 and fusionne_dans is null) as "ok!""#,
            f
        )
        .fetch_one(&etat.pool)
        .await?;
        if !existe {
            return Err(ErreurApi::Donnees("fournisseur inconnu".into()));
        }
    }
    Ok(CorrectionNette {
        date_document: date(&c.date_document)?,
        numero: texte_net(&c.numero),
        numero_commande: texte_net(&c.numero_commande),
        ht: centimes(c.montant_ht),
        tva: centimes(c.montant_tva),
        ttc: centimes(c.montant_ttc),
        r#type: c.r#type,
        societe_id: c.societe_id,
        fournisseur_id: c.fournisseur_id,
    })
}

/// Applique une correction dans la transaction (document verrouillé) : champs, historique des
/// corrections, vérification par cet utilisateur, journal.
async fn appliquer_correction(
    tx: &mut sqlx::PgConnection,
    acces: &Acces,
    id: i64,
    c: &CorrectionNette,
) -> Resultat<()> {
    let a = sqlx::query!(
        r#"select etat, statut_depot, type, societe_id, fournisseur_id, numero, date_document,
                  numero_commande, montant_ht::float8 as ht, montant_tva::float8 as tva,
                  montant_ttc::float8 as ttc, corrections
           from document where id = $1 for update"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if a.etat != "lu" {
        return Err(ErreurApi::Conflit(
            "seul un document lu peut être corrigé".into(),
        ));
    }
    if matches!(a.statut_depot.as_deref(), Some("en_envoi" | "depose")) {
        return Err(ErreurApi::Conflit(
            "document déjà déposé ou en cours d'envoi".into(),
        ));
    }
    let mut changements: Vec<(&str, Value, Value)> = Vec::new();
    let mut noter = |champ: &'static str, avant: Value, apres: Value| {
        if avant != apres {
            changements.push((champ, avant, apres));
        }
    };
    noter("type", json!(a.r#type), json!(c.r#type));
    noter("societe_id", json!(a.societe_id), json!(c.societe_id));
    noter(
        "fournisseur_id",
        json!(a.fournisseur_id),
        json!(c.fournisseur_id),
    );
    noter("numero", json!(a.numero), json!(c.numero));
    noter(
        "date_document",
        json!(a.date_document.map(|d| d.to_string())),
        json!(c.date_document.map(|d| d.to_string())),
    );
    noter(
        "numero_commande",
        json!(a.numero_commande),
        json!(c.numero_commande),
    );
    noter("montant_ht", json!(centimes(a.ht)), json!(c.ht));
    noter("montant_tva", json!(centimes(a.tva)), json!(c.tva));
    noter("montant_ttc", json!(centimes(a.ttc)), json!(c.ttc));
    let corrections = fusionner_corrections(&a.corrections, &changements);
    let type_change = changements.iter().any(|(c, _, _)| *c == "type");
    let societe_change = changements.iter().any(|(c, _, _)| *c == "societe_id");
    sqlx::query!(
        "update document set type = $2, societe_id = $3, fournisseur_id = $4, numero = $5,
            date_document = $6, numero_commande = $7,
            montant_ht = $8::float8::numeric(14, 2), montant_tva = $9::float8::numeric(14, 2),
            montant_ttc = $10::float8::numeric(14, 2),
            corrections = $11, corrige_le = case when $12 then now() else corrige_le end,
            verifie_le = now(), verifie_par = $13,
            type_corrige = type_corrige or $14, societe_corrigee = societe_corrigee or $15
         where id = $1",
        id,
        c.r#type,
        c.societe_id,
        c.fournisseur_id,
        c.numero,
        c.date_document,
        c.numero_commande,
        c.ht,
        c.tva,
        c.ttc,
        corrections,
        !changements.is_empty(),
        acces.utilisateur_id,
        type_change,
        societe_change
    )
    .execute(&mut *tx)
    .await?;
    let details: serde_json::Map<String, Value> = changements
        .iter()
        .map(|(c, av, ap)| (c.to_string(), json!({ "avant": av, "apres": ap })))
        .collect();
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        if changements.is_empty() {
            "verification"
        } else {
            "correction"
        },
        "document",
        Some(id.to_string()),
        Value::Object(details),
    )
    .await?;
    Ok(())
}

/// Correction des champs lus (type, structure, fournisseur, numéro, date, montants), puis
/// nouveaux contrôles. Vaut vérification du document.
pub async fn corriger(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(c): Json<Correction>,
) -> Resultat<StatusCode> {
    controler_modifiable(&etat, &acces, id).await?;
    let c = nettoyer_correction(&etat, &acces, c).await?;
    let mut tx = etat.pool.begin().await?;
    appliquer_correction(&mut tx, &acces, id, &c).await?;
    tx.commit().await?;
    depot::evaluer_et_deposer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    gmflow_core::rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct StatutDepot {
    statut_depot: Option<String>,
}

/// Relance les contrôles d'un document (après une modification de fiche, par exemple).
pub async fn controler(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<StatutDepot>> {
    controler_visible(&etat, &acces, id).await?;
    let statut_depot = depot::evaluer_et_deposer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(Json(StatutDepot { statut_depot }))
}

#[derive(Deserialize, Default)]
pub struct DemandeDepot {
    #[serde(default)]
    forcer: bool,
}

/// Validation humaine et dépôt d'une facture. Une validation passe outre les contrôles en
/// échec, jamais un doublon (sauf administrateur, avec `forcer`). Un dépôt en erreur ne se
/// relance que par un administrateur.
pub async fn deposer(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(dm): Json<DemandeDepot>,
) -> Resultat<StatusCode> {
    controler_modifiable(&etat, &acces, id).await?;
    if dm.forcer && !acces.est_admin() {
        return Err(ErreurApi::Interdit);
    }
    if gmflow_core::parametre::lire(&etat.pool, gmflow_core::expedition::CLE)
        .await?
        .is_none()
    {
        return Err(ErreurApi::Conflit(
            "adresse d'expédition non configurée (Administration > Expédition)".into(),
        ));
    }
    let statut = depot::evaluer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    let controles = sqlx::query_scalar!("select controles from document where id = $1", id)
        .fetch_one(&etat.pool)
        .await?;
    let sans_adresse = controles
        .as_array()
        .is_some_and(|v| v.iter().any(|c| c["code"] == "depot"));
    match statut.as_deref() {
        None => {
            return Err(ErreurApi::Conflit(
                "seuls les factures et avoirs lus se déposent".into(),
            ));
        }
        Some("en_envoi" | "depose") => {
            return Err(ErreurApi::Conflit(
                "document déjà déposé ou en cours d'envoi".into(),
            ));
        }
        Some("erreur") if !acces.est_admin() => {
            return Err(ErreurApi::Conflit(
                "dépôt en erreur : seul un administrateur peut redéposer".into(),
            ));
        }
        Some("doublon") if !dm.forcer => {
            return Err(ErreurApi::Conflit(
                "doublon d'une facture déjà reçue : dépôt bloqué".into(),
            ));
        }
        _ if sans_adresse => {
            return Err(ErreurApi::Conflit(
                "la structure n'a pas d'adresse de dépôt".into(),
            ));
        }
        _ => {}
    }
    let mut tx = etat.pool.begin().await?;
    sqlx::query!(
        "update document set valide_le = now(), valide_par = $2 where id = $1",
        id,
        acces.utilisateur_id
    )
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "validation",
        "document",
        Some(id.to_string()),
        json!({ "statut": statut, "forcer": dm.forcer, "controles": controles }),
    )
    .await?;
    tx.commit().await?;
    file::pousser_envoi(
        &etat.pool,
        file::TacheEnvoi::Deposer {
            document_id: id,
            forcer: dm.forcer,
            auteur_id: Some(acces.utilisateur_id),
        },
    )
    .await
    .map_err(ErreurApi::Interne)?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize, Default)]
pub struct DemandeValidation {
    /// Champs corrigés (mêmes champs que `PUT /api/documents/{id}`) ; absent : rien à corriger.
    #[serde(default)]
    corrections: Option<Correction>,
    /// Administrateur seulement : envoyer un doublon quand même.
    #[serde(default)]
    forcer: bool,
}

/// Société choisie dans l'interface, pour donner le document suivant de la même file.
#[derive(Deserialize)]
pub struct ContexteFile {
    societe: Option<i64>,
}

#[derive(Serialize)]
pub struct Validation {
    id: i64,
    statut_depot: Option<String>,
    /// L'envoi à Dext/Yooz a été demandé (facture ou avoir).
    envoi: bool,
    /// Document suivant de la file « À valider » (même ordre que `GET /api/a-traiter`).
    suivant: Option<i64>,
}

fn refus(code: &'static str, message: &str) -> ErreurApi {
    ErreurApi::ConflitCode {
        code,
        message: message.into(),
    }
}

/// « Valider » en un appel : corrections éventuelles, vérification, puis pour une facture ou un
/// avoir, contrôles, doublon et demande d'envoi à Dext/Yooz, le tout dans une transaction (rien
/// n'est gardé si l'envoi est refusé). Un BL (ou un autre type) est seulement vérifié. Rend le
/// document suivant de la file.
pub async fn valider(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Query(f): Query<ContexteFile>,
    Json(dv): Json<DemandeValidation>,
) -> Resultat<Json<Validation>> {
    controler_modifiable(&etat, &acces, id).await?;
    if dv.forcer && !acces.est_admin() {
        return Err(ErreurApi::Interdit);
    }
    let correction = match dv.corrections {
        Some(c) => Some(nettoyer_correction(&etat, &acces, c).await?),
        None => None,
    };
    let place = super::a_traiter::place_dans_file(&etat, id).await?;
    let mut tx = etat.pool.begin().await?;
    let a = sqlx::query!(
        "select etat, statut_depot from document where id = $1 for update",
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if a.etat != "lu" {
        return Err(refus("etat", "seul un document lu peut être validé"));
    }
    if matches!(a.statut_depot.as_deref(), Some("en_envoi" | "depose")) {
        return Err(refus(
            "deja_envoye",
            "document déjà envoyé ou en cours d'envoi",
        ));
    }
    match &correction {
        Some(c) => appliquer_correction(&mut tx, &acces, id, c).await?,
        None => {
            sqlx::query!(
                "update document set verifie_le = now(), verifie_par = $2 where id = $1",
                id,
                acces.utilisateur_id
            )
            .execute(&mut *tx)
            .await?;
            journal::ecrire(
                &mut *tx,
                Some(acces.utilisateur_id),
                "verification",
                "document",
                Some(id.to_string()),
                json!({}),
            )
            .await?;
        }
    }
    let statut = depot::evaluer_dans(&mut tx, id)
        .await
        .map_err(ErreurApi::Interne)?;
    let type_doc = sqlx::query_scalar!("select type from document where id = $1", id)
        .fetch_one(&mut *tx)
        .await?;
    let envoi = type_doc
        .as_deref()
        .is_some_and(|t| depot::TYPES_DEPOSES.contains(&t));
    if envoi {
        if gmflow_core::parametre::lire(&mut *tx, gmflow_core::expedition::CLE)
            .await?
            .is_none()
        {
            return Err(refus(
                "expedition",
                "adresse d'envoi vers Dext/Yooz non configurée (Réglages)",
            ));
        }
        let controles = sqlx::query_scalar!("select controles from document where id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        let sans_adresse = controles
            .as_array()
            .is_some_and(|v| v.iter().any(|c| c["code"] == "depot"));
        match statut.as_deref() {
            Some("erreur") if !acces.est_admin() => {
                return Err(refus(
                    "erreur_envoi",
                    "envoi précédent en échec : seul un administrateur peut renvoyer",
                ));
            }
            Some("doublon") if !dv.forcer => {
                let message = controles
                    .as_array()
                    .and_then(|v| v.iter().find(|c| c["code"] == "doublon"))
                    .and_then(|c| c["message"].as_str())
                    .unwrap_or("Facture déjà reçue");
                return Err(refus("doublon", message));
            }
            _ if sans_adresse => {
                return Err(refus(
                    "sans_adresse",
                    "la société n'a pas d'adresse d'envoi vers Dext ou Yooz",
                ));
            }
            _ => {}
        }
        sqlx::query!(
            "update document set valide_le = now(), valide_par = $2 where id = $1",
            id,
            acces.utilisateur_id
        )
        .execute(&mut *tx)
        .await?;
        journal::ecrire(
            &mut *tx,
            Some(acces.utilisateur_id),
            "validation",
            "document",
            Some(id.to_string()),
            json!({ "statut": statut, "forcer": dv.forcer, "controles": controles, "par": "valider" }),
        )
        .await?;
    }
    tx.commit().await?;
    // La file d'envoi n'est pas dans la transaction (apalis) : poussée juste après. Si elle
    // échouait, le document resterait validé sans envoi, comme avec `deposer`.
    if envoi {
        file::pousser_envoi(
            &etat.pool,
            file::TacheEnvoi::Deposer {
                document_id: id,
                forcer: dv.forcer,
                auteur_id: Some(acces.utilisateur_id),
            },
        )
        .await
        .map_err(ErreurApi::Interne)?;
    }
    gmflow_core::rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    let suivant = super::a_traiter::suivant(&etat, &acces, f.societe, id, place).await?;
    Ok(Json(Validation {
        id,
        statut_depot: statut,
        envoi,
        suivant,
    }))
}

/// « Écarter » : un doublon (vu par un humain, jamais envoyé, statut `doublon` gardé), ou tout
/// autre document lu ou illisible, qui passe en « Écarté » (`ignore` : pas une facture, jamais
/// envoyé). Le document quitte la file « À traiter ». Un envoi en échec ne s'écarte que par un
/// administrateur (il est peut-être arrivé chez Dext/Yooz).
pub async fn ecarter(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Query(f): Query<ContexteFile>,
) -> Resultat<Json<Validation>> {
    controler_modifiable(&etat, &acces, id).await?;
    let place = super::a_traiter::place_dans_file(&etat, id).await?;
    let mut tx = etat.pool.begin().await?;
    let a = sqlx::query!(
        "select etat, statut_depot from document where id = $1 for update",
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if matches!(a.statut_depot.as_deref(), Some("en_envoi" | "depose")) {
        return Err(refus(
            "deja_envoye",
            "document déjà envoyé ou en cours d'envoi",
        ));
    }
    if a.statut_depot.as_deref() == Some("erreur") && !acces.est_admin() {
        return Err(refus(
            "erreur_envoi",
            "envoi en échec : seul un administrateur peut l'écarter",
        ));
    }
    let action = if a.statut_depot.as_deref() == Some("doublon") && a.etat == "lu" {
        sqlx::query!(
            "update document set verifie_le = now(), verifie_par = $2 where id = $1",
            id,
            acces.utilisateur_id
        )
        .execute(&mut *tx)
        .await?;
        "doublon_ecarte"
    } else if a.etat == "erreur" || a.etat == "lu" {
        sqlx::query!(
            "update document set etat = 'ignore', verifie_le = now(), verifie_par = $2,
                statut_depot = null, controles = '[]'
             where id = $1",
            id,
            acces.utilisateur_id
        )
        .execute(&mut *tx)
        .await?;
        if a.etat == "erreur" {
            "illisible_ecarte"
        } else {
            "document_ecarte"
        }
    } else {
        return Err(refus(
            "etat",
            "seul un document lu ou illisible peut être écarté",
        ));
    };
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        action,
        "document",
        Some(id.to_string()),
        json!({ "etat": a.etat, "statut_depot": a.statut_depot }),
    )
    .await?;
    tx.commit().await?;
    // Un BL ou une facture écartés ne comptent plus dans le rapprochement.
    gmflow_core::rapprochement::recalculer(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    let statut_depot = sqlx::query_scalar!("select statut_depot from document where id = $1", id)
        .fetch_one(&etat.pool)
        .await?;
    let suivant = super::a_traiter::suivant(&etat, &acces, f.societe, id, place).await?;
    Ok(Json(Validation {
        id,
        statut_depot,
        envoi: false,
        suivant,
    }))
}
