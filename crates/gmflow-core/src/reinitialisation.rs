//! Réinitialisation : efface tout ce qui a été tiré des mails (mails, pièces, documents,
//! fournisseurs, rapprochements, envois, réponses, historique, appels à Claude, tâches en
//! attente) et garde la configuration saisie (sociétés, boîtes mail, utilisateurs, réglages,
//! connexion à Claude, expéditeurs publicitaires, sessions). Les relèves repartent ensuite de
//! la date du jour (`boite_mail.reprise_depuis`) : seuls les mails du jour reviennent, pas
//! l'historique de `reprise_mois`. Les `.eml` restent sur disque : la relève les réécrit au même chemin.
//!
//! Concurrence avec le worker : refus si une relève tient son bail ou si un envoi (Dext/Yooz,
//! réponse) est en cours. Les boîtes sont verrouillées ligne par ligne avant tout, puis les
//! tables vidées le sont par `truncate` (verrou exclusif) : une relève ne peut pas prendre son
//! bail avant la fin, et une lecture ou un tri en cours n'écrit plus dans des lignes disparues
//! (mises à jour sans effet, insertions refusées par les clés étrangères).

use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;

use crate::{file, journal};

/// Tables vidées, des plus dépendantes aux plus centrales (ordre des verrous). Aucune table
/// gardée ne pointe vers elles : `truncate` sans `cascade` le vérifie à chaque fois.
pub const TABLES_EFFACEES: &[&str] = &[
    "logiflow_rapprochement",
    "logiflow_livraison",
    "ligne_document",
    "rapprochement",
    "envoi_depot",
    "brouillon_reponse",
    "mail_hors_societe",
    "fournisseur_distinct",
    "document",
    "fournisseur",
    "piece_jointe",
    "mail_boite",
    "boite_dossier",
    "mail",
    "journal",
    "appel_claude",
];

/// Un envoi plus ancien est tenu pour interrompu (même délai que le worker).
const ENVOI_EN_COURS_MINUTES: i32 = 15;
const ESSAIS: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Bilan {
    pub mails: i64,
    pub documents: i64,
    pub fournisseurs: i64,
    pub taches: u64,
    /// Boîtes actives dont la relève a été relancée.
    pub boites_relancees: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum Refus {
    #[error("Une relève est en cours, réessayez dans une minute")]
    ReleveEnCours,
    #[error("Un envoi est en cours, réessayez dans une minute")]
    EnvoiEnCours,
    #[error("La base est occupée, réessayez dans une minute")]
    Occupe,
    #[error(transparent)]
    Base(#[from] sqlx::Error),
}

fn occupe(e: &sqlx::Error) -> bool {
    // 40P01 : interblocage ; 55P03 : verrou pas obtenu à temps.
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "40P01" || c == "55P03")
}

/// Efface les données, journalise, puis relance la relève de chaque boîte active.
pub async fn reinitialiser(pool: &PgPool, auteur_id: Option<i64>) -> Result<Bilan, Refus> {
    let mut essai = 0;
    let mut bilan = loop {
        essai += 1;
        match effacer(pool, auteur_id).await {
            Err(Refus::Base(e)) if occupe(&e) => {
                if essai >= ESSAIS {
                    return Err(Refus::Occupe);
                }
                tokio::time::sleep(std::time::Duration::from_millis(200 * essai as u64)).await;
            }
            autre => break autre?,
        }
    };
    let boites = sqlx::query_scalar!(
        "select id from boite_mail where active and not pause_auto order by id"
    )
    .fetch_all(pool)
    .await?;
    for id in &boites {
        // Une relève non poussée repart au prochain tour du worker (5 min) : pas d'échec ici.
        match file::pousser_releve(pool, *id).await {
            Ok(()) => bilan.boites_relancees += 1,
            Err(e) => tracing::warn!(boite_id = id, erreur = ?e, "relève non relancée"),
        }
    }
    tracing::info!(?bilan, "réinitialisation faite");
    Ok(bilan)
}

async fn effacer(pool: &PgPool, auteur_id: Option<i64>) -> Result<Bilan, Refus> {
    let mut tx = pool.begin().await?;
    sqlx::query("set local lock_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    // Verrou sur chaque boîte : une relève ne prend plus son bail avant la fin.
    let en_releve = sqlx::query_scalar!(
        r#"select coalesce(bool_or(en_releve), false) as "r!"
           from (select releve_jusqu_a is not null and releve_jusqu_a > now() as en_releve
                 from boite_mail order by id for update) b"#
    )
    .fetch_one(&mut *tx)
    .await?;
    if en_releve {
        return Err(Refus::ReleveEnCours);
    }
    let en_envoi = sqlx::query_scalar!(
        r#"select exists (select 1 from envoi_depot
                          where etat = 'en_cours' and quand > now() - make_interval(mins => $1))
               or exists (select 1 from brouillon_reponse
                          where statut = 'envoi' and modifie_le > now() - make_interval(mins => $1))
           as "e!""#,
        ENVOI_EN_COURS_MINUTES
    )
    .fetch_one(&mut *tx)
    .await?;
    if en_envoi {
        return Err(Refus::EnvoiEnCours);
    }

    let liste = TABLES_EFFACEES.join(", ");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "lock table {liste} in access exclusive mode"
    )))
    .execute(&mut *tx)
    .await?;
    let (mails, documents, fournisseurs) = sqlx::query!(
        r#"select (select count(*) from mail) as "mails!",
                  (select count(*) from document) as "documents!",
                  (select count(*) from fournisseur) as "fournisseurs!""#
    )
    .fetch_one(&mut *tx)
    .await
    .map(|r| (r.mails, r.documents, r.fournisseurs))?;
    sqlx::query(sqlx::AssertSqlSafe(format!("truncate table {liste}")))
        .execute(&mut *tx)
        .await?;

    // Tâches pas encore prises qui portent sur les données effacées. Celles en cours finissent
    // seules ; la séance Claude, les essais de boîte, le test d'expédition et les alertes restent.
    let taches = sqlx::query(
        "delete from apalis.jobs
         where status <> 'Running'
           and (job_type = any($1)
                or (job_type = $2 and (convert_from(job, 'UTF8') like '%\"Deposer\"%'
                                       or convert_from(job, 'UTF8') like '%\"EnvoyerReponse\"%'))
                or (job_type = $3 and convert_from(job, 'UTF8') like '%\"Releve\"%'))",
    )
    .bind(&[file::FILE_LECTURE, file::FILE_TRI, file::FILE_REDACTION][..])
    .bind(file::FILE_ENVOI)
    .bind(file::FILE_BOITES)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // Relève remise à zéro : sans `boite_dossier`, elle repart de SINCE ; `reprise_depuis` (jour
    // de la réinitialisation, heure de Paris) la limite aux mails du jour.
    sqlx::query!(
        "update boite_mail set releve_jusqu_a = null, derniere_releve = null, derniere_erreur = null,
             erreur_depuis = null, alerte_le = null, echecs_auth = 0, pause_auto = false,
             reprise_depuis = (now() at time zone 'Europe/Paris')::date"
    )
    .execute(&mut *tx)
    .await?;

    journal::ecrire(
        &mut *tx,
        auteur_id,
        "reinitialisation",
        "donnees",
        None,
        json!({ "mails": mails, "documents": documents, "fournisseurs": fournisseurs, "taches": taches }),
    )
    .await?;
    tx.commit().await?;
    Ok(Bilan {
        mails,
        documents,
        fournisseurs,
        taches,
        boites_relancees: 0,
    })
}
