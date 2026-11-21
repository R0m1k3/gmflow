//! Test du jeton Claude : un appel court à haiku, puis mise à jour de `claude_etat`.

use gmflow_core::{claude, parametre};
use serde_json::{Value, json};

use crate::Contexte;

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub async fn tester(ctx: &Contexte) -> anyhow::Result<()> {
    let Some(chiffre) = parametre::lire_secret(&ctx.pool, parametre::CLAUDE_JETON).await? else {
        parametre::ecrire(
            &ctx.pool,
            parametre::CLAUDE_ETAT,
            &json!({ "etat": "absent", "message": "", "teste_le": maintenant() }),
        )
        .await?;
        return Ok(());
    };
    let jeton = ctx.chiffreur.dechiffrer_texte(&chiffre)?;
    let fin: String = jeton
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let precedent = parametre::lire(&ctx.pool, parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or(Value::Null);

    let r = claude::appeler(
        &ctx.pool,
        &ctx.config.claude_bin,
        "test_jeton",
        "haiku",
        None,
        "Réponds uniquement OK",
        &[],
        &jeton,
    )
    .await;
    let quand = maintenant();
    match r {
        Ok(_) => {
            parametre::ecrire(&ctx.pool, parametre::CLAUDE_ETAT, &json!({
                "etat": "valide", "message": "", "teste_le": quand, "utilise_le": quand, "fin": fin
            }))
            .await?;
            Ok(())
        }
        Err(e) => {
            parametre::ecrire(
                &ctx.pool,
                parametre::CLAUDE_ETAT,
                &json!({
                    "etat": "erreur", "message": e.to_string(), "teste_le": quand,
                    "utilise_le": precedent.get("utilise_le"), "fin": fin
                }),
            )
            .await?;
            Err(e.into())
        }
    }
}
