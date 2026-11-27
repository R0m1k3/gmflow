//! Lot 5 : file « à répondre », catégorie des mails, expéditeurs marqués comme publicité,
//! brouillons de réponse. Une réponse ne part que par `POST /api/brouillons/{id}/envoyer`,
//! c'est-à-dire un clic d'un utilisateur ; aucun autre chemin ne pousse d'envoi.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use gmflow_core::file::{self, TacheEnvoi};
use gmflow_core::{journal, tri};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::mails::{Page, decalage, mail_visible};
use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

time::serde::format_description!(jour_iso, Date, "[year]-[month]-[day]");

#[derive(Serialize)]
pub struct BrouillonVue {
    id: i64,
    boite_id: i64,
    boite: String,
    /// Adresse de la boîte d'envoi (identifiant IMAP), pour « Envoyer depuis compta@… ».
    boite_adresse: String,
    texte: String,
    statut: String,
    modele: Option<String>,
    effort: Option<String>,
    erreur: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    cree_le: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    modifie_le: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    envoye_le: Option<time::OffsetDateTime>,
    envoye_par: Option<String>,
    /// La boîte d'envoi a un serveur SMTP configuré.
    smtp: bool,
}

/// Brouillons d'un mail, limités aux boîtes visibles par cet accès.
pub async fn brouillons_du_mail(
    etat: &Etat,
    acces: &Acces,
    mail_id: i64,
) -> Resultat<Vec<BrouillonVue>> {
    Ok(sqlx::query_as!(
        BrouillonVue,
        r#"select br.id, br.boite_id, b.libelle as boite, b.identifiant as boite_adresse, br.texte, br.statut, br.modele, br.effort,
                  br.erreur, br.cree_le, br.modifie_le, br.envoye_le, u.identifiant as "envoye_par?",
                  (b.smtp_serveur is not null) as "smtp!"
           from brouillon_reponse br
           join boite_mail b on b.id = br.boite_id
           left join utilisateur u on u.id = br.envoye_par
           where br.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)
           order by br.cree_le desc"#,
        mail_id,
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?)
}

#[derive(Deserialize)]
pub struct FiltresARepondre {
    societe: Option<i64>,
    priorite: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct LigneARepondre {
    id: i64,
    expediteur: String,
    objet: String,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
    resume: Option<String>,
    priorite: Option<String>,
    categorie_par: Option<String>,
    structures: Vec<String>,
    boites: Vec<String>,
    brouillon: Option<String>,
}

/// File « à répondre » : priorité haute d'abord, puis les plus anciens.
pub async fn a_repondre(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<FiltresARepondre>,
) -> Resultat<Json<Page<LigneARepondre>>> {
    let priorite = f
        .priorite
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string);
    if let Some(p) = &priorite
        && !tri::PRIORITES.contains(&p.as_str())
    {
        return Err(ErreurApi::Donnees(format!("priorité inconnue : {p}")));
    }
    let (page, decalage) = decalage(f.page);
    let acces_societe = acces.societe_filtre();
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from mail m
           where m.statut_reponse = 'a_repondre'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2))
             and ($3::text is null or coalesce(m.priorite, 'normale') = $3)"#,
        acces_societe,
        f.societe,
        priorite
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        LigneARepondre,
        r#"select m.id, m.expediteur, m.objet, m.date_mail, m.resume, m.priorite, m.categorie_par,
               array(select distinct s.raison_sociale from mail_boite_vue mb
                     join boite_mail b on b.id = mb.boite_id join societe s on s.id = b.societe_id
                     where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1)
                     order by s.raison_sociale) as "structures!",
               array(select distinct b.libelle from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                     where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1)
                     order by b.libelle) as "boites!",
               (select br.statut from brouillon_reponse br
                where br.mail_id = m.id and br.statut in ('generation', 'propose', 'modifie', 'envoi')
                limit 1) as brouillon
           from mail m
           where m.statut_reponse = 'a_repondre'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2))
             and ($3::text is null or coalesce(m.priorite, 'normale') = $3)
           order by case coalesce(m.priorite, 'normale') when 'haute' then 0 when 'normale' then 1 else 2 end,
                    coalesce(m.date_mail, m.recu_le), m.id
           limit $4 offset $5"#,
        acces_societe,
        f.societe,
        priorite,
        super::mails::PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Page::comptee(total, page, elements)))
}

pub async fn compte_a_repondre(acces: &Acces, etat: &Etat) -> Resultat<i64> {
    Ok(sqlx::query_scalar!(
        r#"select count(*) as "n!" from mail m
           where m.statut_reponse = 'a_repondre'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1))"#,
        acces.societe_filtre()
    )
    .fetch_one(&etat.pool)
    .await?)
}

pub async fn compte(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<serde_json::Value>> {
    Ok(Json(
        json!({ "n": compte_a_repondre(&acces, &etat).await? }),
    ))
}

#[derive(Serialize)]
pub struct CompteCategorie {
    cle: Option<String>,
    n: i64,
}

#[derive(Serialize)]
pub struct Semaine {
    #[serde(with = "jour_iso")]
    debut: time::Date,
    #[serde(with = "jour_iso")]
    fin: time::Date,
    par_categorie: Vec<CompteCategorie>,
    a_repondre: i64,
    repondu: i64,
    sans_suite: i64,
    /// Mails triés par une règle ou par Claude, puis revus à la main (corrigés ou confirmés).
    revus: i64,
    /// Parmi eux, ceux dont la catégorie a été changée.
    corriges: i64,
}

/// Bilan du tri pour la semaine en cours et la précédente (lundi à dimanche, heure de Paris) :
/// sert à vérifier que les questions de la semaine ressortent toutes.
pub async fn semaines(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<Vec<Semaine>>> {
    let mut v = Vec::new();
    for decalage in [0i32, 1] {
        let debut = sqlx::query_scalar!(
            r#"select (date_trunc('week', now() at time zone 'Europe/Paris')::date - 7 * $1) as "d!""#,
            decalage
        )
        .fetch_one(&etat.pool)
        .await?;
        let par_categorie = sqlx::query_as!(
            CompteCategorie,
            r#"select m.categorie as cle, count(*) as "n!" from mail m
               where coalesce(m.date_mail, m.recu_le) >= ($1::date::timestamp at time zone 'Europe/Paris')
                 and coalesce(m.date_mail, m.recu_le) < (($1::date + 7)::timestamp at time zone 'Europe/Paris')
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = m.id and ($2::bigint is null or b.societe_id = $2))
               group by m.categorie order by count(*) desc"#,
            debut,
            acces.societe_filtre()
        )
        .fetch_all(&etat.pool)
        .await?;
        let r = sqlx::query!(
            r#"select count(*) filter (where statut_reponse = 'a_repondre') as "a!",
                      count(*) filter (where statut_reponse = 'repondu') as "r!",
                      count(*) filter (where statut_reponse = 'sans_suite') as "s!",
                      count(*) filter (where categorie_auto is not null) as "revus!",
                      count(*) filter (where categorie_auto is not null and categorie_auto <> categorie) as "corriges!"
               from mail m
               where coalesce(m.date_mail, m.recu_le) >= ($1::date::timestamp at time zone 'Europe/Paris')
                 and coalesce(m.date_mail, m.recu_le) < (($1::date + 7)::timestamp at time zone 'Europe/Paris')
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = m.id and ($2::bigint is null or b.societe_id = $2))"#,
            debut,
            acces.societe_filtre()
        )
        .fetch_one(&etat.pool)
        .await?;
        v.push(Semaine {
            debut,
            fin: debut + time::Duration::days(6),
            par_categorie,
            a_repondre: r.a,
            repondu: r.r,
            sans_suite: r.s,
            revus: r.revus,
            corriges: r.corriges,
        });
    }
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct EntreeCategorie {
    categorie: String,
}

/// Catégorie fixée par un utilisateur : elle n'est plus jamais changée par le tri.
pub async fn changer_categorie(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<EntreeCategorie>,
) -> Resultat<StatusCode> {
    if !tri::CATEGORIES.contains(&e.categorie.as_str()) {
        return Err(ErreurApi::Donnees(format!(
            "catégorie inconnue : {}",
            e.categorie
        )));
    }
    if !mail_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let mut tx = etat.pool.begin().await?;
    let avant = sqlx::query_scalar!("select categorie from mail where id = $1 for update", id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query!(
        "update mail set categorie = $2, categorie_par = 'humain', tri_etat = 'trie', trie_le = now(),
            tri_message = null,
            categorie_auto = case when categorie_par in ('regle', 'claude') then categorie else categorie_auto end,
            categorie_auto_par = case when categorie_par in ('regle', 'claude') then categorie_par else categorie_auto_par end,
            statut_reponse = case
                when $2 = 'question' then coalesce(statut_reponse, 'a_repondre')
                when statut_reponse = 'a_repondre' then null
                else statut_reponse end
         where id = $1",
        id,
        e.categorie
    )
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "categorie",
        "mail",
        Some(id.to_string()),
        json!({ "avant": avant, "apres": e.categorie }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct EntreeStatut {
    statut: String,
}

/// « Sans suite », « Rouvrir » (`a_repondre`) ou « Répondu ailleurs » (`repondu`).
pub async fn changer_statut(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<EntreeStatut>,
) -> Resultat<StatusCode> {
    if !["a_repondre", "repondu", "sans_suite"].contains(&e.statut.as_str()) {
        return Err(ErreurApi::Donnees(format!("statut inconnu : {}", e.statut)));
    }
    if !mail_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let mut tx = etat.pool.begin().await?;
    let avant = sqlx::query_scalar!(
        "select statut_reponse from mail where id = $1 for update",
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "update mail set statut_reponse = $2 where id = $1",
        id,
        e.statut
    )
    .execute(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "statut_reponse",
        "mail",
        Some(id.to_string()),
        json!({ "avant": avant, "apres": e.statut }),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
pub struct EntreeBrouillon {
    #[serde(default)]
    boite_id: Option<i64>,
}

#[derive(Serialize)]
pub struct BrouillonCree {
    id: i64,
}

/// Demande un brouillon (ou en régénère un) : la génération part dans la file `redaction`.
pub async fn demander_brouillon(
    acces: Acces,
    State(etat): State<Etat>,
    Path(mail_id): Path<i64>,
    body: Option<Json<EntreeBrouillon>>,
) -> Resultat<(StatusCode, Json<BrouillonCree>)> {
    let e = body.map(|Json(e)| e).unwrap_or_default();
    if !mail_visible(&etat, &acces, mail_id).await? {
        return Err(ErreurApi::Absent);
    }
    // Boîte d'envoi : une boîte qui a reçu le mail et que l'utilisateur voit.
    let boite_id = sqlx::query_scalar!(
        "select b.id from mail_boite mb join boite_mail b on b.id = mb.boite_id
         where mb.mail_id = $1 and ($2::bigint is null or b.societe_id = $2)
           and ($3::bigint is null or b.id = $3)
         order by b.id limit 1",
        mail_id,
        acces.societe_filtre(),
        e.boite_id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or_else(|| ErreurApi::Donnees("boîte d'envoi inconnue pour ce mail".into()))?;
    let mut tx = etat.pool.begin().await?;
    let actif = sqlx::query!(
        "select id, statut, boite_id from brouillon_reponse
         where mail_id = $1 and statut in ('generation', 'propose', 'modifie', 'envoi') for update",
        mail_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let id = match actif {
        Some(b) if b.statut == "generation" || b.statut == "envoi" => {
            return Err(ErreurApi::Conflit(
                "un brouillon est déjà en préparation ou en envoi".into(),
            ));
        }
        Some(b) => {
            sqlx::query!(
                "update brouillon_reponse set statut = 'generation', erreur = null, boite_id = $2,
                    demande_par = $3, modifie_le = now()
                 where id = $1",
                b.id,
                boite_id,
                acces.utilisateur_id
            )
            .execute(&mut *tx)
            .await?;
            b.id
        }
        None => {
            sqlx::query_scalar!(
                "insert into brouillon_reponse (mail_id, boite_id, statut, demande_par)
                 values ($1, $2, 'generation', $3) returning id",
                mail_id,
                boite_id,
                acces.utilisateur_id
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "brouillon_demande",
        "mail",
        Some(mail_id.to_string()),
        json!({ "brouillon_id": id, "boite_id": boite_id }),
    )
    .await?;
    tx.commit().await?;
    file::pousser_brouillon(&etat.pool, id).await?;
    Ok((StatusCode::ACCEPTED, Json(BrouillonCree { id })))
}

struct BrouillonDroits {
    mail_id: i64,
    statut: String,
    texte: String,
    smtp: bool,
}

/// Brouillon visible par cet accès (sa boîte est dans la société de l'utilisateur).
async fn brouillon(etat: &Etat, acces: &Acces, id: i64) -> Resultat<BrouillonDroits> {
    sqlx::query_as!(
        BrouillonDroits,
        r#"select br.mail_id, br.statut, br.texte, (b.smtp_serveur is not null) as "smtp!"
           from brouillon_reponse br join boite_mail b on b.id = br.boite_id
           where br.id = $1 and ($2::bigint is null or b.societe_id = $2)"#,
        id,
        acces.societe_filtre()
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)
}

#[derive(Deserialize)]
pub struct EntreeTexte {
    texte: String,
}

pub async fn modifier_brouillon(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<EntreeTexte>,
) -> Resultat<StatusCode> {
    brouillon(&etat, &acces, id).await?;
    let n = sqlx::query!(
        "update brouillon_reponse set texte = $2, statut = 'modifie', erreur = null, modifie_le = now()
         where id = $1 and statut in ('propose', 'modifie')",
        id,
        e.texte
    )
    .execute(&etat.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Err(ErreurApi::Conflit(
            "ce brouillon n'est plus modifiable".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn abandonner_brouillon(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let b = brouillon(&etat, &acces, id).await?;
    let n = sqlx::query!(
        "update brouillon_reponse set statut = 'abandonne', modifie_le = now()
         where id = $1 and statut in ('propose', 'modifie')",
        id
    )
    .execute(&etat.pool)
    .await?;
    if n.rows_affected() == 0 {
        return Err(ErreurApi::Conflit(
            "ce brouillon n'est plus modifiable".into(),
        ));
    }
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "brouillon_abandonne",
        "mail",
        Some(b.mail_id.to_string()),
        json!({ "brouillon_id": id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Le clic « Envoyer » : seul chemin qui pousse l'envoi d'une réponse.
pub async fn envoyer_brouillon(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let b = brouillon(&etat, &acces, id).await?;
    if b.statut != "propose" && b.statut != "modifie" {
        return Err(ErreurApi::Conflit(
            "ce brouillon ne peut pas être envoyé dans son état actuel".into(),
        ));
    }
    if b.texte.trim().is_empty() {
        return Err(ErreurApi::Donnees("la réponse est vide".into()));
    }
    if !b.smtp {
        return Err(ErreurApi::ConflitCode {
            code: "smtp_absent",
            message: "le serveur SMTP de la boîte n'est pas configuré".into(),
        });
    }
    let mut tx = etat.pool.begin().await?;
    let n = sqlx::query!(
        "update brouillon_reponse set statut = 'envoi', erreur = null, modifie_le = now(),
             envoi_pris_le = null
         where id = $1 and statut in ('propose', 'modifie')",
        id
    )
    .execute(&mut *tx)
    .await?;
    if n.rows_affected() == 0 {
        return Err(ErreurApi::Conflit("brouillon modifié entre-temps".into()));
    }
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "reponse_demandee",
        "mail",
        Some(b.mail_id.to_string()),
        json!({ "brouillon_id": id }),
    )
    .await?;
    tx.commit().await?;
    file::pousser_envoi(
        &etat.pool,
        TacheEnvoi::EnvoyerReponse {
            brouillon_id: id,
            auteur_id: Some(acces.utilisateur_id),
        },
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct Marque {
    id: i64,
    adresse: Option<String>,
    domaine: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    cree_le: time::OffsetDateTime,
    cree_par: Option<String>,
}

pub async fn marques(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vec<Marque>>> {
    Ok(Json(
        sqlx::query_as!(
            Marque,
            r#"select m.id, m.adresse, m.domaine, m.cree_le, u.identifiant as "cree_par?"
               from expediteur_marque m left join utilisateur u on u.id = m.cree_par
               order by coalesce(m.domaine, m.adresse)"#
        )
        .fetch_all(&etat.pool)
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct EntreeMarque {
    mail_id: i64,
    /// `adresse` ou `domaine`.
    portee: String,
}

#[derive(Serialize)]
pub struct MarqueCreee {
    reclasses: u64,
}

/// « Toujours publicité » pour l'expéditeur ou le domaine d'un mail : les mails déjà reçus de
/// cet expéditeur (sauf catégorie fixée à la main) passent aussi en publicité.
pub async fn marquer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeMarque>,
) -> Resultat<Json<MarqueCreee>> {
    let expediteur = sqlx::query_scalar!("select expediteur from mail where id = $1", e.mail_id)
        .fetch_optional(&etat.pool)
        .await?
        .ok_or(ErreurApi::Absent)?
        .trim()
        .to_lowercase();
    let (adresse, domaine) = match e.portee.as_str() {
        "adresse" if expediteur.contains('@') => (Some(expediteur.clone()), None),
        "domaine" => (
            None,
            Some(
                tri::domaine(&expediteur)
                    .ok_or_else(|| ErreurApi::Donnees("expéditeur sans domaine".into()))?,
            ),
        ),
        _ => return Err(ErreurApi::Donnees("portée : adresse ou domaine".into())),
    };
    let mut tx = etat.pool.begin().await?;
    sqlx::query!(
        "insert into expediteur_marque (adresse, domaine, cree_par) values ($1, $2, $3)
         on conflict do nothing",
        adresse,
        domaine,
        a.utilisateur_id
    )
    .execute(&mut *tx)
    .await?;
    let n = sqlx::query!(
        "update mail set categorie = 'publicite',
            categorie_auto = case when id = $3 and categorie_par in ('regle', 'claude') then categorie else categorie_auto end,
            categorie_auto_par = case when id = $3 and categorie_par in ('regle', 'claude') then categorie_par else categorie_auto_par end,
            categorie_par = case when id = $3 then 'humain' else 'regle' end,
            tri_etat = 'trie', trie_le = now(), tri_message = 'expéditeur marqué comme publicité',
            statut_reponse = case when statut_reponse = 'a_repondre' then null else statut_reponse end
         where (id = $3 or categorie_par is distinct from 'humain')
           and ((($1::text is not null) and lower(expediteur) = $1)
             or (($2::text is not null) and (lower(split_part(expediteur, '@', 2)) = $2
                                            or lower(split_part(expediteur, '@', 2)) like '%.' || $2)))",
        adresse,
        domaine,
        e.mail_id
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "expediteur_publicite",
        "mail",
        Some(e.mail_id.to_string()),
        json!({ "adresse": adresse, "domaine": domaine, "reclasses": n }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(MarqueCreee { reclasses: n }))
}

pub async fn supprimer_marque(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let m = sqlx::query!(
        "delete from expediteur_marque where id = $1 returning adresse, domaine",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "expediteur_publicite_retire",
        "expediteur_marque",
        Some(id.to_string()),
        json!({ "adresse": m.adresse, "domaine": m.domaine }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
