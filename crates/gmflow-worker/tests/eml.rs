use gmflow_core::eml::{decouper, extraire_piece, sha256};

fn fixture(nom: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{nom}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn simple() {
    let d = decouper(&fixture("simple.eml")).unwrap();
    assert_eq!(d.message_id, "simple-1@fournisseur.test");
    assert_eq!(d.expediteur, "contact@fournisseur.test");
    assert_eq!(d.destinataires, ["compta@frouard.test"]);
    assert_eq!(d.objet, "Bonjour");
    assert!(d.date.is_some());
    assert!(d.corps_texte.contains("sans pièce jointe"));
    assert!(d.pieces.is_empty());
}

#[test]
fn pdf_et_objet_encode() {
    let d = decouper(&fixture("pdf.eml")).unwrap();
    assert_eq!(d.objet, "Facture n°2026-001");
    assert_eq!(d.destinataires, ["compta@frouard.test", "michael@frouard.test"]);
    assert_eq!(d.pieces.len(), 1);
    assert_eq!(d.pieces[0].nom, "F2026-001.pdf");
    assert_eq!(d.pieces[0].type_mime, "application/pdf");
}

#[test]
fn nom_rfc2231() {
    let d = decouper(&fixture("rfc2231.eml")).unwrap();
    assert_eq!(d.pieces.len(), 1);
    assert_eq!(d.pieces[0].nom, "facture n°12 été.pdf");
}

#[test]
fn inline_et_extraction() {
    let brut = fixture("inline.eml");
    let d = decouper(&brut).unwrap();
    assert_eq!(d.pieces.len(), 2, "{:?}", d.pieces);
    for p in &d.pieces {
        let octets = extraire_piece(&brut, p.rang).unwrap();
        assert_eq!(sha256(&octets), p.sha256);
        assert_eq!(octets.len() as i64, p.taille);
    }
    assert!(d.pieces.iter().any(|p| p.nom == "logo.png"));
    assert!(d.pieces.iter().any(|p| p.nom == "F-inline.pdf"));
    assert!(extraire_piece(&brut, 2).is_err());
}

#[test]
fn html_seul() {
    let d = decouper(&fixture("html-seul.eml")).unwrap();
    assert!(d.corps_texte.contains("catalogue"), "{}", d.corps_texte);
    assert!(!d.corps_texte.contains('<'), "{}", d.corps_texte);
}

#[test]
fn sans_message_id() {
    let d = decouper(&fixture("sans-message-id.eml")).unwrap();
    assert!(d.message_id.starts_with("sha256:"));
}

#[test]
fn empreintes() {
    let a = decouper(&fixture("pdf.eml")).unwrap();
    let b = decouper(&fixture("meme-mail-boite2.eml")).unwrap();
    assert_eq!(a.empreinte_contenu, b.empreinte_contenu);
    assert_eq!(a.message_id, b.message_id);
    let x = decouper(&fixture("meme-id-a.eml")).unwrap();
    let y = decouper(&fixture("meme-id-b.eml")).unwrap();
    assert_eq!(x.message_id, y.message_id);
    assert_ne!(x.empreinte_contenu, y.empreinte_contenu);
}
