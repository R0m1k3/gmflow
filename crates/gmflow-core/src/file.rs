//! Six files apalis : `boites` (relève, test de connexion), `claude` (séance setup-token,
//! test du jeton), `lecture` (lecture des pièces jointes), `envoi` (dépôts Dext/Yooz, test
//! de l'expédition, alertes, réponses aux mails), `tri` (tri des mails) et `redaction`
//! (brouillons de réponse, à la demande). Chaque tâche est poussée avec un nombre d'essais explicite : apalis rejoue
//! sinon jusqu'à 25 fois.

use apalis::prelude::{TaskBuilder, TaskSink};
use apalis_postgres::{Config, PostgresStorage};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

pub const FILE_BOITES: &str = "boites";
pub const FILE_CLAUDE: &str = "claude";
pub const FILE_LECTURE: &str = "lecture";
pub const FILE_ENVOI: &str = "envoi";
pub const FILE_TRI: &str = "tri";
pub const FILE_REDACTION: &str = "redaction";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheTri {
    TrierMail { mail_id: i64 },
}

/// Brouillon de réponse, généré seulement quand un utilisateur le demande. File à part : un
/// clic n'attend pas derrière le tri de 200 mails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheRedaction {
    GenererBrouillon { brouillon_id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheBoite {
    Releve {
        boite_id: i64,
    },
    TestBoite {
        boite_id: i64,
    },
    /// Essai de paramètres pas encore enregistrés, ou essai SMTP (module `essai_boite`).
    Essai {
        essai_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheClaude {
    SeanceSetupToken { seance_id: Uuid },
    TestJeton,
}

/// `forcer` : relecture demandée par un administrateur (pas de reprise d'un document identique).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheLecture {
    LireDocument {
        document_id: i64,
        forcer: bool,
        /// Relecture « en détail » demandée par un humain : directement le réglage du nouvel
        /// essai (modèle plus fort), fichier d'origine joint.
        #[serde(default)]
        approfondie: bool,
        /// Demande d'un humain sur un seul document : le quota du jour ne compte pas.
        #[serde(default)]
        hors_quota: bool,
    },
}

/// Tâches d'envoi de mails depuis l'adresse d'expédition. Jamais rejouées par apalis : un
/// dépôt en échec se relance depuis l'interface, pour ne jamais déposer deux fois.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheEnvoi {
    /// Dépôt d'une facture. `forcer` : un administrateur passe outre un doublon.
    Deposer {
        document_id: i64,
        forcer: bool,
        auteur_id: Option<i64>,
    },
    TestExpedition {
        destinataire: String,
    },
    /// Essai de connexion à l'API du serveur (adresse et jeton des réglages).
    TestServeurApi,
    /// Alertes de relève (boîte en échec depuis plus de 30 min, puis rétablie).
    Alertes,
    /// Réponse à un mail, depuis la boîte d'origine. Poussée uniquement par le clic « Envoyer »
    /// d'un utilisateur ; jamais rejouée.
    EnvoyerReponse {
        brouillon_id: i64,
        auteur_id: Option<i64>,
    },
}

pub fn stockage_tri(pool: &PgPool) -> PostgresStorage<TacheTri> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_TRI))
}

pub async fn pousser_tri(pool: &PgPool, mail_id: i64) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(TacheTri::TrierMail { mail_id })
        .max_attempts(1)
        .build();
    stockage_tri(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file tri : {e}"))
}

pub fn stockage_redaction(pool: &PgPool) -> PostgresStorage<TacheRedaction> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_REDACTION))
}

pub async fn pousser_brouillon(pool: &PgPool, brouillon_id: i64) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(TacheRedaction::GenererBrouillon { brouillon_id })
        .max_attempts(1)
        .build();
    stockage_redaction(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file redaction : {e}"))
}

pub fn stockage_envoi(pool: &PgPool) -> PostgresStorage<TacheEnvoi> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_ENVOI))
}

pub async fn pousser_envoi(pool: &PgPool, tache: TacheEnvoi) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(tache).max_attempts(1).build();
    stockage_envoi(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file envoi : {e}"))
}

/// Priorités de la file `lecture` (apalis sert `priority desc`, puis la plus ancienne) : une
/// demande d'un humain passe devant tout, un mail de moins de 24 h devant l'arriéré.
pub const PRIORITE_HUMAIN: usize = 10;
pub const PRIORITE_RECENT: usize = 5;
pub const PRIORITE_ARRIERE: usize = 0;

/// Lots de 2 au plus : une tâche plus prioritaire poussée ensuite n'attend pas derrière dix
/// tâches déjà prises par le worker.
pub fn stockage_lecture(pool: &PgPool) -> PostgresStorage<TacheLecture> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_LECTURE).batch_size(2))
}

/// Lecture poussée à la main (relecture des BL, « Relire »), `forcer` : pas de reprise d'un
/// document identique. Le quota du jour s'applique.
pub async fn pousser_lecture(pool: &PgPool, document_id: i64, forcer: bool) -> anyhow::Result<()> {
    pousser(pool, document_id, forcer, false, false, PRIORITE_RECENT).await
}

/// « Lire maintenant » / « Relire » sur une fiche (administrateur) : devant toute la file, hors
/// quota du jour.
pub async fn pousser_lecture_maintenant(pool: &PgPool, document_id: i64) -> anyhow::Result<()> {
    pousser(pool, document_id, true, false, true, PRIORITE_HUMAIN).await
}

/// Lecture automatique (pompe) : priorité selon l'âge du mail.
pub async fn pousser_lecture_auto(
    pool: &PgPool,
    document_id: i64,
    priorite: usize,
) -> anyhow::Result<()> {
    pousser(pool, document_id, false, false, false, priorite).await
}

/// Relecture en détail (modèle plus fort, fichier d'origine joint), toujours forcée, devant
/// toute la file, hors quota du jour.
pub async fn pousser_relecture_approfondie(pool: &PgPool, document_id: i64) -> anyhow::Result<()> {
    pousser(pool, document_id, true, true, true, PRIORITE_HUMAIN).await
}

async fn pousser(
    pool: &PgPool,
    document_id: i64,
    forcer: bool,
    approfondie: bool,
    hors_quota: bool,
    priorite: usize,
) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(TacheLecture::LireDocument {
        document_id,
        forcer,
        approfondie,
        hors_quota,
    })
    .max_attempts(1)
    .priority(priorite)
    .build();
    stockage_lecture(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file lecture : {e}"))
}

pub fn stockage_boites(pool: &PgPool) -> PostgresStorage<TacheBoite> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_BOITES))
}

pub fn stockage_claude(pool: &PgPool) -> PostgresStorage<TacheClaude> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_CLAUDE))
}

async fn pousser_boite(pool: &PgPool, tache: TacheBoite) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(tache).max_attempts(1).build();
    stockage_boites(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file boites : {e}"))
}

async fn pousser_claude(pool: &PgPool, tache: TacheClaude, essais: usize) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(tache).max_attempts(essais).build();
    stockage_claude(pool)
        .push_task(tache)
        .await
        .map_err(|e| anyhow::anyhow!("file claude : {e}"))
}

pub async fn pousser_releve(pool: &PgPool, boite_id: i64) -> anyhow::Result<()> {
    pousser_boite(pool, TacheBoite::Releve { boite_id }).await
}

pub async fn pousser_test_boite(pool: &PgPool, boite_id: i64) -> anyhow::Result<()> {
    pousser_boite(pool, TacheBoite::TestBoite { boite_id }).await
}

pub async fn pousser_essai_boite(pool: &PgPool, essai_id: Uuid) -> anyhow::Result<()> {
    pousser_boite(pool, TacheBoite::Essai { essai_id }).await
}

pub async fn pousser_seance(pool: &PgPool, seance_id: Uuid) -> anyhow::Result<()> {
    pousser_claude(pool, TacheClaude::SeanceSetupToken { seance_id }, 1).await
}

pub async fn pousser_test_jeton(pool: &PgPool) -> anyhow::Result<()> {
    pousser_claude(pool, TacheClaude::TestJeton, 2).await
}

/// Une tâche `running` plus vieille que ça est une tâche fantôme (worker mort pendant la
/// lecture, accusé de fin perdu) : elle ne compte plus comme « en file ».
pub const TACHE_FANTOME_MIN: i32 = 30;

/// Documents qui ont déjà une tâche `lecture` en attente ou en cours (args JSON de la tâche :
/// `{"LireDocument": {"document_id": 12, ...}}`).
pub async fn documents_en_file<'e>(db: impl sqlx::PgExecutor<'e>) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar!(
        r#"select distinct (convert_from(job, 'UTF8')::jsonb #>> '{LireDocument,document_id}')::bigint as "id!"
           from apalis.jobs
           where job_type = $1
             and (status in ('Pending', 'Queued')
                  or (status = 'Running' and lock_at > now() - make_interval(mins => $2)))
             and convert_from(job, 'UTF8')::jsonb #>> '{LireDocument,document_id}' is not null"#,
        FILE_LECTURE,
        TACHE_FANTOME_MIN
    )
    .fetch_all(db)
    .await
}

/// Place d'un document dans la file `lecture`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PlaceEnFile {
    /// Tâche prise par le worker (lecture en cours ou toute proche).
    Prise,
    /// En attente : `devant` tâches passeront avant elle ; `humain` : demandée par un humain.
    EnAttente { devant: i64, humain: bool },
}

/// Place du document dans la file de lecture, `None` s'il n'y a aucune tâche pour lui.
pub async fn place_lecture(pool: &PgPool, document_id: i64) -> sqlx::Result<Option<PlaceEnFile>> {
    let id = document_id.to_string();
    let Some(t) = sqlx::query!(
        r#"select id as "id!", status as "status!", priority, run_at as "run_at!" from apalis.jobs
           where job_type = $1
             and (status in ('Pending', 'Queued')
                  or (status = 'Running' and lock_at > now() - make_interval(mins => $3)))
             and convert_from(job, 'UTF8')::jsonb #>> '{LireDocument,document_id}' = $2
           order by (status = 'Pending'), priority desc nulls last, run_at
           limit 1"#,
        FILE_LECTURE,
        id,
        TACHE_FANTOME_MIN
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    if t.status != "Pending" {
        return Ok(Some(PlaceEnFile::Prise));
    }
    let devant = sqlx::query_scalar!(
        r#"select count(*) as "n!" from apalis.jobs
           where job_type = $1
             and (status = 'Queued'
                  or (status = 'Pending' and run_at <= now()
                      and (coalesce(priority, 0) > $2
                           or (coalesce(priority, 0) = $2
                               and (run_at < $3 or (run_at = $3 and id < $4))))))"#,
        FILE_LECTURE,
        t.priority.unwrap_or(0),
        t.run_at,
        t.id
    )
    .fetch_one(pool)
    .await?;
    let priorite = t.priority.unwrap_or(0);
    Ok(Some(PlaceEnFile::EnAttente {
        devant,
        humain: priorite >= PRIORITE_HUMAIN as i32,
    }))
}
