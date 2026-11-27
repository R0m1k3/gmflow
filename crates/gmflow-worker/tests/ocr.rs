//! Scans lus avec l'OCR local (pdftoppm + tesseract) : texte du document, contrôle des montants,
//! PDF joint à Claude sans transcription. Sautés si les outils manquent.

mod commun;

use commun::{Banc, mail};
use gmflow_core::fixtures::{self, PageTest, SIRET_A};
use gmflow_core::lecture::ocr;
use gmflow_core::parametre;
use gmflow_worker::lecture;
use serde_json::{Value, json};

struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(reponse: &str) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-ocr").unwrap();
        std::fs::write(dossier.0.join("reponse.json"), reponse).unwrap();
        let d = dossier.0.display();
        let script = format!(
            "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; ls; echo ===; }} >> '{d}/journal'\n\
             cat '{d}/reponse.json'\n"
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn appels(&self) -> Vec<String> {
        std::fs::read_to_string(self.dossier.0.join("journal"))
            .unwrap_or_default()
            .split("===\n")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

fn facture() -> String {
    json!({ "type": "result", "is_error": false, "result": "", "structured_output": {
        "type": "facture", "societe_id": null,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": "FR96552100554" },
        "numero": "2026-501", "date": "2026-11-10", "numero_commande": null,
        "montant_ht": 1535.40, "montant_tva": 307.08, "montant_ttc": 1842.48,
        "confiance": { "type": 0.95, "societe": 0.5, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0.9 }
    }})
    .to_string()
}

const FACTURE: &[&str] = &[
    "FACTURE N 2026-501 du 10/11/2026",
    "Client : Magasin Frouard",
    "Bougies parfumees 24 unites",
    "Total HT 1 535,40",
    "TVA 20 % 307,08",
    "Total TTC 1 842,48",
];

async fn lire(faux: &FauxClaude, pdf: &[u8]) -> (Banc, i64) {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.dossier.0.join("claude").display().to_string();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_ocr")
        .await
        .unwrap();
    let a: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "ocr@test",
            "Facture",
            &[("scan.pdf", "application/pdf", pdf)],
            false,
        ),
    )
    .await;
    lecture::pompe(b.pool()).await.unwrap();
    let id: i64 = sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    lecture::lire(&b.ctx, id, false, false, false)
        .await
        .unwrap();
    (b, id)
}

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    etat: String,
    voie: Option<String>,
    message: Option<String>,
    texte: Option<String>,
    confiance: Value,
}

async fn doc(b: &Banc, id: i64) -> Doc {
    sqlx::query_as("select etat, voie, message, texte, confiance from document where id = $1")
        .bind(id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn scan_lu_avec_l_ocr_local_sans_transcription() {
    if !ocr::disponible(&ocr::Outils::default()).await {
        eprintln!("tesseract (fra) ou pdftoppm absent : test sauté");
        return;
    }
    let faux = FauxClaude::new(&facture());
    let pdf = fixtures::pdf_pages(vec![fixtures::page_scannee(FACTURE).unwrap()]);
    let (b, id) = lire(&faux, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!((d.etat.as_str(), d.voie.as_deref()), ("lu", Some("scan")));
    // Montants retrouvés dans le texte de l'OCR : aucune anomalie, un seul appel.
    assert_eq!(d.message, None);
    assert_eq!(d.confiance["texte_source"], "ocr_local");
    assert_eq!(d.confiance["pages_ocr"], json!([1]));
    assert!(d.texte.unwrap().contains("FACTURE"));
    let appels = faux.appels();
    assert_eq!(appels.len(), 1);
    let a = &appels[0];
    assert!(a.contains("reconnaissance de caractères"), "{a}");
    assert!(
        a.contains("Magasin Frouard"),
        "texte OCR dans le prompt : {a}"
    );
    assert!(a.contains("--tools\nRead\n"), "PDF joint : {a}");
    assert!(a.contains("document.pdf"), "{a}");
    assert!(!a.contains("transcription fidèle"), "{a}");
    assert!(
        !a.contains(r#""texte":{"type":"string"}"#),
        "schéma sans transcription : {a}"
    );
    b.fermer().await;
}

#[tokio::test]
async fn pdf_mi_texte_mi_scan_complete_par_l_ocr() {
    if !ocr::disponible(&ocr::Outils::default()).await {
        eprintln!("tesseract (fra) ou pdftoppm absent : test sauté");
        return;
    }
    let faux = FauxClaude::new(&facture());
    let pdf = fixtures::pdf_pages(vec![
        PageTest::texte(&[
            "Lettre d'accompagnement de la societe Durand, Nancy",
            "Veuillez trouver ci-joint notre facture du mois de novembre.",
        ]),
        fixtures::page_scannee(FACTURE).unwrap(),
    ]);
    let (b, id) = lire(&faux, &pdf).await;
    let d = doc(&b, id).await;
    // Voie texte (assez de texte en page 1), mais la page 2 a été lue par l'OCR.
    assert_eq!(
        (d.etat.as_str(), d.voie.as_deref()),
        ("lu", Some("pdf_texte"))
    );
    assert_eq!(d.confiance["pages_ocr"], json!([2]));
    assert_eq!(d.message, None);
    let t = d.texte.unwrap();
    assert!(
        t.contains("Lettre d'accompagnement") && t.contains("1 842,48"),
        "{t}"
    );
    b.fermer().await;
}
