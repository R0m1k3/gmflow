use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use base64::Engine;

/// Configuration lue dans les variables d'environnement.
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub cle: [u8; 32],
    pub url_publique: String,
    pub fichiers: PathBuf,
    pub claude_bin: String,
}

impl Config {
    pub fn depuis_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL manquante")?;
        let cle = cle_depuis(&std::env::var("GMFLOW_CLE_CHIFFREMENT").unwrap_or_default())?;
        Ok(Self {
            database_url,
            cle,
            url_publique: std::env::var("GMFLOW_URL_PUBLIQUE")
                .unwrap_or_else(|_| "http://localhost:8347".into()),
            fichiers: std::env::var("GMFLOW_FICHIERS")
                .unwrap_or_else(|_| "/data/fichiers".into())
                .into(),
            claude_bin: std::env::var("GMFLOW_CLAUDE_BIN").unwrap_or_else(|_| "claude".into()),
        })
    }
}

/// Décode la clé de chiffrement (base64, 32 octets). Refuse toute autre valeur.
pub fn cle_depuis(b64: &str) -> Result<[u8; 32]> {
    if b64.trim().is_empty() {
        bail!(
            "GMFLOW_CLE_CHIFFREMENT manquante (32 octets en base64, ex. `openssl rand -base64 32`)"
        );
    }
    let octets = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .context("GMFLOW_CLE_CHIFFREMENT n'est pas du base64 valide")?;
    octets.try_into().map_err(|v: Vec<u8>| {
        anyhow::anyhow!(
            "GMFLOW_CLE_CHIFFREMENT doit faire 32 octets, pas {}",
            v.len()
        )
    })
}
