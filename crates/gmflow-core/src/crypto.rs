//! AES-256-GCM. Format stocké : nonce (12 octets aléatoires) suivi du chiffré.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

#[derive(Debug, thiserror::Error)]
pub enum ErreurCrypto {
    #[error("donnée chiffrée trop courte")]
    TropCourt,
    #[error("déchiffrement impossible (clé fausse ou donnée altérée)")]
    Dechiffrement,
}

#[derive(Clone)]
pub struct Chiffreur(Aes256Gcm);

impl Chiffreur {
    pub fn new(cle: &[u8; 32]) -> Self {
        Self(Aes256Gcm::new(cle.into()))
    }

    pub fn chiffrer(&self, clair: &[u8]) -> Vec<u8> {
        let nonce_octets: [u8; 12] = rand::random();
        let nonce = Nonce::from(nonce_octets);
        let chiffre = self
            .0
            .encrypt(&nonce, clair)
            .expect("le chiffrement AES-GCM ne peut pas échouer sur une entrée en mémoire");
        let mut sortie = nonce_octets.to_vec();
        sortie.extend(chiffre);
        sortie
    }

    pub fn dechiffrer(&self, donnee: &[u8]) -> Result<Vec<u8>, ErreurCrypto> {
        if donnee.len() < 12 + 16 {
            return Err(ErreurCrypto::TropCourt);
        }
        let (nonce, chiffre) = donnee.split_at(12);
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| ErreurCrypto::TropCourt)?;
        self.0
            .decrypt(&Nonce::from(nonce), chiffre)
            .map_err(|_| ErreurCrypto::Dechiffrement)
    }

    pub fn dechiffrer_texte(&self, donnee: &[u8]) -> Result<String, ErreurCrypto> {
        String::from_utf8(self.dechiffrer(donnee)?).map_err(|_| ErreurCrypto::Dechiffrement)
    }
}
