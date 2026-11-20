//! Table `parametre` : configuration globale (jsonb) et secrets chiffrés.

use serde_json::Value;
use sqlx::PgExecutor;

pub const CLAUDE_JETON: &str = "claude_jeton";
pub const CLAUDE_ETAT: &str = "claude_etat";
pub const WORKER_BATTEMENT: &str = "worker_battement";

pub async fn lire<'e>(db: impl PgExecutor<'e>, cle: &str) -> sqlx::Result<Option<Value>> {
    Ok(sqlx::query_scalar!("select valeur from parametre where cle = $1", cle)
        .fetch_optional(db)
        .await?
        .flatten())
}

pub async fn ecrire<'e>(db: impl PgExecutor<'e>, cle: &str, valeur: &Value) -> sqlx::Result<()> {
    sqlx::query!(
        "insert into parametre (cle, valeur) values ($1, $2)
         on conflict (cle) do update set valeur = excluded.valeur, modifie_le = now()",
        cle,
        valeur
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn lire_secret<'e>(db: impl PgExecutor<'e>, cle: &str) -> sqlx::Result<Option<Vec<u8>>> {
    Ok(sqlx::query_scalar!("select valeur_chiffree from parametre where cle = $1", cle)
        .fetch_optional(db)
        .await?
        .flatten())
}

pub async fn ecrire_secret<'e>(db: impl PgExecutor<'e>, cle: &str, chiffre: &[u8]) -> sqlx::Result<()> {
    sqlx::query!(
        "insert into parametre (cle, valeur_chiffree) values ($1, $2)
         on conflict (cle) do update set valeur_chiffree = excluded.valeur_chiffree, modifie_le = now()",
        cle,
        chiffre
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn supprimer<'e>(db: impl PgExecutor<'e>, cle: &str) -> sqlx::Result<()> {
    sqlx::query!("delete from parametre where cle = $1", cle).execute(db).await?;
    Ok(())
}
