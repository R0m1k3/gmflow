use serde_json::Value;
use sqlx::PgExecutor;

/// Écrit une ligne dans le journal des actions.
pub async fn ecrire<'e>(
    db: impl PgExecutor<'e>,
    auteur_id: Option<i64>,
    action: &str,
    objet_type: &str,
    objet_id: Option<String>,
    details: Value,
) -> sqlx::Result<()> {
    sqlx::query!(
        "insert into journal (auteur_id, action, objet_type, objet_id, details) values ($1, $2, $3, $4, $5)",
        auteur_id,
        action,
        objet_type,
        objet_id,
        details
    )
    .execute(db)
    .await?;
    Ok(())
}
