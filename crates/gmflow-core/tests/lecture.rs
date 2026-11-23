//! Aiguillage et lecture par le code (sans base ni Claude).

use gmflow_core::fixtures::{self, SIRET_A, SIRET_B};
use gmflow_core::lecture::extraction::{self, ErreurExtraction};
use gmflow_core::lecture::societe::{self, SocieteRef};
use gmflow_core::lecture::{Aiguillage, Voie, aiguiller, pdf, xml};
use serde_json::json;

fn societes() -> Vec<SocieteRef> {
    vec![
        SocieteRef {
            id: 1,
            raison_sociale: "Magasin Frouard".into(),
            autres_noms: vec![
                "Frouard Distribution".into(),
                "La Foir'Fouille Frouard".into(),
            ],
            siret: Some(SIRET_A.into()),
            tva: Some("FR58123456789".into()),
            adresses: vec![],
        },
        SocieteRef {
            id: 2,
            raison_sociale: "Magasin Houdemont".into(),
            autres_noms: vec!["La Foir'Fouille Houdemont".into()],
            siret: Some(SIRET_B.into()),
            tva: None,
            adresses: vec![],
        },
    ]
}

#[test]
fn acheteur_reconnu_par_un_autre_nom() {
    let acheteur = |nom: &str| xml::Partie {
        nom: Some(nom.into()),
        siret: None,
        tva: None,
    };
    let soc = societes();
    assert_eq!(
        societe::pour_acheteur(&acheteur("MAGASIN FROUARD"), &soc),
        Some(1)
    );
    assert_eq!(
        societe::pour_acheteur(&acheteur("Frouard distribution"), &soc),
        Some(1)
    );
    assert_eq!(
        societe::pour_acheteur(&acheteur("LA FOIR FOUILLE HOUDEMONT"), &soc),
        Some(2)
    );
    assert_eq!(
        societe::pour_acheteur(&acheteur("La Foir'Fouille"), &soc),
        None
    );
}

#[test]
fn autres_noms_dans_le_prompt() {
    let mail = extraction::ContexteMail {
        expediteur: "x@y.fr",
        objet: "Facture",
        corps: "",
        nom_fichier: "f.pdf",
    };
    let p = extraction::prompt(&mail, &societes(), &extraction::Contenu::Texte("Facture"));
    assert!(p.contains(
        "Magasin Frouard (aussi appelée : Frouard Distribution | La Foir'Fouille Frouard)"
    ));
}

#[test]
fn facturx_lu_sans_claude() {
    let pdf = fixtures::pdf_facturx(
        &fixtures::xml_cii("FA-2026-118", SIRET_A, false),
        &["Facture FA-2026-118", "Total TTC 120,00 EUR"],
    );
    assert!(pdf::xml_facturx(&pdf).is_some());
    match aiguiller("facture.pdf", "application/pdf", &pdf, false) {
        Aiguillage::Structure {
            voie,
            donnees,
            texte,
        } => {
            assert_eq!(voie, Voie::FacturX);
            assert_eq!(donnees.type_doc, "facture");
            assert_eq!(donnees.numero.as_deref(), Some("FA-2026-118"));
            assert_eq!(donnees.date.as_deref(), Some("2026-11-12"));
            assert_eq!(donnees.numero_commande.as_deref(), Some("CMD-77"));
            assert_eq!(
                donnees.vendeur.nom.as_deref(),
                Some("Grossiste Durand & Fils")
            );
            assert_eq!(donnees.vendeur.tva.as_deref(), Some("FR96552100554"));
            assert_eq!(donnees.montant_ht, Some(100.0));
            assert_eq!(donnees.montant_tva, Some(20.0));
            assert_eq!(donnees.montant_ttc, Some(120.0));
            assert_eq!(
                societe::pour_acheteur(&donnees.acheteur, &societes()),
                Some(1)
            );
            assert!(texte.unwrap_or_default().contains("FA-2026-118"));
        }
        a => panic!("aiguillage inattendu : {a:?}"),
    }
}

#[test]
fn xml_cii_et_ubl() {
    let cii = fixtures::xml_cii("AV-3", SIRET_B, true);
    match aiguiller("facture.xml", "application/xml", cii.as_bytes(), false) {
        Aiguillage::Structure { voie, donnees, .. } => {
            assert_eq!(voie, Voie::Xml);
            assert_eq!(donnees.type_doc, "avoir");
            assert_eq!(
                societe::pour_acheteur(&donnees.acheteur, &societes()),
                Some(2)
            );
        }
        a => panic!("aiguillage inattendu : {a:?}"),
    }
    let ubl = xml::lire(fixtures::xml_ubl_avoir("AV-9", "FR58123456789").as_bytes()).unwrap();
    assert_eq!(ubl.type_doc, "avoir");
    assert_eq!(ubl.numero.as_deref(), Some("AV-9"));
    assert_eq!(ubl.date.as_deref(), Some("2026-11-03"));
    assert_eq!(ubl.vendeur.nom.as_deref(), Some("Fournitures Martin"));
    assert_eq!(
        (ubl.montant_ht, ubl.montant_tva, ubl.montant_ttc),
        (Some(10.0), Some(2.0), Some(12.0))
    );
    assert_eq!(societe::pour_acheteur(&ubl.acheteur, &societes()), Some(1));
    // Un XML qui n'est pas une facture n'est pas lu.
    assert!(matches!(
        aiguiller(
            "flux.xml",
            "application/xml",
            b"<?xml version=\"1.0\"?><rss><channel/></rss>",
            false
        ),
        Aiguillage::Ignore(_)
    ));
}

#[test]
fn pdf_texte_et_scan() {
    let pdf = fixtures::pdf_texte(&[
        "FACTURE N 2026-501 du 10/11/2026",
        "Client : Magasin Frouard SIRET 123 456 789 00014",
        "Total HT 50,00  TVA 10,00  TTC 60,00",
    ]);
    match aiguiller("f.pdf", "application/pdf", &pdf, false) {
        Aiguillage::Texte { voie, texte } => {
            assert_eq!(voie, Voie::PdfTexte);
            assert!(texte.contains("2026-501"), "{texte}");
            assert_eq!(societe::dans_texte(&texte, &societes()), Some(1));
        }
        a => panic!("aiguillage inattendu : {a:?}"),
    }
    assert_eq!(
        aiguiller("scan.pdf", "application/pdf", &fixtures::pdf_scan(), false),
        Aiguillage::Scan {
            extension: "pdf",
            pages: 1
        }
    );
}

#[test]
fn tableurs() {
    let x = fixtures::xlsx(&[
        &["Référence", "Désignation", "Quantité"],
        &["A12", "Bougie parfumée", "24"],
    ]);
    match aiguiller("bl.xlsx", "application/octet-stream", &x, false) {
        Aiguillage::Texte { voie, texte } => {
            assert_eq!(voie, Voie::Tableur);
            assert!(texte.contains("A12;Bougie parfumée;24"), "{texte}");
        }
        a => panic!("aiguillage inattendu : {a:?}"),
    }
    // CSV en Windows-1252 (« é » = 0xE9).
    match aiguiller(
        "bl.csv",
        "text/csv",
        b"ref;libell\xe9;qte\nA1;Vase;3\n",
        false,
    ) {
        Aiguillage::Texte { texte, .. } => assert!(texte.contains("libellé"), "{texte}"),
        a => panic!("aiguillage inattendu : {a:?}"),
    }
}

#[test]
fn images_et_types_ignores() {
    assert!(matches!(
        aiguiller("image001.png", "image/png", &fixtures::png_logo(), false),
        Aiguillage::Ignore(_)
    ));
    let photo = fixtures::jpeg(120_000);
    assert!(matches!(
        aiguiller("IMG_2041.jpg", "image/jpeg", &photo, false),
        Aiguillage::Ignore(_)
    ));
    assert!(matches!(
        aiguiller("promo.jpg", "image/jpeg", &photo, true),
        Aiguillage::Ignore(_)
    ));
    assert!(matches!(
        aiguiller("invite.ics", "text/calendar", b"BEGIN:VCALENDAR", false),
        Aiguillage::Ignore(_)
    ));
    assert!(matches!(
        aiguiller("anim.gif", "image/gif", b"GIF89a....", false),
        Aiguillage::Ignore(_)
    ));
}

#[test]
fn societe_par_le_texte() {
    let s = societes();
    assert_eq!(
        societe::dans_texte("SIRET : 987 654 321 00027", &s),
        Some(2)
    );
    assert_eq!(societe::dans_texte("TVA FR 58 123 456 789", &s), Some(1));
    // Les deux sociétés citées : pas de choix par le code.
    assert_eq!(societe::dans_texte("123456789 et 987654321", &s), None);
    // SIREN noyé dans un nombre plus long : ignoré.
    assert_eq!(societe::dans_texte("Commande 41234567890", &s), None);
}

fn reponse_valide() -> serde_json::Value {
    json!({
        "type": "bl", "societe_id": 2,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": null },
        "numero": "BL-1", "date": "2026-11-12", "numero_commande": null,
        "montant_ht": null, "montant_tva": null, "montant_ttc": null,
        "confiance": { "type": 0.9, "societe": 0.8, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0 }
    })
}

#[test]
fn reponse_de_claude() {
    let s = societes();
    let e = extraction::depuis_reponse(
        &json!({ "is_error": false, "structured_output": reponse_valide() }),
        &s,
    )
    .unwrap();
    assert_eq!(e.r#type, "bl");
    assert_eq!(e.societe_id, Some(2));
    // JSON rendu en texte, dans une clôture Markdown.
    let texte = format!("```json\n{}\n```", reponse_valide());
    assert!(extraction::depuis_reponse(&json!({ "result": texte }), &s).is_ok());
    let mut faux = reponse_valide();
    faux["type"] = json!("bon_de_commande");
    assert!(matches!(
        extraction::depuis_reponse(&json!({ "structured_output": faux }), &s),
        Err(ErreurExtraction::Schema(_))
    ));
    let mut faux = reponse_valide();
    faux["societe_id"] = json!(99);
    assert!(matches!(
        extraction::depuis_reponse(&json!({ "structured_output": faux }), &s),
        Err(ErreurExtraction::Schema(_))
    ));
    let mut faux = reponse_valide();
    faux["date"] = json!("12/11/2026");
    assert!(extraction::depuis_reponse(&json!({ "structured_output": faux }), &s).is_err());
    assert_eq!(
        extraction::depuis_reponse(&json!({ "result": "Je ne peux pas lire ce fichier." }), &s),
        Err(ErreurExtraction::Absente)
    );
    // Le schéma demande la transcription pour un scan seulement.
    assert!(
        extraction::schema(true)["required"]
            .as_array()
            .unwrap()
            .contains(&json!("texte"))
    );
    assert!(
        !extraction::schema(false)["required"]
            .as_array()
            .unwrap()
            .contains(&json!("texte"))
    );
}

#[test]
fn liste_de_diffusion() {
    let avec = fixtures::eml("a@test", "Promo", &[], true);
    let sans = fixtures::eml("b@test", "Facture", &[], false);
    assert!(gmflow_core::eml::liste_diffusion(&avec));
    assert!(!gmflow_core::eml::liste_diffusion(&sans));
}
