//! Mots de passe : Argon2id (paramètres par défaut de la crate).

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

use crate::erreur::ErreurApi;

pub const LONGUEUR_MIN: usize = 12;

pub fn valider(mdp: &str) -> Result<(), ErreurApi> {
    if mdp.chars().count() < LONGUEUR_MIN {
        return Err(ErreurApi::Donnees(format!(
            "le mot de passe doit faire au moins {LONGUEUR_MIN} caractères"
        )));
    }
    Ok(())
}

pub fn hacher(mdp: &str) -> Result<String, ErreurApi> {
    Argon2::default()
        .hash_password(mdp.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| ErreurApi::Interne(anyhow::anyhow!("argon2 : {e}")))
}

pub fn verifier(mdp: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(h) => Argon2::default().verify_password(mdp.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

/// Hash factice : la vérification prend le même temps quand l'identifiant n'existe pas.
pub fn hash_factice() -> &'static str {
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| hacher("mot-de-passe-factice-gmflow").expect("hash factice"))
}

/// Hachage hors du fil asynchrone (Argon2 coûte quelques dizaines de ms).
pub async fn hacher_async(mdp: String) -> Result<String, ErreurApi> {
    tokio::task::spawn_blocking(move || hacher(&mdp))
        .await
        .map_err(|e| ErreurApi::Interne(e.into()))?
}

pub async fn verifier_async(mdp: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verifier(&mdp, &hash)).await.unwrap_or(false)
}
