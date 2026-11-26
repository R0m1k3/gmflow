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
    assert!(p.contains(extraction::AVOIRS_POSITIFS));
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
            assert_eq!(donnees.remise_ht, Some(5.0));
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
    assert_eq!((ubl.remise_ht, ubl.annexes_ht), (Some(2.5), Some(1.2)));
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
        Aiguillage::Texte {
            voie,
            texte,
            tronque,
        } => {
            assert!(!tronque);
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
        Aiguillage::Texte {
            voie,
            texte,
            tronque,
        } => {
            assert_eq!(voie, Voie::Tableur);
            assert!(texte.contains("A12;Bougie parfumée;24"), "{texte}");
            assert!(!tronque);
        }
        a => panic!("aiguillage inattendu : {a:?}"),
    }
    // Tableur de plus de 400 lignes : coupé, et signalé.
    let lignes: Vec<Vec<String>> = (0..450)
        .map(|i| vec![format!("R{i}"), "Article".to_string(), "1".to_string()])
        .collect();
    let lignes: Vec<Vec<&str>> = lignes
        .iter()
        .map(|l| l.iter().map(String::as_str).collect())
        .collect();
    let lignes: Vec<&[&str]> = lignes.iter().map(Vec::as_slice).collect();
    match aiguiller("bl.xlsx", "", &fixtures::xlsx(&lignes), false) {
        Aiguillage::Texte { tronque, texte, .. } => {
            assert!(tronque);
            assert!(!texte.contains("R449"));
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
    // Date mal formée ou impossible : la lecture est gardée, la date vide et signalée.
    for d in ["12/11/2026", "2026-11-31"] {
        let mut faux = reponse_valide();
        faux["type"] = json!("facture");
        faux["date"] = json!(d);
        let e = extraction::depuis_reponse(&json!({ "structured_output": faux }), &s).unwrap();
        assert_eq!(e.date, None);
        assert!(
            extraction::anomalies(&e, None)
                .iter()
                .any(|a| a == &format!("la date lue « {d} » n'existe pas")),
            "{d}"
        );
    }
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

fn lecture_facture(ht: Option<f64>, tva: Option<f64>, ttc: Option<f64>) -> extraction::Extraction {
    serde_json::from_value(json!({
        "type": "facture", "societe_id": null,
        "fournisseur": { "nom": "Access Deco", "siret": null, "tva": null },
        "numero": "FA00066497", "date": "2026-11-12", "numero_commande": null,
        "montant_ht": ht, "montant_tva": tva, "montant_ttc": ttc,
        "confiance": { "type": 1, "societe": 1, "fournisseur": 1, "numero": 1, "date": 1, "montants": 1 }
    }))
    .unwrap()
}

#[test]
fn anomalies_de_lecture() {
    let texte = "FACTURE FA00066497\nTotal HT 1 279,50\nTVA 255,90\nNet à payer 1 535,40 €";
    let bonne = lecture_facture(Some(1279.5), Some(255.9), Some(1535.4));
    assert!(extraction::anomalies(&bonne, Some(texte)).is_empty());
    // Formats 1.535,40, 1,535.40 et 1535.4 (tableur) reconnus aussi.
    for t in [
        "HT 1.279,50 TVA 255,90 Total 1.535,40",
        "HT 1,279.50 TVA 255.90 Total 1,535.40",
        "ht;1279.5;tva;255.9;total;1535.4",
    ] {
        assert!(extraction::anomalies(&bonne, Some(t)).is_empty(), "{t}");
    }

    let sans_ttc = lecture_facture(Some(1279.5), Some(255.9), None);
    assert_eq!(
        extraction::anomalies(&sans_ttc, Some(texte)),
        vec!["le montant TTC manque".to_string()]
    );
    let incoherente = lecture_facture(Some(1279.5), Some(255.9), Some(1555.4));
    let a = extraction::anomalies(&incoherente, Some(texte));
    assert_eq!(a.len(), 2, "{a:?}");
    assert!(a[0].starts_with("HT + TVA"));
    assert!(a[1].contains("n'apparaît pas"));

    let mut bl = bonne.clone();
    bl.r#type = "bl".into();
    assert_eq!(extraction::anomalies(&bl, None).len(), 1);
    let mut autre = sans_ttc.clone();
    autre.r#type = "autre".into();
    assert!(extraction::anomalies(&autre, None).is_empty());
}

#[test]
fn texte_long_garde_le_debut_et_la_fin() {
    let texte = format!(
        "ENTETE {} TOTAL TTC 1 535,40",
        "ligne article\n".repeat(5_000)
    );
    let t = extraction::debut_et_fin(&texte, 3_000);
    assert!(t.starts_with("ENTETE"));
    assert!(t.ends_with("TOTAL TTC 1 535,40"));
    assert!(t.contains("caractères du milieu coupés"));
    assert!(t.chars().count() < 3_100);
    assert_eq!(extraction::debut_et_fin("court", 3_000), "court");
}

#[test]
fn remise_lue_a_part() {
    // Ancienne réponse sans les champs de remise : lue quand même.
    let sans = lecture_facture(Some(1161.29), Some(232.26), Some(1393.55));
    assert_eq!((sans.remise_ht, sans.remise_taux), (None, None));
    let mut v = serde_json::to_value(&sans).unwrap();
    v["remise_ht"] = json!(280.16);
    v["remise_taux"] = json!(20);
    let avec: extraction::Extraction = serde_json::from_value(v).unwrap();
    assert_eq!(
        (avec.remise_ht, avec.remise_taux),
        (Some(280.16), Some(20.0))
    );
    // Le schéma la demande toujours, et la consigne explique que le HT reste net.
    let schema = extraction::schema(false);
    let requis: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r.as_str())
        .collect();
    assert!(
        ["remise_ht", "remise_taux", "annexes_ht"]
            .iter()
            .all(|c| requis.contains(c))
    );
    assert!(extraction::consigne_nouvel_essai(&[], None, None).contains("remise_ht"));
    // Avoirs : montants demandés en positif, au premier essai comme au nouvel essai.
    assert!(
        extraction::consigne_nouvel_essai(&[], None, None).contains(extraction::AVOIRS_POSITIFS)
    );
}

const UBL_ENTETE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">
  <cbc:ID>F-77</cbc:ID>
  <cbc:IssueDate>2026-11-10</cbc:IssueDate>"#;

#[test]
fn ubl_gln_pas_pris_pour_un_siret() {
    // GLN en PartyIdentification, SIRET en PartyLegalEntity (imprimé après) : le SIRET gagne.
    let x = format!(
        r#"{UBL_ENTETE}
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyIdentification><cbc:ID schemeID="0088">3012345000017</cbc:ID></cac:PartyIdentification>
    <cac:PartyLegalEntity><cbc:RegistrationName>Durand</cbc:RegistrationName><cbc:CompanyID>55210055400013</cbc:CompanyID></cac:PartyLegalEntity>
  </cac:Party></cac:AccountingSupplierParty>
  <cac:AccountingCustomerParty><cac:Party>
    <cac:PartyIdentification><cbc:ID>3012345000024</cbc:ID></cac:PartyIdentification>
  </cac:Party></cac:AccountingCustomerParty>
</Invoice>"#
    );
    let d = xml::lire(x.as_bytes()).unwrap();
    assert_eq!(d.vendeur.siret.as_deref(), Some("55210055400013"));
    // GLN seul : pas un SIRET, rien n'est pris.
    assert_eq!(d.acheteur.siret, None);
    // SIRET en PartyIdentification, sans PartyLegalEntity : gardé.
    let x = format!(
        r#"{UBL_ENTETE}
  <cac:AccountingSupplierParty><cac:Party>
    <cac:PartyIdentification><cbc:ID>55210055400013</cbc:ID></cac:PartyIdentification>
  </cac:Party></cac:AccountingSupplierParty>
</Invoice>"#
    );
    let d = xml::lire(x.as_bytes()).unwrap();
    assert_eq!(d.vendeur.siret.as_deref(), Some("55210055400013"));
}

#[test]
fn xml_reference_designation_et_prix_de_base() {
    // CII : GTIN avant la référence du vendeur, prix pour 100 unités, devise.
    let cii = r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100">
  <rsm:ExchangedDocument><ram:ID>F-1</ram:ID><ram:TypeCode>380</ram:TypeCode></rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:SpecifiedTradeProduct><ram:GlobalID schemeID="0160">3123450000011</ram:GlobalID><ram:SellerAssignedID>REF-9</ram:SellerAssignedID><ram:Name>Vis</ram:Name></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeAgreement><ram:NetPriceProductTradePrice><ram:ChargeAmount>12.50</ram:ChargeAmount><ram:BasisQuantity unitCode="C62">100</ram:BasisQuantity></ram:NetPriceProductTradePrice></ram:SpecifiedLineTradeAgreement>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="C62">200</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:IncludedSupplyChainTradeLineItem>
      <ram:SpecifiedTradeProduct><ram:GlobalID>3123450000012</ram:GlobalID><ram:Name>Clou</ram:Name></ram:SpecifiedTradeProduct>
      <ram:SpecifiedLineTradeDelivery><ram:BilledQuantity unitCode="C62">1</ram:BilledQuantity></ram:SpecifiedLineTradeDelivery>
    </ram:IncludedSupplyChainTradeLineItem>
    <ram:ApplicableHeaderTradeSettlement><ram:InvoiceCurrencyCode>USD</ram:InvoiceCurrencyCode></ram:ApplicableHeaderTradeSettlement>
  </rsm:SupplyChainTradeTransaction>
</rsm:CrossIndustryInvoice>"#;
    let d = xml::lire(cii.as_bytes()).unwrap();
    let l = &d.lignes[0];
    assert_eq!(l.reference.as_deref(), Some("REF-9"));
    assert_eq!(l.gencod.as_deref(), Some("3123450000011"));
    assert_eq!(l.prix_unitaire_ht, Some(0.125));
    assert_eq!(d.devise.as_deref(), Some("USD"));
    // Clé GS1 fausse : pas un gencod, gardé comme référence.
    assert_eq!(d.lignes[1].gencod, None);
    assert_eq!(d.lignes[1].reference.as_deref(), Some("3123450000012"));

    // UBL : description longue imprimée avant le nom, prix pour 10.
    let ubl = format!(
        r#"{UBL_ENTETE}
  <cbc:DocumentCurrencyCode>EUR</cbc:DocumentCurrencyCode>
  <cac:InvoiceLine>
    <cbc:ID>1</cbc:ID>
    <cbc:InvoicedQuantity unitCode="C62">20</cbc:InvoicedQuantity>
    <cac:Item><cbc:Description>Bougie parfumée à la vanille, pot en verre de 200 g, mèche coton</cbc:Description><cbc:Name>Bougie vanille</cbc:Name>
      <cac:SellersItemIdentification><cbc:ID>BV-200</cbc:ID></cac:SellersItemIdentification>
      <cac:StandardItemIdentification><cbc:ID schemeID="0160">3123450000011</cbc:ID></cac:StandardItemIdentification></cac:Item>
    <cac:Price><cbc:PriceAmount currencyID="EUR">25.00</cbc:PriceAmount><cbc:BaseQuantity unitCode="C62">10</cbc:BaseQuantity></cac:Price>
  </cac:InvoiceLine>
</Invoice>"#
    );
    let d = xml::lire(ubl.as_bytes()).unwrap();
    let l = &d.lignes[0];
    assert_eq!(l.designation.as_deref(), Some("Bougie vanille"));
    assert_eq!(l.reference.as_deref(), Some("BV-200"));
    assert_eq!(l.gencod.as_deref(), Some("3123450000011"));
    assert_eq!(l.prix_unitaire_ht, Some(2.5));
    assert_eq!(d.devise.as_deref(), Some("EUR"));
}

#[test]
fn cle_des_gencods() {
    assert!(xml::gencod_valide("3123450000011"));
    assert!(xml::gencod_valide("3 123450 000011"));
    assert!(!xml::gencod_valide("3123450000012"));
    assert!(xml::gencod_valide("96385074")); // EAN-8
    assert!(xml::gencod_valide("03123450000011")); // GTIN-14
    assert!(!xml::gencod_valide("REF-12"));
}

#[test]
fn montants_cherches_parmi_les_nombres_du_texte() {
    let n = extraction::nombres("Qté 2 15,00 € ; total 1 535,40 ; 12 345 678,90 ; 1'250.00");
    for x in [2.0, 15.0, 1535.4, 535.4, 12_345_678.9, 1250.0] {
        assert!(n.iter().any(|v| (v - x).abs() < 0.001), "{x} dans {n:?}");
    }
    // « 2 15,00 » n'est pas « 215,00 » (15 n'est pas un groupe de milliers).
    assert!(!n.iter().any(|v| (v - 215.0).abs() < 0.001));

    // TTC inventé dont les chiffres se retrouvent collés dans le texte : signalé.
    let texte = "FACTURE FA00066497 du 12/11/2026\nRéf 15 354 Qté 0\nTotal HT 1 279,50\nTVA 255,90\nNet à payer 1 535,41";
    let e = lecture_facture(Some(1279.5), Some(255.9), Some(1535.4));
    let a = extraction::anomalies(&e, Some(texte));
    assert!(
        a.iter().any(|x| x.contains("TTC 1535.40 n'apparaît pas")),
        "{a:?}"
    );
    // HT inventé (TVA et TTC imprimés, HT + TVA = TTC faux) : signalé.
    let e = lecture_facture(Some(1279.0), Some(255.9), Some(1535.41));
    let a = extraction::anomalies(&e, Some(texte));
    assert!(a.iter().any(|x| x.contains("HT 1279.00")), "{a:?}");

    // Éco-participation ajoutée au HT et à la TVA (règle de lecture) : HT − annexes imprimé,
    // TVA déduite du HT et du TTC imprimés ; rien n'est signalé.
    let texte =
        "Total HT 1 677,74\nTVA 335,55\nEcomaison 12,47\nTVA Ecomaison 2,49\nTotal TTC 2 028,25";
    let mut e = lecture_facture(Some(1690.21), Some(338.04), Some(2028.25));
    e.annexes_ht = Some(12.47);
    assert!(extraction::anomalies(&e, Some(texte)).is_empty());
    // Plusieurs taux, total de TVA non imprimé : déduit, rien n'est signalé.
    let texte =
        "Base 5,5 % 100,00 TVA 5,50\nBase 20 % 100,00 TVA 20,00\nTotal HT 200,00\nTotal TTC 225,50";
    let e = lecture_facture(Some(200.0), Some(25.5), Some(225.5));
    assert!(extraction::anomalies(&e, Some(texte)).is_empty());
}

#[test]
fn lignes_coupees_signalees() {
    let s = societes();
    let ligne =
        |i: usize| json!({ "reference": format!("R{i}"), "designation": "Vase", "quantite": 1 });
    let mut r = reponse_valide();
    r["lignes"] = json!((0..10).map(ligne).collect::<Vec<_>>());
    let e = extraction::depuis_reponse(&json!({ "structured_output": r }), &s).unwrap();
    assert!(!e.lignes_tronquees);
    let mut r = reponse_valide();
    r["lignes"] = json!(
        (0..extraction::LIGNES_MAX + 5)
            .map(ligne)
            .collect::<Vec<_>>()
    );
    let e = extraction::depuis_reponse(&json!({ "structured_output": r }), &s).unwrap();
    assert!(e.lignes_tronquees);
    assert_eq!(e.lignes.len(), extraction::LIGNES_MAX);
}

#[test]
fn xml_de_plus_de_200_lignes_signale() {
    let refs: Vec<String> = (0..extraction::LIGNES_MAX + 3)
        .map(|i| format!("R{i}"))
        .collect();
    let lignes: Vec<fixtures::LigneTest<'_>> = refs
        .iter()
        .map(|r| (r.as_str(), "Vase", 1.0, 2.0))
        .collect();
    let x = fixtures::xml_cii_lignes("F-9", SIRET_A, "20261112", "C-1", None, &lignes);
    let d = xml::lire(x.as_bytes()).unwrap();
    assert!(d.lignes_tronquees);
    assert_eq!(d.lignes.len(), extraction::LIGNES_MAX);
    let x = fixtures::xml_cii_lignes("F-9", SIRET_A, "20261112", "C-1", None, &lignes[..5]);
    assert!(!xml::lire(x.as_bytes()).unwrap().lignes_tronquees);
}
