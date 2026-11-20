use std::time::Duration;

use sqlx::PgPool;

use crate::Contexte;

#[derive(Clone, Copy, Debug)]
pub struct DelaisSeance {
    pub url: Duration,
    pub jeton: Duration,
    pub code: Duration,
}

impl Default for DelaisSeance {
    fn default() -> Self {
        Self { url: Duration::from_secs(180), jeton: Duration::from_secs(60), code: Duration::from_secs(600) }
    }
}

pub async fn fermer_seances_ouvertes(_pool: &PgPool) -> anyhow::Result<()> {
    Ok(())
}

pub async fn executer(_ctx: &Contexte, _seance: uuid::Uuid) -> anyhow::Result<()> {
    anyhow::bail!("pas encore")
}
