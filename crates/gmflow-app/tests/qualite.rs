//! « Qualité de la lecture » : pièces validées du mois, corrections par champ, voie, modèle du
//! dernier essai et fournisseur, coût estimé des appels de lecture ; réservé à l'administrateur.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

/// Pièce lue et validée maintenant, avec ces corrections.
async fn piece(
    app: &Appli,
    boite: i64,
    societe: i64,
    fournisseur: i64,
    voie: &str,
    corrections: Value,
) -> i64 {
    let eml = gmflow_core::fixtures::eml(
        &format!("q{}@test", suffixe()),
        "Facture",
        &[("f.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    let pj = app.piece_de(mail).await;
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, montant_ttc, lu_le, verifie_le, corrections)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', $2, 'facture', $3, $4,
                'F', 60.00, now(), now(), $5
         from piece_jointe where id = $1 returning id",
    )
    .bind(pj)
    .bind(voie)
    .bind(societe)
    .bind(fournisseur)
    .bind(corrections)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

async fn appel(app: &Appli, document: Option<i64>, tache: &str, modele: &str, cout: Option<f64>) {
    sqlx::query(
        "insert into appel_claude (tache, modele, duree_ms, succes, document_id, tokens_entree,
                                   cout_usd)
         values ($1, $2, 1000, true, $3, 1200, $4::float8::numeric)",
    )
    .bind(tache)
    .bind(modele)
    .bind(document)
    .bind(cout)
    .execute(app.pool())
    .await
    .unwrap();
}

async fn fournisseur(app: &Appli, nom: &str) -> i64 {
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, statut) values ($1, lower($1), 'valide')
         returning id",
    )
    .bind(nom)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn qualite_et_cout_de_la_lecture_du_mois() {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    let ba = app.boite(sa, "Compta Frouard").await;
    let bb = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp", MDP, "responsable", Some(sa)).await;
    let admin = app.connecter("admin", MDP).await;
    let resp = app.connecter("resp", MDP).await;
    let durand = fournisseur(&app, "Durand").await;
    let martin = fournisseur(&app, "Martin").await;

    let ech = json!({ "date_echeance": { "avant": null, "apres": "2026-12-31" } });
    let ttc = json!({ "montant_ttc": { "avant": 6.0, "apres": 60.0 },
                      "date_echeance": { "avant": null, "apres": "2026-12-31" } });
    // Durand : 2 pièces sur 2 corrigées ; Martin : 1 sur 2 ; une pièce de Houdemont.
    let d1 = piece(&app, ba, sa, durand, "pdf_texte", ech.clone()).await;
    let d2 = piece(&app, ba, sa, durand, "scan", ttc).await;
    let d3 = piece(&app, ba, sa, martin, "pdf_texte", json!({})).await;
    let d4 = piece(&app, ba, sa, martin, "facturx", ech).await;
    let d5 = piece(&app, bb, sb, martin, "pdf_texte", json!({})).await;
    // Pas encore validée : ne compte pas.
    sqlx::query("update document set verifie_le = null where id = $1")
        .bind(piece(&app, ba, sa, durand, "pdf_texte", json!({})).await)
        .execute(app.pool())
        .await
        .unwrap();
    appel(&app, Some(d1), "extraction_texte", "haiku", Some(0.01)).await;
    appel(&app, Some(d2), "ocr_scan", "haiku", Some(0.02)).await;
    appel(&app, Some(d2), "nouvel_essai", "sonnet", Some(0.05)).await;
    appel(&app, Some(d3), "extraction_texte", "haiku", None).await;
    appel(&app, Some(d5), "extraction_texte", "haiku", Some(0.04)).await;
    // Tri des mails : pas de la lecture.
    appel(&app, None, "tri_mail", "haiku", Some(1.0)).await;
    let _ = d4;

    let r = app
        .requete("GET", "/api/admin/qualite", None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    let q = &r.json;
    assert_eq!(q["validees"], 5);
    assert_eq!(q["corrigees"], 3);
    assert_eq!(
        q["par_champ"],
        json!([{ "champ": "date_echeance", "pieces": 3 }, { "champ": "montant_ttc", "pieces": 1 }])
    );
    let voie = |cle: &str| {
        q["par_voie"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["cle"] == cle)
            .cloned()
            .unwrap()
    };
    assert_eq!(voie("pdf_texte")["validees"], 3);
    assert_eq!(voie("pdf_texte")["corrigees"], 1);
    assert_eq!(voie("scan")["corrigees"], 1);
    let modele = |m: Value, reprise: bool| {
        q["par_modele"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["modele"] == m && v["nouvel_essai"] == reprise)
            .cloned()
            .unwrap_or_else(|| panic!("{m} {reprise} : {}", q["par_modele"]))
    };
    assert_eq!(modele(json!("haiku"), false)["validees"], 3);
    assert_eq!(modele(json!("sonnet"), true)["corrigees"], 1);
    // Factur-X : sans appel mesuré.
    assert_eq!(modele(Value::Null, false)["validees"], 1);
    // Durand (2/2) avant Martin (1/3).
    assert_eq!(q["fournisseurs"][0]["nom"], "Durand");
    assert_eq!(q["fournisseurs"][1]["nom"], "Martin");
    assert_eq!(q["fournisseurs"][1]["validees"], 3);
    let c = &q["cout"];
    assert_eq!(c["appels"], 5);
    assert_eq!(c["appels_sans_cout"], 1);
    assert_eq!(c["pieces"], 3);
    assert!(
        (c["total_usd"].as_f64().unwrap() - 0.12).abs() < 1e-9,
        "{c}"
    );
    assert!(
        (c["moyen_par_piece_usd"].as_f64().unwrap() - 0.04).abs() < 1e-9,
        "{c}"
    );

    // Une société : ses pièces et les appels de ses pièces seulement.
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/qualite?societe={sb}"),
            None,
            Some(&admin),
        )
        .await;
    assert_eq!(r.json["validees"], 1);
    assert_eq!(r.json["corrigees"], 0);
    assert!((r.json["cout"]["total_usd"].as_f64().unwrap() - 0.04).abs() < 1e-9);

    // Un autre mois : rien.
    let r = app
        .requete("GET", "/api/admin/qualite?mois=2020-01", None, Some(&admin))
        .await;
    assert_eq!(r.json["validees"], 0);
    assert_eq!(r.json["cout"]["moyen_par_piece_usd"], Value::Null);
    let r = app
        .requete("GET", "/api/admin/qualite?mois=2020-13", None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);

    // Responsable : refusé.
    let r = app
        .requete("GET", "/api/admin/qualite", None, Some(&resp))
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    app.fermer().await;
}
