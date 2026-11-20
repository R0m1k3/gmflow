//! Deux files apalis : `boites` (relève, test de connexion) et `claude` (séance setup-token,
//! test du jeton). Chaque tâche est poussée avec un nombre d'essais explicite : apalis rejoue
//! sinon jusqu'à 25 fois.

use apalis::prelude::{TaskBuilder, TaskSink};
use apalis_postgres::{Config, PostgresStorage};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

pub const FILE_BOITES: &str = "boites";
pub const FILE_CLAUDE: &str = "claude";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheBoite {
    Releve { boite_id: i64 },
    TestBoite { boite_id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TacheClaude {
    SeanceSetupToken { seance_id: Uuid },
    TestJeton,
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

pub async fn pousser_seance(pool: &PgPool, seance_id: Uuid) -> anyhow::Result<()> {
    pousser_claude(pool, TacheClaude::SeanceSetupToken { seance_id }, 1).await
}

pub async fn pousser_test_jeton(pool: &PgPool) -> anyhow::Result<()> {
    pousser_claude(pool, TacheClaude::TestJeton, 2).await
}
