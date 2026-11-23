//! Test de l'accès Claude (setup token ou clé API, selon le mode choisi) : un appel court avec
//! le modèle et l'effort de l'extraction (pour vérifier aussi ce réglage), puis mise à jour de
//! `claude_etat`.

use gmflow_core::{claude, parametre, reglages};
use serde_json::{Value, json};

use crate::Contexte;

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub async fn tester(ctx: &Contexte) -> anyhow::Result<()> {
    let Some(acces) = claude::acces(&ctx.pool, &ctx.chiffreur).await? else {
        parametre::ecrire(
            &ctx.pool,
            parametre::CLAUDE_ETAT,
            &json!({ "etat": "absent", "message": "", "teste_le": maintenant() }),
        )
        .await?;
        return Ok(());
    };
    let fin: String = acces
        .secret
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

    let reglage = reglages::lire(&ctx.pool).await?.extraction_texte;
    let mut a = claude::Appel::avec_acces(
        "test_jeton",
        &reglage.modele,
        "Réponds uniquement OK",
        &acces,
    );
    a.effort = Some(&reglage.effort);
    let r = claude::lancer(&ctx.pool, &ctx.config.claude_bin, &a).await;
    let quand = maintenant();
    match r {
        Ok(_) => {
            parametre::ecrire(&ctx.pool, parametre::CLAUDE_ETAT, &json!({
                "etat": "valide", "message": "", "teste_le": quand, "utilise_le": quand, "fin": fin,
                "cle_api": acces.cle_api
            }))
            .await?;
            Ok(())
        }
        Err(e) => {
            let mut v = json!({
                "etat": "erreur", "message": e.to_string(), "teste_le": quand,
                "utilise_le": precedent.get("utilise_le"), "fin": fin, "cle_api": acces.cle_api
            });
            // Refus (limite d'usage, jeton refusé) : `claude::lancer` vient d'en noter
            // l'explication et l'heure de reprise, elles restent.
            if claude::refus(&e).is_some()
                && let Some(actuel) = parametre::lire(&ctx.pool, parametre::CLAUDE_ETAT).await?
            {
                for cle in ["refus", "reprise", "reprise_texte", "explication"] {
                    if let Some(x) = actuel.get(cle) {
                        v[cle] = x.clone();
                    }
                }
            }
            parametre::ecrire(&ctx.pool, parametre::CLAUDE_ETAT, &v).await?;
            Err(e.into())
        }
    }
}
