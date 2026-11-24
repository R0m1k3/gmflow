//! Lot 4 sans base : lignes des factures électroniques, lignes de la réponse de Claude, et
//! règles du rapprochement (numéros, citations, écarts).

use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::lecture::extraction::{self, LIGNES_MAX, Ligne};
use gmflow_core::lecture::xml;
use gmflow_core::rapprochement::{
    BlRef, FactureRef, Methode, cite, comparer, controle_bl, correspondance, ecart_montant,
    montant_bl, normaliser,
};
use serde_json::json;

fn ligne(reference: Option<&str>, designation: &str, qte: f64, montant: Option<f64>) -> Ligne {
    Ligne {
        reference: reference.map(str::to_string),
        designation: Some(designation.into()),
        quantite: Some(qte),
        montant_ht: montant,
        ..Default::default()
    }
}

#[test]
fn lignes_et_bl_d_une_facture_cii() {
    let x = fixtures::xml_cii_lignes(
        "FA-1",
        SIRET_A,
        "20261112",
        "CMD-77",
        Some("BL-501"),
        &[
            ("REF-1", "Seau 10 L", 20.0, 2.5),
            ("REF-2", "Balai", 5.0, 4.0),
        ],
    );
    let d = xml::lire(x.as_bytes()).unwrap();
    assert_eq!(d.numero.as_deref(), Some("FA-1"));
    assert_eq!(d.montant_ht, Some(70.0));
    assert_eq!(d.numero_commande.as_deref(), Some("CMD-77"));
    assert_eq!(d.numeros_bl, ["BL-501"]);
    assert_eq!(d.lignes.len(), 2);
    let l = &d.lignes[0];
    assert_eq!(l.reference.as_deref(), Some("REF-1"));
    assert_eq!(l.designation.as_deref(), Some("Seau 10 L"));
    assert_eq!(l.quantite, Some(20.0));
    assert_eq!(l.unite.as_deref(), Some("C62"));
    assert_eq!(l.prix_unitaire_ht, Some(2.5));
    assert_eq!(l.montant_ht, Some(50.0));
    assert_eq!(d.lignes[1].montant_ht, Some(20.0));
    // Une facture sans lignes reste lue comme avant.
    let sans = xml::lire(fixtures::xml_cii("FA-2", SIRET_A, false).as_bytes()).unwrap();
    assert!(sans.lignes.is_empty() && sans.numeros_bl.is_empty());
}

#[test]
fn lignes_et_bl_d_une_facture_ubl() {
    let d = xml::lire(fixtures::xml_ubl_facture_lignes("U-7").as_bytes()).unwrap();
    assert_eq!(d.type_doc, "facture");
    assert_eq!(d.numero.as_deref(), Some("U-7"));
    assert_eq!(d.numero_commande.as_deref(), Some("PO-9"));
    assert_eq!(d.numeros_bl, ["BL-900", "BL-901"]);
    assert_eq!(d.lignes.len(), 2);
    assert_eq!(d.lignes[0].reference.as_deref(), Some("T-01"));
    assert_eq!(d.lignes[0].quantite, Some(2.5));
    assert_eq!(d.lignes[0].unite.as_deref(), Some("KGM"));
    assert_eq!(d.lignes[0].montant_ht, Some(25.0));
    assert_eq!(d.lignes[1].reference, None);
    assert_eq!(d.lignes[1].designation.as_deref(), Some("Pot en terre"));
    // L'ID de ligne ne remplace pas le numéro de la facture.
    assert_eq!(d.montant_ht, Some(35.0));
}

#[test]
fn lignes_de_la_reponse_de_claude_bornees() {
    let mut lignes: Vec<_> = (0..LIGNES_MAX + 30)
        .map(|i| {
            json!({ "reference": format!("R{i}"), "designation": "x", "quantite": 1,
                         "unite": null, "prix_unitaire_ht": null, "montant_ht": null })
        })
        .collect();
    lignes.insert(
        0,
        json!({ "reference": " ", "designation": null, "quantite": null,
                             "unite": null, "prix_unitaire_ht": null, "montant_ht": null }),
    );
    let rep = json!({ "structured_output": {
        "type": "bl", "societe_id": null,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": null },
        "numero": "BL-1", "date": null, "numero_commande": null,
        "montant_ht": null, "montant_tva": null, "montant_ttc": null,
        "lignes": lignes, "numeros_bl": ["BL-9", " BL-9 ", ""],
        "confiance": { "type": 1, "societe": 0, "fournisseur": 1, "numero": 1, "date": 0, "montants": 0 }
    }});
    let e = extraction::depuis_reponse(&rep, &[]).unwrap();
    assert_eq!(e.lignes.len(), LIGNES_MAX);
    assert_eq!(e.lignes[0].reference.as_deref(), Some("R0"));
    assert_eq!(e.numeros_bl, ["BL-9"]);
    // Le schéma exige les deux champs.
    let s = extraction::schema(false);
    let requis = s["required"].as_array().unwrap();
    assert!(requis.contains(&json!("lignes")) && requis.contains(&json!("numeros_bl")));
}

#[test]
fn numeros_normalises_et_citations() {
    assert_eq!(normaliser(" cmd-077 / a "), "CMD077A");
    assert!(cite("Facture n° FA-2026-118 du 12/11", "FA 2026/118"));
    assert!(cite("Réf. facture : fa.2026.118.", "FA-2026-118"));
    // Le numéro doit être un mot entier.
    assert!(!cite("FA-2026-1189", "FA-2026-118"));
    assert!(!cite("XFA2026118", "FA-2026-118"));
    // Numéro trop court : ignoré.
    assert!(!cite("BL 12", "12"));
}

#[test]
fn methodes_par_ordre_de_surete() {
    let f = FactureRef {
        numero: Some("FA-31".into()),
        numero_commande: Some("CMD 77".into()),
        numeros_bl: vec!["bl-501".into()],
    };
    let bl = |numero: &str, commande: Option<&str>, texte: Option<&str>| BlRef {
        numero: Some(numero.into()),
        numero_commande: commande.map(str::to_string),
        texte: texte.map(str::to_string),
    };
    assert_eq!(
        correspondance(&f, &bl("BL 501", Some("CMD-77"), None)),
        Some(Methode::NumeroBl)
    );
    assert_eq!(
        correspondance(
            &f,
            &bl("BL-600", None, Some("Livraison pour facture FA-31"))
        ),
        Some(Methode::FactureCitee)
    );
    assert_eq!(
        correspondance(&f, &bl("BL-601", Some("cmd77"), None)),
        Some(Methode::Commande)
    );
    assert_eq!(
        correspondance(&f, &bl("BL-602", Some("CMD-78"), None)),
        None
    );
    assert!(Methode::NumeroBl < Methode::FactureCitee && Methode::FactureCitee < Methode::Commande);
}

#[test]
fn ecarts_de_quantites_par_article() {
    let facture = [
        ligne(Some("REF-1"), "Seau", 20.0, Some(50.0)),
        ligne(Some("REF-2"), "Balai", 5.0, Some(20.0)),
        ligne(None, "Pelle", 1.0, Some(3.0)),
    ];
    // Deux BL : REF-1 livré en deux fois, un balai manquant, une pelle (sans référence).
    let bls = [
        ligne(Some("ref 1"), "Seau 10 L", 12.0, None),
        ligne(Some("REF-1"), "Seau 10 L", 8.0, None),
        ligne(Some("REF-2"), "Balai", 4.0, None),
        ligne(None, "pelle", 1.0, None),
        ligne(Some("REF-3"), "Râteau", 2.0, None),
    ];
    let e = comparer(&facture, &bls);
    assert_eq!(e.len(), 2, "{e:?}");
    assert_eq!(e[0].reference.as_deref(), Some("REF-2"));
    assert_eq!((e[0].qte_bl, e[0].qte_facture), (4.0, 5.0));
    assert_eq!(e[1].reference.as_deref(), Some("REF-3"));
    assert_eq!((e[1].qte_bl, e[1].qte_facture), (2.0, 0.0));
    // Rien à comparer d'un côté : pas d'écart signalé.
    assert!(comparer(&facture, &[]).is_empty());
}

#[test]
fn ecart_de_montant_et_controle_des_lignes_d_un_bl() {
    let l = [
        ligne(Some("A"), "a", 1.0, Some(10.0)),
        ligne(Some("B"), "b", 1.0, Some(5.5)),
    ];
    assert_eq!(montant_bl(&l, None), Some(15.5));
    assert_eq!(montant_bl(&l, Some(16.0)), Some(16.0));
    assert_eq!(montant_bl(&[ligne(None, "c", 1.0, None)], None), None);
    assert_eq!(
        ecart_montant(Some(31.0), &[Some(15.5), Some(15.0)]),
        Some(0.5)
    );
    assert_eq!(ecart_montant(Some(31.0), &[Some(15.5), None]), None);
    assert_eq!(ecart_montant(Some(31.0), &[]), None);
    assert!(controle_bl(&l, Some(15.5)).is_empty());
    assert!(controle_bl(&l, None).is_empty());
    let c = controle_bl(&l, Some(20.0));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["code"], "lignes");
}

#[test]
fn ecart_explique_par_remise_et_eco_participation() {
    use gmflow_core::rapprochement::ecart_explique;
    // Facture Tendance Bain : HT net 1 161,29 = 1 400,80 + 40,65 (Ecomaison) − 280,16 (remise 20 %),
    // BL valorisé 1 400,80 avant remise : écart −239,51.
    assert!(ecart_explique(-239.51, Some(280.16), Some(40.65)));
    // Remise seule, ou éco-participation seule.
    assert!(ecart_explique(-280.16, Some(280.16), None));
    assert!(ecart_explique(12.47, None, Some(12.47)));
    // BL déjà valorisé après remise : seule l'éco-participation reste.
    assert!(ecart_explique(40.65, Some(280.16), Some(40.65)));
    // Rien sur la facture, ou un écart qui reste : pas expliqué.
    assert!(!ecart_explique(-239.51, None, None));
    assert!(!ecart_explique(-200.0, Some(280.16), Some(40.65)));
}
