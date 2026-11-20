use anyhow::Result;
use gmflow_core::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    gmflow_core::initialiser_logs();
    let config = Config::depuis_env()?;
    if std::env::args().nth(1).as_deref() == Some("sante") {
        // healthcheck Docker : même contrôle que /health, sans curl.
        let ok = match gmflow_core::db::connecter(&config.database_url).await {
            Ok(pool) => sqlx::query("select 1").execute(&pool).await.is_ok(),
            Err(_) => false,
        };
        std::process::exit(if ok { 0 } else { 1 });
    }
    gmflow_app::lancer(config).await
}
