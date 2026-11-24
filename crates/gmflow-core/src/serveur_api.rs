//! Connexion à une API du serveur de Michael : adresse (en clair) et jeton (chiffré).
//! L'appel part toujours du worker ; le jeton n'est jamais renvoyé par l'API de GMFlow.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::crypto::Chiffreur;
use crate::parametre;

/// `valeur` = [`ServeurApi`], `valeur_chiffree` = jeton.
pub const CLE: &str = "serveur_api";
/// Dernier essai : `{"quand", "ok", "message"}`.
pub const CLE_TEST: &str = "serveur_api_test";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServeurApi {
    pub adresse: String,
    /// 4 derniers caractères du jeton, pour le reconnaître.
    #[serde(default)]
    pub fin: Option<String>,
}

/// Adresse `http(s)://hôte[...]`, sans espace ni identifiants dans l'URL.
pub fn controler_adresse(adresse: &str) -> Result<(), String> {
    let reste = adresse
        .strip_prefix("https://")
        .or_else(|| adresse.strip_prefix("http://"))
        .ok_or("l'adresse doit commencer par https:// ou http://")?;
    let hote = reste.split(['/', '?', '#']).next().unwrap_or("");
    if hote.is_empty() || hote.contains('@') || adresse.chars().any(char::is_whitespace) {
        return Err("adresse invalide (exemple : https://api.mon-serveur.fr)".into());
    }
    Ok(())
}

pub fn fin(jeton: &str) -> String {
    let c: Vec<char> = jeton.chars().collect();
    c[c.len().saturating_sub(4)..].iter().collect()
}

/// Adresse et jeton déchiffré, si l'adresse est configurée.
pub async fn lire(
    pool: &PgPool,
    chiffreur: &Chiffreur,
) -> anyhow::Result<Option<(ServeurApi, Option<String>)>> {
    let Some(v) = parametre::lire(pool, CLE).await? else {
        return Ok(None);
    };
    let s: ServeurApi = serde_json::from_value(v)?;
    let jeton = match parametre::lire_secret(pool, CLE).await? {
        Some(c) => Some(chiffreur.dechiffrer_texte(&c)?),
        None => None,
    };
    Ok(Some((s, jeton)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresses() {
        assert!(controler_adresse("https://api.exemple.fr").is_ok());
        assert!(controler_adresse("http://192.168.1.10:8080/v1/").is_ok());
        assert!(controler_adresse("api.exemple.fr").is_err());
        assert!(controler_adresse("https://").is_err());
        assert!(controler_adresse("https://moi:mdp@exemple.fr").is_err());
        assert!(controler_adresse("https://a b.fr").is_err());
        assert_eq!(fin("abcdef"), "cdef");
        assert_eq!(fin("ab"), "ab");
    }
}
