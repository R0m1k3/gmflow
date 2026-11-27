//! Lot 4 sans base : lignes des factures électroniques, lignes de la réponse de Claude, et
//! règles du rapprochement (numéros, citations, écarts).

use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::lecture::extraction::{self, LIGNES_MAX, Ligne};
use gmflow_core::lecture::xml;
use gmflow_core::montant::{Decimal, dec};
use gmflow_core::rapprochement::{
    BlRef, FactureRef, Methode, cite, comparer, controle_bl, correspondance, ecart_montant,
    montant_bl, normaliser,
};
use serde_json::json;

fn ligne(
    reference: Option<&str>,
    designation: &str,
    qte: Decimal,
    montant: Option<Decimal>,
) -> Ligne {
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
    assert_eq!(d.montant_ht, Some(dec!(70.0)));
    assert_eq!(d.numero_commande.as_deref(), Some("CMD-77"));
    assert_eq!(d.numeros_bl, ["BL-501"]);
    assert_eq!(d.lignes.len(), 2);
    let l = &d.lignes[0];
    assert_eq!(l.reference.as_deref(), Some("REF-1"));
    assert_eq!(l.designation.as_deref(), Some("Seau 10 L"));
    assert_eq!(l.quantite, Some(dec!(20.0)));
    assert_eq!(l.unite.as_deref(), Some("C62"));
    assert_eq!(l.prix_unitaire_ht, Some(dec!(2.5)));
    assert_eq!(l.montant_ht, Some(dec!(50.0)));
    assert_eq!(d.lignes[1].montant_ht, Some(dec!(20.0)));
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
    assert_eq!(d.lignes[0].quantite, Some(dec!(2.5)));
    assert_eq!(d.lignes[0].unite.as_deref(), Some("KGM"));
    assert_eq!(d.lignes[0].montant_ht, Some(dec!(25.0)));
    assert_eq!(d.lignes[1].reference, None);
    assert_eq!(d.lignes[1].designation.as_deref(), Some("Pot en terre"));
    // L'ID de ligne ne remplace pas le numéro de la facture.
    assert_eq!(d.montant_ht, Some(dec!(35.0)));
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
        ligne(Some("REF-1"), "Seau", dec!(20.0), Some(dec!(50.0))),
        ligne(Some("REF-2"), "Balai", dec!(5.0), Some(dec!(20.0))),
        ligne(None, "Pelle", dec!(1.0), Some(dec!(3.0))),
    ];
    // Deux BL : REF-1 livré en deux fois, un balai manquant, une pelle (sans référence).
    let bls = [
        ligne(Some("ref 1"), "Seau 10 L", dec!(12.0), None),
        ligne(Some("REF-1"), "Seau 10 L", dec!(8.0), None),
        ligne(Some("REF-2"), "Balai", dec!(4.0), None),
        ligne(None, "pelle", dec!(1.0), None),
        ligne(Some("REF-3"), "Râteau", dec!(2.0), None),
    ];
    let e = comparer(&facture, &bls, None);
    assert_eq!(e.len(), 2, "{e:?}");
    assert_eq!(e[0].reference.as_deref(), Some("REF-2"));
    assert_eq!((e[0].qte_bl, e[0].qte_facture), (dec!(4.0), dec!(5.0)));
    assert_eq!(e[0].genre, "manquant");
    // Un balai manquant à 4,00 € (prix de la facture, le BL n'en a pas).
    assert_eq!(e[0].montant_ht, Some(dec!(4.0)));
    assert_eq!(e[1].reference.as_deref(), Some("REF-3"));
    assert_eq!((e[1].qte_bl, e[1].qte_facture), (dec!(2.0), dec!(0.0)));
    assert_eq!(e[1].genre, "en_trop");
    // Rien à comparer d'un côté : pas d'écart signalé.
    assert!(comparer(&facture, &[], None).is_empty());
}

#[test]
fn ecart_de_montant_et_controle_des_lignes_d_un_bl() {
    let l = [
        ligne(Some("A"), "a", dec!(1.0), Some(dec!(10.0))),
        ligne(Some("B"), "b", dec!(1.0), Some(dec!(5.5))),
    ];
    assert_eq!(montant_bl(&l, None), Some(dec!(15.5)));
    assert_eq!(montant_bl(&l, Some(dec!(16.0))), Some(dec!(16.0)));
    assert_eq!(montant_bl(&[ligne(None, "c", dec!(1.0), None)], None), None);
    assert_eq!(
        ecart_montant(Some(dec!(31.0)), &[Some(dec!(15.5)), Some(dec!(15.0))]),
        Some(dec!(0.5))
    );
    assert_eq!(
        ecart_montant(Some(dec!(31.0)), &[Some(dec!(15.5)), None]),
        None
    );
    assert_eq!(ecart_montant(Some(dec!(31.0)), &[]), None);
    assert!(controle_bl(&l, Some(dec!(15.5))).is_empty());
    assert!(controle_bl(&l, None).is_empty());
    let c = controle_bl(&l, Some(dec!(20.0)));
    assert_eq!(c.len(), 1);
    assert_eq!(c[0]["code"], "lignes");
}

#[test]
fn ecart_explique_par_remise_et_eco_participation() {
    use gmflow_core::rapprochement::ecart_explique;
    // Facture Tendance Bain : HT net 1 161,29 = 1 400,80 + 40,65 (Ecomaison) − 280,16 (remise 20 %),
    // BL valorisé 1 400,80 avant remise : écart −239,51.
    assert!(ecart_explique(
        -dec!(239.51),
        Some(dec!(280.16)),
        Some(dec!(40.65)),
        None
    ));
    // Remise seule, ou éco-participation seule.
    assert!(ecart_explique(
        -dec!(280.16),
        Some(dec!(280.16)),
        None,
        None
    ));
    assert!(ecart_explique(dec!(12.47), None, Some(dec!(12.47)), None));
    // BL déjà valorisé après remise : seule l'éco-participation reste.
    assert!(ecart_explique(
        dec!(40.65),
        Some(dec!(280.16)),
        Some(dec!(40.65)),
        None
    ));
    // Rien sur la facture, ou un écart qui reste : pas expliqué.
    assert!(!ecart_explique(-dec!(239.51), None, None, None));
    assert!(!ecart_explique(
        -dec!(200.0),
        Some(dec!(280.16)),
        Some(dec!(40.65)),
        None
    ));
}

#[test]
fn eco_participation_deja_dans_les_bl_pas_retiree_deux_fois() {
    use gmflow_core::rapprochement::{annexes_hors_bl, ecart_explique};
    // BL au HT calculé : 77,50 de produits + 2,50 d'éco = 80,00 ; facture : 77,50 − 2,33 de
    // remise (3 %) + 2,50 d'éco = 77,67. Écart −2,33 : la remise seule, l'éco s'annule.
    assert!(ecart_explique(
        -dec!(2.33),
        Some(dec!(2.33)),
        Some(dec!(2.5)),
        Some(dec!(2.5))
    ));
    // Sans tenir compte de l'éco des BL, l'éco était retirée une seconde fois.
    assert!(!ecart_explique(
        -dec!(2.33),
        Some(dec!(2.33)),
        Some(dec!(2.5)),
        None
    ));
    // Éco en partie seulement dans les BL (port en plus sur la facture).
    assert!(ecart_explique(
        dec!(5.67),
        Some(dec!(2.33)),
        Some(dec!(10.5)),
        Some(dec!(2.5))
    ));
    // Jamais négatif : éco des BL plus forte que les annexes lues sur la facture.
    assert_eq!(annexes_hors_bl(Some(dec!(2.5)), Some(dec!(4.0))), dec!(0.0));
    assert_eq!(
        annexes_hors_bl(Some(dec!(10.5)), Some(dec!(2.5))),
        dec!(8.0)
    );
    assert_eq!(annexes_hors_bl(None, None), dec!(0.0));
}

fn ligne_prix(reference: Option<&str>, designation: &str, qte: Decimal, prix: Decimal) -> Ligne {
    Ligne {
        reference: reference.map(str::to_string),
        designation: Some(designation.into()),
        quantite: Some(qte),
        prix_unitaire_ht: Some(prix),
        ..Default::default()
    }
}

#[test]
fn prix_change_entre_bl_et_facture() {
    let facture = [
        ligne_prix(Some("A1"), "Tapis de bain", dec!(10.0), dec!(12.5)),
        ligne_prix(Some("A2"), "Porte-savon", dec!(4.0), dec!(3.0)),
        // Référence fournisseur sur la facture, absente du BL : retrouvé par la désignation.
        ligne_prix(Some("FX-88"), "Rideau douche", dec!(2.0), dec!(9.0)),
    ];
    let bls = [
        ligne_prix(Some("A1"), "Tapis de bain", dec!(10.0), dec!(11.9)),
        ligne_prix(Some("A2"), "Porte-savon", dec!(4.0), dec!(3.0)),
        ligne_prix(None, "RIDEAU DOUCHE", dec!(2.0), dec!(9.0)),
    ];
    let e = comparer(&facture, &bls, None);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].genre, "prix");
    assert_eq!(
        (e[0].prix_facture, e[0].prix_bl),
        (Some(dec!(12.5)), Some(dec!(11.9)))
    );
    assert_eq!(e[0].montant_ht, Some(dec!(6.0)));
    // Prix du BL net d'une remise de 20 % que la facture applique en pied : pas un changement.
    let bls_nets = [
        ligne_prix(Some("A1"), "Tapis de bain", dec!(10.0), dec!(10.0)),
        ligne_prix(Some("A2"), "Porte-savon", dec!(4.0), dec!(2.4)),
        ligne_prix(None, "Rideau douche", dec!(2.0), dec!(7.2)),
    ];
    let f2 = [
        ligne_prix(Some("A1"), "Tapis de bain", dec!(10.0), dec!(12.5)),
        ligne_prix(Some("A2"), "Porte-savon", dec!(4.0), dec!(3.0)),
        ligne_prix(None, "Rideau douche", dec!(2.0), dec!(9.0)),
    ];
    assert!(comparer(&f2, &bls_nets, Some(dec!(20.0))).is_empty());
}

#[test]
fn articles_retrouves_par_gencod_et_colis() {
    // Facture : référence fournisseur et gencod, quantité en unités. BL : autre référence (code
    // du transporteur) mais même gencod, livré en colis sans quantité.
    let mut f1 = ligne(
        Some("AB-12"),
        "Bougie vanille",
        dec!(12.0),
        Some(dec!(24.0)),
    );
    f1.gencod = Some("3123450000011".into());
    let f2 = ligne(Some("CD-34"), "Vase", dec!(2.0), Some(dec!(10.0)));
    let mut b1 = ligne(Some("998877"), "BOUGIE VANILLE 200G", dec!(0.0), None);
    b1.quantite = None;
    b1.gencod = Some("03123450000011".into()); // GTIN-14 du même article
    b1.colis = Some(dec!(2.0));
    b1.pcb = Some(dec!(6.0));
    let b2 = ligne(Some("CD-34"), "Vase", dec!(2.0), None);
    assert!(comparer(&[f1.clone(), f2.clone()], &[b1.clone(), b2.clone()], None).is_empty());
    // Gencod à la clé fausse : ignoré, l'article est cherché par sa référence (absente du BL).
    b1.gencod = Some("03123450000012".into());
    let e = comparer(&[f1, f2], &[b1, b2], None);
    assert_eq!(e.len(), 2, "{e:?}");
}

/// Écarts pile au seuil de 0,05 € : en f64, 10,38 − 10,33 vaut 0,0500000000000007 et passait
/// au-dessus du seuil (fausse alerte « écart BL », somme des lignes d'un BL signalée à tort).
#[test]
fn ecarts_au_seuil_exacts() {
    use gmflow_core::rapprochement::ecart_explique;
    assert!(std::hint::black_box(10.38_f64) - 10.33 > 0.05);
    // Port de 10,33 sur la facture, écart de 10,38 avec les BL : expliqué à 5 centimes près.
    assert!(ecart_explique(dec!(10.38), None, Some(dec!(10.33)), None));
    assert!(!ecart_explique(dec!(10.39), None, Some(dec!(10.33)), None));
    // Somme des lignes d'un BL à 5 centimes du total : pas d'alerte.
    let l = [ligne(Some("A"), "Vase", dec!(1), Some(dec!(10.38)))];
    assert!(controle_bl(&l, Some(dec!(10.33))).is_empty());
    assert_eq!(controle_bl(&l, Some(dec!(10.32))).len(), 1);
    // 0,1 + 0,2 de BL contre une facture de 0,30 : écart nul, pas -0,00000000000000004.
    assert_eq!(
        ecart_montant(Some(dec!(0.3)), &[Some(dec!(0.1)), Some(dec!(0.2))]),
        Some(dec!(0))
    );
    // Avoir : trois BL négatifs de 33,35 contre −100,05.
    let bls = [Some(dec!(-33.35)); 3];
    assert_eq!(ecart_montant(Some(dec!(-100.05)), &bls), Some(dec!(0)));
}
