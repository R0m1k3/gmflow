//! Essai de connexion à l'API du serveur : GET sur l'adresse, jeton en `Authorization: Bearer`.
//! Réussi si la réponse est 2xx ; 401/403 = jeton refusé.

use std::time::Duration;

use gmflow_core::{parametre, serveur_api};
use serde_json::json;

use crate::Contexte;

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Appelle `adresse` avec le jeton ; renvoie le message affiché à l'admin.
pub async fn essayer(adresse: &str, jeton: Option<&str>) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("GMFlow")
        .build()
        .map_err(|e| format!("{e}"))?;
    let mut req = client.get(adresse);
    if let Some(j) = jeton {
        req = req.bearer_auth(j);
    }
    let r = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "le serveur ne répond pas (15 s)".to_string()
        } else if e.is_connect() {
            format!("connexion impossible : {}", source_la_plus_profonde(&e))
        } else {
            source_la_plus_profonde(&e)
        }
    })?;
    let s = r.status();
    match s.as_u16() {
        200..=299 => Ok(format!("connexion réussie (HTTP {})", s.as_u16())),
        401 | 403 => Err(format!("jeton refusé (HTTP {})", s.as_u16())),
        404 => Err("adresse introuvable sur le serveur (HTTP 404)".into()),
        c => Err(format!("le serveur a répondu HTTP {c}")),
    }
}

fn source_la_plus_profonde(e: &(dyn std::error::Error + 'static)) -> String {
    let mut s = e;
    while let Some(suivante) = s.source() {
        s = suivante;
    }
    s.to_string()
}

pub async fn tester(ctx: &Contexte) -> anyhow::Result<()> {
    let resultat = match serveur_api::lire(&ctx.pool, &ctx.chiffreur).await? {
        None => Err("adresse du serveur non configurée".to_string()),
        Some((s, jeton)) => essayer(&s.adresse, jeton.as_deref()).await,
    };
    let (ok, message) = match resultat {
        Ok(m) => (true, m),
        Err(m) => (false, m),
    };
    parametre::ecrire(
        &ctx.pool,
        serveur_api::CLE_TEST,
        &json!({ "quand": maintenant(), "ok": ok, "message": message }),
    )
    .await?;
    Ok(())
}
