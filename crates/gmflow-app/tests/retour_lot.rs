//! Retour en arrière et validation par lot côté API (audit U1, U2, U23) : « Annuler » après
//! « Valider », validation de plusieurs pièces sans alerte, motif d'écartement et « Remettre à
//! contrôler ». Droits vérifiés pour chaque pièce.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
    c_resp_b: String,
    sa: i64,
    boite_a: i64,
    boite_b: i64,
    sb: i64,
    fournisseur: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    sqlx::query(
        "update societe set siret = '12345678900014', adresse_depot = 'depot@dext.test', outil_depot = 'dext'",
    )
    .execute(app.pool())
    .await
    .unwrap();
    let boite_a = app.boite(sa, "Compta Frouard").await;
    let boite_b = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    app.utilisateur("resp-b", MDP, "responsable", Some(sb))
        .await;
    let fournisseur: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut)
         values ('Grossiste Durand', 'grossiste durand', '55210055400013', 'valide') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::expedition::CLE,
        &json!({ "serveur": "smtp.test", "port": 465, "securite": "tls", "adresse": "gmflow@test.local" }),
    )
    .await
    .unwrap();
    Monde {
        c_admin: app.connecter("admin", MDP).await,
        c_resp_a: app.connecter("resp-a", MDP).await,
        c_resp_b: app.connecter("resp-b", MDP).await,
        app,
        sa,
        sb,
        boite_a,
        boite_b,
        fournisseur,
    }
}

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

/// Facture lue et évaluée ; `fiche` : rattachée à la fiche fournisseur valide (prête), sinon
/// sans fiche (alerte « fournisseur »).
async fn facture(m: &Monde, boite: i64, societe: i64, numero: &str, fiche: bool) -> i64 {
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}-{}@test", suffixe()),
        "Facture",
        &[("f.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    let piece = app.piece_de(mail).await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, fournisseur_id, fournisseur_siret, numero, date_document, montant_ht,
             montant_tva, montant_ttc, confiance, texte, lu_le)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', 'facture', $2,
                'Durand', $3, '55210055400013', $4, current_date, 50.00, 10.00, 60.00,
                '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(societe)
    .bind(fiche.then_some(m.fournisseur))
    .bind(numero)
    .fetch_one(app.pool())
    .await
    .unwrap();
    gmflow_core::depot::evaluer(app.pool(), id).await.unwrap();
    id
}

async fn doc(app: &Appli, id: i64) -> Value {
    sqlx::query_scalar(
        "select jsonb_build_object('numero', numero, 'statut_depot', statut_depot,
                'valide', valide_le is not null, 'verifie', verifie_le is not null,
                'etat', etat, 'controles', controles)
         from document where id = $1",
    )
    .bind(id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn valider_puis_annuler_remet_l_etat_d_avant() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.c_resp_a.as_str());
    let f = facture(&m, m.boite_a, m.sa, "F-1", false).await;
    let avant = doc(app, f).await;
    assert_eq!(avant["statut_depot"], "a_verifier");

    // Valider avec une correction : envoi prévu, pas parti, annulable pendant le délai.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f}/valider"),
            Some(json!({ "corrections": {
                "type": "facture", "societe_id": m.sa, "fournisseur_id": null, "numero": "F-1 bis",
                "date_document": null, "numero_commande": null,
                "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0 } })),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["envoi"], true);
    assert_eq!(
        r.json["annulation"]["delai_s"],
        gmflow_core::envoi_prevu::DELAI_ANNULATION_S
    );
    let lot = r.json["annulation"]["lot"].as_str().unwrap().to_string();
    assert_eq!(doc(app, f).await["valide"], true);
    assert_eq!(
        app.compter("select count(*) from envoi_prevu where etat = 'attente'")
            .await,
        1
    );
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'envoi' and convert_from(job, 'UTF8') like '%DeposerPrevu%'").await,
        1
    );

    // Un responsable d'une autre société n'annule rien.
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            Some(&m.c_resp_b),
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    assert_eq!(doc(app, f).await["valide"], true);

    // « Annuler » : exactement l'état d'avant, correction comprise ; journalisé.
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["annulees"], 1);
    assert_eq!(r.json["pieces"][0]["issue"], "annulee");
    assert_eq!(doc(app, f).await, avant);
    assert_eq!(
        app.compter("select count(*) from journal where action = 'validation_annulee'")
            .await,
        1
    );
    // Seconde fois : plus rien à annuler.
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            resp,
        )
        .await;
    assert_eq!(r.json["annulees"], 0);

    // Validée de nouveau, puis prise par le worker : trop tard.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f}/valider"),
            Some(json!({})),
            resp,
        )
        .await;
    let lot2 = r.json["annulation"]["lot"].as_str().unwrap().to_string();
    sqlx::query("update envoi_prevu set etat = 'pris' where lot = $1::uuid")
        .bind(&lot2)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot2 })),
            resp,
        )
        .await;
    assert_eq!(r.json["annulees"], 0);
    assert_eq!(r.json["deja_parties"], 1);
    assert_eq!(doc(app, f).await["valide"], true);
    m.app.fermer().await;
}

#[tokio::test]
async fn validation_par_lot_sans_alerte_seulement() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.c_resp_a.as_str());
    let pret1 = facture(&m, m.boite_a, m.sa, "L-1", true).await;
    let pret2 = facture(&m, m.boite_a, m.sa, "L-2", true).await;
    let alerte = facture(&m, m.boite_a, m.sa, "L-3", false).await;
    let ailleurs = facture(&m, m.boite_b, m.sb, "L-4", true).await;
    assert_eq!(doc(app, pret1).await["statut_depot"], "pret");
    // Écart d'article avec un BL : alerte de la fiche, hors lot aussi.
    let ecart = facture(&m, m.boite_a, m.sa, "L-5", true).await;
    let bl = facture(&m, m.boite_a, m.sa, "BL-5", true).await;
    sqlx::query("update document set type = 'bl' where id = $1")
        .bind(bl)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into rapprochement (facture_id, bl_id, methode, ecarts)
         values ($1, $2, 'manuel', '[{\"genre\": \"prix\"}]')",
    )
    .bind(ecart)
    .bind(bl)
    .execute(app.pool())
    .await
    .unwrap();

    let ids = json!({ "ids": [pret1, pret2, alerte, ailleurs, ecart] });
    let r = app
        .requete(
            "POST",
            "/api/documents/lot/eligibilite",
            Some(ids.clone()),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let el = r.json.as_array().unwrap();
    assert_eq!(
        el.len(),
        4,
        "la pièce de Houdemont n'est pas rendue : {}",
        r.json
    );
    let de = |id: i64| el.iter().find(|e| e["id"] == id).unwrap().clone();
    assert_eq!(de(pret1)["possible"], true);
    assert_eq!(de(alerte)["possible"], false);
    assert_eq!(de(alerte)["raison"], "1 alerte à lire");
    assert_eq!(de(ecart)["raison"], "1 alerte à lire");

    // Bornes.
    let trop: Vec<i64> = (1..=101).collect();
    assert_eq!(
        app.requete(
            "POST",
            "/api/documents/lot/valider",
            Some(json!({ "ids": trop })),
            resp
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );

    // Valider le lot : le serveur revérifie chaque pièce.
    let r = app
        .requete("POST", "/api/documents/lot/valider", Some(ids), resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["validees"], 2, "{}", r.json);
    assert_eq!(r.json["laissees"], 3);
    let piece = |id: i64| {
        r.json["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(piece(alerte)["raison"], "1 alerte à lire");
    assert_eq!(piece(ailleurs)["validee"], false);
    assert_eq!(doc(app, ailleurs).await["valide"], false);
    assert_eq!(doc(app, alerte).await["valide"], false);
    assert_eq!(doc(app, alerte).await["verifie"], false, "rien d'écrit");
    assert_eq!(doc(app, pret1).await["valide"], true);
    let lot = r.json["annulation"]["lot"].as_str().unwrap().to_string();
    assert_eq!(
        app.compter(&format!(
            "select count(*) from envoi_prevu where lot = '{lot}' and etat = 'attente'"
        ))
        .await,
        2
    );

    // Une pièce du lot déjà partie : les autres s'annulent, résumé de ce qui est parti.
    sqlx::query("update envoi_prevu set etat = 'pris' where document_id = $1")
        .bind(pret2)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            resp,
        )
        .await;
    assert_eq!(r.json["annulees"], 1, "{}", r.json);
    assert_eq!(r.json["deja_parties"], 1);
    assert_eq!(doc(app, pret1).await["valide"], false);
    assert_eq!(doc(app, pret1).await["statut_depot"], "pret");
    m.app.fermer().await;
}

#[tokio::test]
async fn ecarter_avec_motif_puis_remettre_a_controler() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.c_resp_a.as_str());
    let f = facture(&m, m.boite_a, m.sa, "E-1", false).await;
    let avant = doc(app, f).await;
    let url = format!("/api/documents/{f}/ecarter");
    assert_eq!(
        app.requete("POST", &url, Some(json!({ "motif": "n_importe" })), resp)
            .await
            .code,
        StatusCode::BAD_REQUEST
    );
    let r = app
        .requete(
            "POST",
            &url,
            Some(json!({ "motif": "pas_pour_nous", "texte": "  Magasin de Nancy " })),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let fiche = app
        .requete("GET", &format!("/api/documents/{f}"), None, resp)
        .await;
    assert_eq!(fiche.json["etat"], "ignore");
    assert_eq!(fiche.json["ecartement"]["motif"], "pas_pour_nous");
    assert_eq!(fiche.json["ecartement"]["texte"], "Magasin de Nancy");
    assert_eq!(fiche.json["ecartement"]["par_nom"], "resp-a");
    assert!(fiche.json["ecartement"].get("avant").is_none());

    // Pas sa société : refusé.
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/documents/{f}/remettre"),
            None,
            Some(&m.c_resp_b)
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    let r = app
        .requete("POST", &format!("/api/documents/{f}/remettre"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(doc(app, f).await, avant);
    let fiche = app
        .requete("GET", &format!("/api/documents/{f}"), None, resp)
        .await;
    assert!(fiche.json["ecartement"].is_null());
    assert_eq!(
        app.compter("select count(*) from journal where action = 'ecartement_annule'")
            .await,
        1
    );
    // Une pièce qui n'est pas écartée ne se « remet » pas.
    assert_eq!(
        app.requete("POST", &format!("/api/documents/{f}/remettre"), None, resp)
            .await
            .code,
        StatusCode::CONFLICT
    );
    // Sans corps : écarter marche comme avant (sans motif).
    let r = app.requete("POST", &url, None, Some(&m.c_admin)).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    m.app.fermer().await;
}
