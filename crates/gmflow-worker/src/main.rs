use anyhow::Result;
use gmflow_core::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    gmflow_core::initialiser_logs();
    let config = Config::depuis_env()?;
    if std::env::args().nth(1).as_deref() == Some("sante") {
        let ok = match gmflow_core::db::connecter(&config.database_url).await {
            Ok(pool) => gmflow_worker::sante::controle(&pool).await,
            Err(_) => false,
        };
        std::process::exit(if ok { 0 } else { 1 });
    }
    gmflow_worker::lancer(config).await
}
