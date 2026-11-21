use anyhow::{Context, Result};
use apalis_postgres::PostgresStorage;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower_sessions_sqlx_store::PostgresStore;

/// Clé du verrou d'initialisation ("GMFL" en ASCII).
const VERROU_INIT: i64 = 0x474D_464C;

/// Pool de connexions. Au moins 2 : l'une est tenue par le verrou pendant l'initialisation.
pub async fn connecter(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .min_connections(2)
        .connect(url)
        .await
        .context("connexion à PostgreSQL impossible")
}

/// Migrations GMFlow, puis apalis, puis sessions, sous un verrou propre à GMFlow :
/// l'app et le worker peuvent démarrer en même temps.
pub async fn initialiser(pool: &PgPool) -> Result<()> {
    let mut verrou = pool.acquire().await?;
    sqlx::query("select pg_advisory_lock($1)")
        .bind(VERROU_INIT)
        .execute(&mut *verrou)
        .await?;
    let resultat = async {
        sqlx::migrate!("../../migrations")
            .run(pool)
            .await
            .context("migrations GMFlow")?;
        PostgresStorage::setup(pool)
            .await
            .map_err(|e| anyhow::anyhow!("migrations apalis : {e}"))?;
        PostgresStore::new(pool.clone())
            .migrate()
            .await
            .context("migration des sessions")?;
        anyhow::Ok(())
    }
    .await;
    sqlx::query("select pg_advisory_unlock($1)")
        .bind(VERROU_INIT)
        .execute(&mut *verrou)
        .await?;
    resultat
}
