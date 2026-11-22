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
    LireDocument { document_id: i64, forcer: bool },
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

pub fn stockage_lecture(pool: &PgPool) -> PostgresStorage<TacheLecture> {
    PostgresStorage::new(pool).with_config(Config::default().queue(FILE_LECTURE))
}

pub async fn pousser_lecture(pool: &PgPool, document_id: i64, forcer: bool) -> anyhow::Result<()> {
    let tache = TaskBuilder::new(TacheLecture::LireDocument {
        document_id,
        forcer,
    })
    .max_attempts(1)
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
