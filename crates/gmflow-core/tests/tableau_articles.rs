//! Lignes d'articles lues dans les cellules de vrais fichiers (xlsx, csv) générés ici.

use gmflow_core::lecture::grilles_tableur;
use gmflow_core::lecture::tableau_articles::{self as ta, Apprises};

fn classeur(feuilles: &[(&str, bool, &[&[&str]])]) -> Vec<u8> {
    // La première feuille (active) ne peut pas être masquée.
    let mut wb = rust_xlsxwriter::Workbook::new();
    for (nom, cachee, lignes) in feuilles {
        let ws = wb.add_worksheet();
        ws.set_name(*nom).unwrap();
        if *cachee {
            ws.set_hidden(true);
        }
        for (i, l) in lignes.iter().enumerate() {
            for (j, c) in l.iter().enumerate() {
                if let Some(f) = c.strip_prefix('=') {
                    ws.write_formula(
                        i as u32,
                        j as u16,
                        rust_xlsxwriter::Formula::new(f).set_result("#REF!"),
                    )
                    .unwrap();
                } else if let Ok(n) = c.parse::<f64>() {
                    ws.write_number(i as u32, j as u16, n).unwrap();
                } else {
                    ws.write_string(i as u32, j as u16, *c).unwrap();
                }
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

#[test]
fn feuille_cachee_ignoree_et_plusieurs_feuilles() {
    let x = classeur(&[
        (
            "BL",
            false,
            &[
                &["BL n° 4512"],
                &[
                    "Code article",
                    "Libellé",
                    "Gencod",
                    "Colis",
                    "PCB",
                    "PU HT",
                    "Remise",
                ],
                &["A1", "Bougie", "3760000000017", "2", "6", "1.5", "0.1"],
                &["A2", "Vase", "3760000000024", "1", "4", "3", "0.05"],
                &["", "Total", "", "3", "", "", ""],
            ],
        ),
        (
            "Calculs",
            true,
            &[
                &["Réf", "Désignation", "Qté"],
                &["CACHE", "Ligne cachée", "9"],
            ],
        ),
        (
            "CGV",
            false,
            &[&["Conditions générales de vente"], &["Paiement à 30 jours"]],
        ),
    ]);
    let g = grilles_tableur("bl.xlsx", "", &x).unwrap();
    assert_eq!(g.len(), 2, "feuille cachée ignorée");
    let lu = ta::lire(&g, &Apprises::new()).unwrap();
    assert_eq!(lu.lignes.len(), 2, "{:?}", lu.lignes);
    let l = &lu.lignes[0];
    assert_eq!(l.reference.as_deref(), Some("A1"));
    assert_eq!(l.gencod.as_deref(), Some("3760000000017"));
    assert_eq!((l.colis, l.pcb, l.quantite), (Some(2.0), Some(6.0), None));
    // 0,1 dans la cellule (format pourcentage) : 10 %.
    assert_eq!(l.remise_taux, Some(10.0));
    assert!(lu.extrait.contains("BL n° 4512"));
    assert!(lu.extrait.contains("Conditions générales"));
    assert!(!lu.extrait.contains("CACHE"));
}

#[test]
fn cellule_en_erreur_signalee() {
    let x = classeur(&[(
        "BL",
        false,
        &[
            &["Référence", "Désignation", "Quantité", "Prix unitaire HT"],
            &["A1", "Bougie", "2", "=Z99*2"],
            &["A2", "Vase", "1", "3"],
        ],
    )]);
    let g = grilles_tableur("bl.xlsx", "", &x).unwrap();
    let lu = ta::lire(&g, &Apprises::new()).unwrap();
    assert_eq!(lu.lignes[0].prix_unitaire_ht, None);
    assert_eq!(lu.anomalies.len(), 1, "{:?}", lu.anomalies);
    assert!(lu.anomalies[0].contains("#REF!"));
}

#[test]
fn csv_utf16_point_virgule_et_nombres_en_texte() {
    let texte = "Bon de livraison 88\r\nRéf;Désignation;Qté;PU HT;TVA\r\nB1;Coussin;3;1 234,50;20 %\r\nB2;Plaid;1;12,00;5,5%\r\n";
    let mut octets = vec![0xFF, 0xFE];
    for u in texte.encode_utf16() {
        octets.extend(u.to_le_bytes());
    }
    let g = grilles_tableur("bl.csv", "text/csv", &octets).unwrap();
    let lu = ta::lire(&g, &Apprises::new()).unwrap();
    assert_eq!(lu.lignes.len(), 2);
    assert_eq!(lu.lignes[0].prix_unitaire_ht, Some(1234.5));
    assert_eq!(lu.lignes[0].taux_tva, Some(20.0));
    assert_eq!(lu.lignes[1].taux_tva, Some(5.5));
    assert_eq!(lu.lignes[1].designation.as_deref(), Some("Plaid"));
}
