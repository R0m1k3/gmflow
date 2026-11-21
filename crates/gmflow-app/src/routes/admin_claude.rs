//! Accès Claude : setup token (collage, séance setup-token) ou clé API Anthropic, test, réglages
//! par tâche. Aucun secret n'est jamais renvoyé : seulement ses 4 derniers caractères.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::{file, journal, parametre, reglages};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

pub async fn etat(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Value>> {
    let mut v = parametre::lire(&etat.pool, parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or_else(|| json!({ "etat": "absent" }));
    let mode = gmflow_core::claude::mode_acces(&etat.pool).await?;
    let jeton = parametre::lire_secret(&etat.pool, parametre::CLAUDE_JETON)
        .await?
        .is_some();
    let cle = parametre::lire_secret(&etat.pool, parametre::CLAUDE_CLE_API)
        .await?
        .is_some();
    let fin_cle = parametre::lire(&etat.pool, parametre::CLAUDE_CLE_API)
        .await?
        .and_then(|v| v.get("fin").cloned());
    if v.is_object() {
        v["acces"] =
            json!({ "mode": mode, "jeton": jeton, "cle_api": cle, "cle_api_fin": fin_cle });
    }
    Ok(Json(v))
}

fn fin(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    c[c.len().saturating_sub(4)..].iter().collect()
}

#[derive(Deserialize)]
pub struct EntreeCle {
    cle: String,
}

/// Clé API Anthropic, alternative au setup token (facturée à l'usage).
pub async fn enregistrer_cle(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeCle>,
) -> Resultat<StatusCode> {
    let cle = e.cle.trim();
    if !cle.starts_with("sk-ant-api") || cle.len() < 20 || cle.chars().any(char::is_whitespace) {
        return Err(ErreurApi::Donnees("clé attendue : sk-ant-api…".into()));
    }
    let chiffre = etat.chiffreur.chiffrer(cle.as_bytes());
    let mode = gmflow_core::claude::mode_acces(&etat.pool).await?;
    let mut tx = etat.pool.begin().await?;
    parametre::ecrire_secret(&mut *tx, parametre::CLAUDE_CLE_API, &chiffre).await?;
    parametre::ecrire(
        &mut *tx,
        parametre::CLAUDE_CLE_API,
        &json!({ "fin": fin(cle) }),
    )
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "cle_api_enregistree",
        "claude",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    if mode == "cle_api" {
        file::pousser_test_jeton(&etat.pool).await?;
    }
    Ok(StatusCode::ACCEPTED)
}

/// Supprime la clé API ; si elle était utilisée, GMFlow revient au setup token.
pub async fn supprimer_cle(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<StatusCode> {
    let mode = gmflow_core::claude::mode_acces(&etat.pool).await?;
    let mut tx = etat.pool.begin().await?;
    parametre::supprimer(&mut *tx, parametre::CLAUDE_CLE_API).await?;
    if mode == "cle_api" {
        parametre::ecrire(
            &mut *tx,
            parametre::CLAUDE_ACCES,
            &json!({ "mode": "jeton" }),
        )
        .await?;
    }
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "cle_api_supprimee",
        "claude",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    if mode == "cle_api" {
        file::pousser_test_jeton(&etat.pool).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct EntreeAcces {
    mode: String,
}

/// Choix du secret utilisé pour les appels : `jeton` ou `cle_api`.
pub async fn choisir_acces(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeAcces>,
) -> Resultat<StatusCode> {
    let cle = match e.mode.as_str() {
        "jeton" => parametre::CLAUDE_JETON,
        "cle_api" => parametre::CLAUDE_CLE_API,
        _ => return Err(ErreurApi::Donnees("mode : jeton ou cle_api".into())),
    };
    if parametre::lire_secret(&etat.pool, cle).await?.is_none() {
        return Err(ErreurApi::Conflit(if e.mode == "jeton" {
            "aucun setup token enregistré".into()
        } else {
            "aucune clé API enregistrée".into()
        }));
    }
    let mut tx = etat.pool.begin().await?;
    parametre::ecrire(
        &mut *tx,
        parametre::CLAUDE_ACCES,
        &json!({ "mode": e.mode }),
    )
    .await?;
    parametre::ecrire(
        &mut *tx,
        parametre::CLAUDE_ETAT,
        &json!({ "etat": "en_test", "message": "" }),
    )
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "acces_claude",
        "claude",
        None,
        json!({ "mode": e.mode }),
    )
    .await?;
    tx.commit().await?;
    file::pousser_test_jeton(&etat.pool).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct EntreeJeton {
    jeton: String,
}

pub async fn coller(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<EntreeJeton>,
) -> Resultat<StatusCode> {
    let jeton = e.jeton.trim();
    if !jeton.starts_with("sk-ant-") || jeton.len() < 20 || jeton.chars().any(char::is_whitespace) {
        return Err(ErreurApi::Donnees("jeton attendu : sk-ant-…".into()));
    }
    parametre::enregistrer_jeton_claude(&etat.pool, &etat.chiffreur, jeton).await?;
    file::pousser_test_jeton(&etat.pool).await?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "jeton_colle",
        "claude",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn supprimer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<StatusCode> {
    let mode = gmflow_core::claude::mode_acces(&etat.pool).await?;
    let mut tx = etat.pool.begin().await?;
    parametre::supprimer(&mut *tx, parametre::CLAUDE_JETON).await?;
    // L'état décrit le secret utilisé : il ne change que si c'était le setup token.
    if mode == "jeton" {
        parametre::ecrire(
            &mut *tx,
            parametre::CLAUDE_ETAT,
            &json!({ "etat": "absent", "message": "" }),
        )
        .await?;
    }
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "jeton_supprime",
        "claude",
        None,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tester(AccesAdmin(a): AccesAdmin, State(etat): State<Etat>) -> Resultat<StatusCode> {
    file::pousser_test_jeton(&etat.pool).await?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test_jeton",
        "claude",
        None,
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct SeanceCreee {
    id: Uuid,
}

pub async fn nouvelle_seance(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<Json<SeanceCreee>> {
    let mut tx = etat.pool.begin().await?;
    // Une séance jamais prise par le worker (arrêté) ne bloque pas les suivantes au-delà de son délai.
    sqlx::query!(
        "update seance_claude set etat = 'expiree', message = 'délai dépassé', code_chiffre = null
         where etat in ('demarrage', 'attente_code', 'code_recu') and expire_le < now()"
    )
    .execute(&mut *tx)
    .await?;
    let id =
        sqlx::query_scalar!("insert into seance_claude (etat) values ('demarrage') returning id")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| match ErreurApi::from(e) {
                ErreurApi::Conflit(_) => ErreurApi::Conflit("une séance est déjà en cours".into()),
                autre => autre,
            })?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "seance_setup_token",
        "claude",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    file::pousser_seance(&etat.pool, id).await?;
    Ok(Json(SeanceCreee { id }))
}

#[derive(Serialize)]
pub struct Seance {
    etat: String,
    url: Option<String>,
    message: Option<String>,
}

pub async fn seance(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<Uuid>,
) -> Resultat<Json<Seance>> {
    let s = sqlx::query_as!(
        Seance,
        "select etat, url, message from seance_claude where id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    Ok(Json(s))
}

#[derive(Deserialize)]
pub struct EntreeCode {
    code: String,
}

pub async fn code(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<Uuid>,
    Json(e): Json<EntreeCode>,
) -> Resultat<StatusCode> {
    let code = e.code.trim();
    if code.is_empty() || code.len() > 500 {
        return Err(ErreurApi::Donnees("code invalide".into()));
    }
    let chiffre = etat.chiffreur.chiffrer(code.as_bytes());
    let n = sqlx::query!(
        "update seance_claude set etat = 'code_recu', code_chiffre = $2
         where id = $1 and etat = 'attente_code' and expire_le > now()",
        id,
        chiffre
    )
    .execute(&etat.pool)
    .await?
    .rows_affected();
    if n == 0 {
        let existe = sqlx::query_scalar!(
            "select exists (select 1 from seance_claude where id = $1) as \"ok!\"",
            id
        )
        .fetch_one(&etat.pool)
        .await?;
        return Err(if existe {
            ErreurApi::Conflit("la séance n'attend pas de code".into())
        } else {
            ErreurApi::Absent
        });
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct ReglagesVue {
    #[serde(flatten)]
    reglages: reglages::Reglages,
    appels_lecture_du_jour: i64,
    appels_tri_du_jour: i64,
    modeles: &'static [&'static str],
    efforts: &'static [&'static str],
}

pub async fn reglages(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<ReglagesVue>> {
    Ok(Json(ReglagesVue {
        reglages: reglages::lire(&etat.pool).await?,
        appels_lecture_du_jour: reglages::appels_lecture_du_jour(&etat.pool).await?,
        appels_tri_du_jour: reglages::appels_tri_du_jour(&etat.pool).await?,
        modeles: reglages::MODELES,
        efforts: reglages::EFFORTS,
    }))
}

pub async fn modifier_reglages(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(r): Json<reglages::Reglages>,
) -> Resultat<Json<reglages::Reglages>> {
    r.controler().map_err(ErreurApi::Donnees)?;
    let avant = reglages::lire(&etat.pool).await?;
    reglages::ecrire(&etat.pool, &r).await?;
    journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "reglages_claude",
        "parametre",
        Some(reglages::CLE.into()),
        json!({ "avant": avant, "apres": r }),
    )
    .await?;
    Ok(Json(r))
}

#[derive(Deserialize)]
pub struct PageAppels {
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct AppelVue {
    #[serde(with = "time::serde::rfc3339")]
    quand: time::OffsetDateTime,
    tache: String,
    modele: String,
    effort: Option<String>,
    duree_ms: i32,
    succes: bool,
    message: Option<String>,
}

#[derive(Serialize)]
pub struct StatTache {
    tache: String,
    modele: String,
    n: i64,
    reussis: i64,
    duree_moyenne_ms: i64,
}

#[derive(Serialize)]
pub struct Appels {
    #[serde(flatten)]
    page: super::mails::Page<AppelVue>,
    /// Sur les 30 derniers jours, par tâche et modèle.
    stats: Vec<StatTache>,
}

/// Journal des appels Claude (tâche, modèle, effort, durée, résultat).
pub async fn appels(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    axum::extract::Query(p): axum::extract::Query<PageAppels>,
) -> Resultat<Json<Appels>> {
    let (page, decalage) = super::mails::decalage(p.page);
    let total = sqlx::query_scalar!(r#"select count(*) as "n!" from appel_claude"#)
        .fetch_one(&etat.pool)
        .await?;
    let elements = sqlx::query_as!(
        AppelVue,
        "select quand, tache, modele, effort, duree_ms, succes, message from appel_claude
         order by quand desc, id desc limit $1 offset $2",
        super::mails::PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    let stats = sqlx::query_as!(
        StatTache,
        r#"select tache, modele, count(*) as "n!", count(*) filter (where succes) as "reussis!",
                  coalesce(avg(duree_ms), 0)::int8 as "duree_moyenne_ms!"
           from appel_claude where quand > now() - interval '30 days'
           group by tache, modele order by tache, modele"#
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Appels {
        page: super::mails::Page {
            total,
            page,
            elements,
        },
        stats,
    }))
}
