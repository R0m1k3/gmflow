use gmflow_core::config::cle_depuis;
use gmflow_core::crypto::Chiffreur;

#[test]
fn aller_retour() {
    let c = Chiffreur::new(&[1u8; 32]);
    let chiffre = c.chiffrer(b"mot de passe IMAP");
    assert_eq!(c.dechiffrer(&chiffre).unwrap(), b"mot de passe IMAP");
}

#[test]
fn cle_fausse() {
    let chiffre = Chiffreur::new(&[1u8; 32]).chiffrer(b"secret");
    assert!(Chiffreur::new(&[2u8; 32]).dechiffrer(&chiffre).is_err());
}

#[test]
fn donnee_alteree() {
    let c = Chiffreur::new(&[1u8; 32]);
    let mut chiffre = c.chiffrer(b"secret");
    let dernier = chiffre.len() - 1;
    chiffre[dernier] ^= 1;
    assert!(c.dechiffrer(&chiffre).is_err());
    assert!(c.dechiffrer(&[0u8; 5]).is_err());
}

#[test]
fn deux_chiffrements_differents() {
    let c = Chiffreur::new(&[1u8; 32]);
    assert_ne!(c.chiffrer(b"meme texte"), c.chiffrer(b"meme texte"));
}

#[test]
fn cle_de_configuration() {
    assert!(cle_depuis("").is_err());
    assert!(cle_depuis("pas du base64 !").is_err());
    assert!(cle_depuis("AAAA").is_err()); // 3 octets
    assert_eq!(cle_depuis("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=").unwrap(), [1u8; 32]);
}
