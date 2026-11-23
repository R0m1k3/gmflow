//! Pourquoi un document attend : Claude indisponible (pas connecté, refus, quota du jour),
//! lecture automatique coupée, place dans la file de lecture. Partagé par le worker (qui remet
//! un document en attente) et l'app (qui l'explique sur la fiche).
//!
//! Refus de Claude : un appel qui échoue sur une limite d'usage ou un jeton refusé pose
//! `claude_etat` en erreur (`refus`, `reprise`, `explication`) ; tant que la reprise n'est pas
//! passée (30 min sans heure de reprise), lecture et tri n'appellent plus Claude. Un appel
//! réussi remet l'état à `valide`.

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::claude::{self, GenreRefus, Refus, Reprise};
use crate::{file, parametre, reglages};

/// Voies de lecture qui passent par Claude.
pub const VOIES_CLAUDE: &[&str] = &["pdf_texte", "tableur", "scan"];
/// Sans heure de reprise, un refus bloque les appels pendant ce temps.
pub const REFUS_SANS_REPRISE_MIN: i64 = 30;

/// Claude ne peut pas servir maintenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Indispo {
    /// `jeton_absent`, `claude_refuse` ou `quota`.
    pub genre: &'static str,
    /// Phrase pour Michael, gardée aussi dans `document.message`.
    pub message: String,
}

fn rfc(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn lire_instant(v: &Value, cle: &str) -> Option<OffsetDateTime> {
    v.get(cle)
        .and_then(Value::as_str)
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
}

/// « vers 15:00 », « demain vers 01:00 » ou « le 25/11 vers 09:00 » (heure de Paris).
pub async fn quand_paris(pool: &PgPool, t: OffsetDateTime) -> sqlx::Result<String> {
    sqlx::query_scalar!(
        r#"select case ($1::timestamptz at time zone 'Europe/Paris')::date
                         - (now() at time zone 'Europe/Paris')::date
                    when 0 then 'vers ' || to_char($1 at time zone 'Europe/Paris', 'HH24:MI')
                    when 1 then 'demain vers ' || to_char($1 at time zone 'Europe/Paris', 'HH24:MI')
                    else 'le ' || to_char($1 at time zone 'Europe/Paris', 'DD/MM')
                         || ' vers ' || to_char($1 at time zone 'Europe/Paris', 'HH24:MI')
                  end as "q!""#,
        t
    )
    .fetch_one(pool)
    .await
}

/// Instant de reprise annoncé : le prochain passage à l'heure dite, dans le fuseau donné (connu
/// de PostgreSQL), UTC sinon.
async fn instant_reprise(pool: &PgPool, r: &Reprise) -> sqlx::Result<Option<OffsetDateTime>> {
    match r {
        Reprise::Instant(n) => Ok(OffsetDateTime::from_unix_timestamp(*n).ok()),
        Reprise::Heure {
            heure,
            minute,
            fuseau,
        } => {
            let t = sqlx::query_scalar!(
                r#"with f as (select coalesce((select name from pg_timezone_names where name = $3 limit 1), 'UTC') as tz),
                        x as (select (((now() at time zone f.tz)::date + make_time($1, $2, 0)) at time zone f.tz) as t from f)
                   select case when t > now() then t else t + interval '1 day' end as "t!" from x"#,
                i32::from(*heure),
                i32::from(*minute),
                fuseau.as_deref()
            )
            .fetch_one(pool)
            .await?;
            Ok(Some(t))
        }
    }
}

/// Note un refus général de Claude dans `claude_etat`. Journalisé seulement quand l'état change.
pub async fn noter_refus(pool: &PgPool, r: &Refus, brut: &str) {
    if let Err(e) = noter_refus_(pool, r, brut).await {
        tracing::warn!(erreur = %e, "écriture du refus de Claude impossible");
    }
}

async fn noter_refus_(pool: &PgPool, r: &Refus, brut: &str) -> sqlx::Result<()> {
    let reprise = match &r.reprise {
        Some(x) => instant_reprise(pool, x).await?,
        None => None,
    };
    let reprise_texte = match reprise {
        Some(t) => Some(quand_paris(pool, t).await?),
        None => None,
    };
    let explication = match (r.genre, &reprise_texte) {
        (GenreRefus::Limite, Some(q)) => format!("Claude refuse : limite d’usage, reprise {q}"),
        (GenreRefus::Limite, None) => "Claude refuse : limite d’usage atteinte".to_string(),
        (GenreRefus::Jeton, _) => {
            "Claude refuse la connexion : le jeton n’est plus accepté, reconnectez Claude dans Réglages > Claude"
                .to_string()
        }
    };
    let precedent = parametre::lire(pool, parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or(Value::Null);
    let deja = precedent.get("etat").and_then(Value::as_str) == Some("erreur")
        && precedent.get("refus").and_then(Value::as_str) == Some(r.genre.code())
        && precedent.get("explication").and_then(Value::as_str) == Some(explication.as_str());
    let maj = json!({
        "etat": "erreur",
        "message": brut.chars().take(500).collect::<String>(),
        "teste_le": rfc(OffsetDateTime::now_utc()),
        "refus": r.genre.code(),
        "reprise": reprise.map(rfc),
        "reprise_texte": reprise_texte,
        "explication": explication,
    });
    sqlx::query!(
        "insert into parametre (cle, valeur) values ($1, $2)
         on conflict (cle) do update
           set valeur = coalesce(parametre.valeur, '{}'::jsonb) || excluded.valeur, modifie_le = now()",
        parametre::CLAUDE_ETAT,
        maj
    )
    .execute(pool)
    .await?;
    if !deja {
        tracing::warn!(refus = r.genre.code(), %explication, "Claude refuse les appels");
    }
    Ok(())
}

/// Un appel réussi : Claude répond, l'état repasse à `valide` s'il était en erreur.
pub async fn noter_succes(pool: &PgPool) {
    let r = sqlx::query!(
        r#"update parametre
           set valeur = (valeur - 'refus' - 'reprise' - 'reprise_texte' - 'explication')
                        || '{"etat": "valide", "message": ""}'::jsonb,
               modifie_le = now()
           where cle = $1 and valeur ->> 'etat' = 'erreur'"#,
        parametre::CLAUDE_ETAT
    )
    .execute(pool)
    .await;
    match r {
        Ok(r) if r.rows_affected() > 0 => tracing::info!("Claude répond de nouveau"),
        Ok(_) => {}
        Err(e) => tracing::warn!(erreur = %e, "remise à zéro de l'état de Claude impossible"),
    }
}

/// Refus de Claude encore en vigueur (reprise pas encore passée).
pub async fn refus_actif(pool: &PgPool) -> sqlx::Result<Option<Indispo>> {
    let Some(v) = parametre::lire(pool, parametre::CLAUDE_ETAT).await? else {
        return Ok(None);
    };
    Ok(
        refus_en_vigueur(&v, OffsetDateTime::now_utc()).map(|message| Indispo {
            genre: "claude_refuse",
            message,
        }),
    )
}

/// Phrase du refus si `claude_etat` en note un qui vaut encore à `maintenant`.
pub fn refus_en_vigueur(v: &Value, maintenant: OffsetDateTime) -> Option<String> {
    if v.get("etat").and_then(Value::as_str) != Some("erreur") {
        return None;
    }
    v.get("refus").and_then(Value::as_str)?;
    let actif = match lire_instant(v, "reprise") {
        Some(t) => t > maintenant,
        None => lire_instant(v, "teste_le")
            .is_some_and(|t| maintenant - t < time::Duration::minutes(REFUS_SANS_REPRISE_MIN)),
    };
    actif.then(|| {
        v.get("explication")
            .and_then(Value::as_str)
            .unwrap_or("Claude refuse les appels")
            .to_string()
    })
}

/// Fin du jour de quota (minuit UTC), dite en heure de Paris.
async fn fin_du_quota(pool: &PgPool) -> sqlx::Result<String> {
    let t =
        sqlx::query_scalar!(r#"select date_trunc('day', now(), 'UTC') + interval '1 day' as "t!""#)
            .fetch_one(pool)
            .await?;
    quand_paris(pool, t).await
}

/// Claude peut-il lire maintenant ? `hors_quota` : demande d'un humain, le quota du jour ne
/// compte pas.
pub async fn lecture_indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
    hors_quota: bool,
) -> anyhow::Result<Option<Indispo>> {
    if !claude::acces_configure(pool).await? {
        return Ok(Some(absent("la lecture")));
    }
    if let Some(i) = refus_actif(pool).await? {
        return Ok(Some(i));
    }
    if !hors_quota && reglages::appels_lecture_du_jour(pool).await? >= r.quota_lecture_jour {
        return Ok(Some(Indispo {
            genre: "quota",
            message: format!(
                "Limite du jour atteinte ({} lectures par Claude) : la lecture reprend seule {}",
                r.quota_lecture_jour,
                fin_du_quota(pool).await?
            ),
        }));
    }
    Ok(None)
}

/// Claude peut-il trier maintenant ?
pub async fn tri_indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
) -> anyhow::Result<Option<Indispo>> {
    if !claude::acces_configure(pool).await? {
        return Ok(Some(absent("le tri")));
    }
    if let Some(i) = refus_actif(pool).await? {
        return Ok(Some(i));
    }
    if reglages::appels_tri_du_jour(pool).await? >= r.quota_tri_jour {
        return Ok(Some(Indispo {
            genre: "quota",
            message: format!(
                "Limite du jour atteinte ({} tris par Claude) : le tri reprend seul {}",
                r.quota_tri_jour,
                fin_du_quota(pool).await?
            ),
        }));
    }
    Ok(None)
}

fn absent(quoi: &str) -> Indispo {
    Indispo {
        genre: "jeton_absent",
        message: format!(
            "Claude n’est pas connecté : {quoi} reprendra dès qu’il le sera (Réglages > Claude)"
        ),
    }
}

/// Explication d'un document qui attend sa lecture (`a_lire` ou `en_cours`).
#[derive(Debug, Clone, Serialize)]
pub struct Attente {
    /// `en_cours`, `en_file`, `bloque`, `jeton_absent`, `claude_refuse`, `quota`,
    /// `lecture_coupee` ou `a_relancer`.
    pub genre: &'static str,
    /// Phrase principale, sans point final.
    pub raison: String,
    /// En attente depuis (création, ou dernière lecture pour une relecture).
    #[serde(with = "time::serde::rfc3339")]
    pub depuis: OffsetDateTime,
    /// Documents qui passeront avant lui dans la file de lecture.
    pub devant: Option<i64>,
    pub lecture_auto: bool,
    /// Raison notée au dernier essai (« Claude : délai de 300 s dépassé »), si autre.
    pub dernier_essai: Option<String>,
}

pub async fn expliquer(pool: &PgPool, document_id: i64) -> anyhow::Result<Option<Attente>> {
    let Some(d) = sqlx::query!(
        r#"select etat, voie, message, pris_le,
                  greatest(cree_le, coalesce(lu_le, cree_le)) as "depuis!",
                  coalesce(pris_le < now() - interval '30 minutes', false) as "bloque!",
                  case when planifie_le > now() - interval '30 minutes'
                       then planifie_le + interval '30 minutes' end as prochain
           from document where id = $1"#,
        document_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    if d.etat != "a_lire" && d.etat != "en_cours" {
        return Ok(None);
    }
    let r = reglages::lire(pool).await?;
    let mut a = Attente {
        genre: "en_cours",
        raison: String::new(),
        depuis: d.depuis,
        devant: None,
        lecture_auto: r.lecture_auto,
        dernier_essai: None,
    };
    if d.etat == "en_cours" {
        if d.bloque {
            a.genre = "bloque";
            a.raison = "La lecture ne répond plus : elle sera relancée automatiquement".into();
        } else {
            a.raison = match d.pris_le {
                Some(t) => format!(
                    "Lecture en cours (commencée {})",
                    quand_paris(pool, t).await?.replace("vers ", "à ")
                ),
                None => "Lecture en cours".into(),
            };
        }
        return Ok(Some(a));
    }
    let par_claude = d.voie.as_deref().is_none_or(|v| VOIES_CLAUDE.contains(&v));
    let indispo = if par_claude {
        lecture_indisponible(pool, &r, false).await?
    } else {
        None
    };
    match file::place_lecture(pool, document_id).await? {
        Some(file::PlaceEnFile::Prise) => {
            a.raison = "Lecture en cours".into();
        }
        Some(file::PlaceEnFile::EnAttente { devant, humain }) => {
            a.devant = Some(devant);
            match indispo {
                // La tâche le remettra en attente : la vraie raison est l'indisponibilité (le
                // quota ne compte pas pour une demande d'un humain).
                Some(i) if !(humain && i.genre == "quota") => {
                    a.genre = i.genre;
                    a.raison = i.message;
                }
                _ => {
                    a.genre = "en_file";
                    a.raison = match devant {
                        0 => "Dans la file de lecture : il passe dans quelques instants".into(),
                        1 => "Dans la file de lecture : 1 document passe avant lui".into(),
                        n => format!("Dans la file de lecture : {n} documents passent avant lui"),
                    };
                }
            }
        }
        None => {
            if let Some(i) = indispo {
                a.genre = i.genre;
                a.raison = i.message;
            } else if !r.lecture_auto {
                a.genre = "lecture_coupee";
                a.raison = "La lecture automatique est coupée (Réglages > Claude) : il attend qu’un administrateur le lance".into();
            } else {
                a.genre = "a_relancer";
                a.raison = match d.prochain {
                    Some(t) => format!(
                        "Il repartira en lecture tout seul {}",
                        quand_paris(pool, t).await?
                    ),
                    None => "Il repartira en lecture tout seul dans 5 minutes au plus".into(),
                };
            }
        }
    }
    a.dernier_essai = d
        .message
        .filter(|m| !m.is_empty() && m.trim_end_matches('.') != a.raison);
    Ok(Some(a))
}
