//! OCR local (pdftoppm + tesseract) sur de vrais scans fabriqués ici. Sautés si les outils
//! manquent.

use gmflow_core::fixtures::{self, PageTest};
use gmflow_core::lecture::extraction;
use gmflow_core::lecture::ocr::{self, Outils};
use gmflow_core::montant::dec;

async fn outils() -> Option<Outils> {
    let o = Outils::default();
    if ocr::disponible(&o).await {
        Some(o)
    } else {
        eprintln!("tesseract (fra) ou pdftoppm absent : test sauté");
        None
    }
}

const FACTURE: &[&str] = &[
    "FACTURE N 2026-501 du 10/11/2026",
    "Client : Magasin Frouard",
    "Bougies parfumees 24 unites",
    "Total HT 1 535,40",
    "TVA 20 % 307,08",
    "Total TTC 1 842,48",
];

#[tokio::test]
async fn scan_lu_par_l_ocr_avec_ses_montants() {
    let Some(o) = outils().await else { return };
    let page = fixtures::page_scannee(FACTURE).unwrap();
    let pdf = fixtures::pdf_pages(vec![page]);
    let t = ocr::completer(&o, &pdf).await.expect("texte OCR");
    assert_eq!(t.pages_ocr, vec![1]);
    assert!(!t.partiel);
    assert!(t.texte.contains("FACTURE"), "{}", t.texte);
    // Les montants sont retrouvés dans le texte OCR : le contrôle « dans le texte » peut se faire.
    let n = extraction::nombres(&t.texte);
    for m in [dec!(1535.40), dec!(307.08), dec!(1842.48)] {
        assert!(
            n.iter().any(|x| (x - m).abs() < dec!(0.005)),
            "{m} absent de {}",
            t.texte
        );
    }
}

#[tokio::test]
async fn pdf_mi_texte_mi_scan_seules_les_pages_sans_texte() {
    let Some(o) = outils().await else { return };
    let pdf = fixtures::pdf_pages(vec![
        PageTest::texte(&[
            "Lettre d'accompagnement de la societe Durand, Nancy",
            "Veuillez trouver ci-dessous notre facture du mois.",
        ]),
        fixtures::page_scannee(FACTURE).unwrap(),
    ]);
    let t = ocr::completer(&o, &pdf).await.expect("texte OCR");
    assert_eq!((t.pages, t.pages_ocr.clone()), (2, vec![2]));
    assert!(t.texte.contains("Lettre d'accompagnement"), "{}", t.texte);
    assert!(t.texte.contains("FACTURE"), "{}", t.texte);
    // PDF tout en texte : pas d'OCR.
    let texte = fixtures::pdf_pages(vec![PageTest::texte(FACTURE)]);
    assert_eq!(ocr::completer(&o, &texte).await, None);
}

#[tokio::test]
async fn ocr_sans_texte_exploitable_rien() {
    let Some(o) = outils().await else { return };
    // Image de bruit : pdftoppm la rend, tesseract n'y lit rien d'utile.
    assert_eq!(ocr::completer(&o, &fixtures::pdf_scan()).await, None);
}
