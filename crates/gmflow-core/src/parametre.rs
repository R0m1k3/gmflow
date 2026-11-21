//! Table `parametre` : configuration globale (jsonb) et secrets chiffrés.

use serde_json::Value;
use sqlx::PgExecutor;

pub const CLAUDE_JETON: &str = "claude_jeton";
pub const CLAUDE_ETAT: &str = "claude_etat";
/// Clé API Anthropic (chiffrée), alternative au setup token ; `valeur` = `{"fin": "abcd"}`.
pub const CLAUDE_CLE_API: &str = "claude_cle_api";
/// Secret utilisé pour les appels : `{"mode": "jeton" | "cle_api"}`.
pub const CLAUDE_ACCES: &str = "claude_acces";
pub const WORKER_BATTEMENT: &str = "worker_battement";

pub async fn lire<'e>(db: impl PgExecutor<'e>, cle: &str) -> sqlx::Result<Option<Value>> {
    Ok(
        sqlx::query_scalar!("select valeur from parametre where cle = $1", cle)
            .fetch_optional(db)
            .await?
            .flatten(),
    )
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
    Ok(
        sqlx::query_scalar!("select valeur_chiffree from parametre where cle = $1", cle)
            .fetch_optional(db)
            .await?
            .flatten(),
    )
}

pub async fn ecrire_secret<'e>(
    db: impl PgExecutor<'e>,
    cle: &str,
    chiffre: &[u8],
) -> sqlx::Result<()> {
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
    sqlx::query!("delete from parametre where cle = $1", cle)
        .execute(db)
        .await?;
    Ok(())
}

/// Enregistre le jeton Claude (chiffré) et note qu'il reste à tester. Le jeton n'est jamais relu
/// par l'API : seuls ses 4 derniers caractères sont gardés en clair.
pub async fn enregistrer_jeton_claude(
    pool: &sqlx::PgPool,
    chiffreur: &crate::crypto::Chiffreur,
    jeton: &str,
) -> sqlx::Result<()> {
    let chiffre = chiffreur.chiffrer(jeton.as_bytes());
    let fin: String = jeton
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut tx = pool.begin().await?;
    ecrire_secret(&mut *tx, CLAUDE_JETON, &chiffre).await?;
    ecrire(
        &mut *tx,
        CLAUDE_ETAT,
        &serde_json::json!({ "etat": "en_test", "message": "", "fin": fin }),
    )
    .await?;
    tx.commit().await
}
