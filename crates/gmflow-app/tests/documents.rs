mod commun;

use axum::http::StatusCode;
use commun::{Appli, fixture};
use serde_json::json;

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
    sa: i64,
    sb: i64,
    doc_a: i64,
    doc_b: i64,
    mail_a: i64,
}

/// Document lu (comme la lecture l'aurait enregistré) pour la première pièce du mail.
async fn doc_lu(app: &Appli, mail_id: i64, type_doc: &str, societe_id: Option<i64>) -> i64 {
    let piece = app.piece_de(mail_id).await;
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, date_document, montant_ttc, confiance, texte, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', $2, $3, 'Durand', 'F-1', date '2026-11-12',
                60.00, '{\"type\": 0.9}', 'FACTURE F-1', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(type_doc)
    .bind(societe_id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    let boite_a = app.boite(sa, "Compta Frouard").await;
    let boite_b = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    let mail_a = app.mail_vu(&fixture("pdf.eml"), &[boite_a]).await;
    let mail_b = app.mail_vu(&fixture("inline.eml"), &[boite_b]).await;
    let doc_a = doc_lu(&app, mail_a, "facture", Some(sa)).await;
    let doc_b = doc_lu(&app, mail_b, "bl", Some(sb)).await;
    let c_admin = app.connecter("admin", MDP).await;
    let c_resp_a = app.connecter("resp-a", MDP).await;
    Monde {
        app,
        c_admin,
        c_resp_a,
        sa,
        sb,
        doc_a,
        doc_b,
        mail_a,
    }
}

#[tokio::test]
async fn responsable_ne_voit_que_les_documents_de_sa_structure() {
    let m = monde().await;
    let app = &m.app;
    let c = Some(m.c_resp_a.as_str());
    let r = app.requete("GET", "/api/documents", None, c).await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["total"], 1);
    assert_eq!(r.json["elements"][0]["id"], m.doc_a);
    assert_eq!(r.json["elements"][0]["date_document"], "2026-11-12");
    assert_eq!(r.json["elements"][0]["montant_ttc"], 60.0);
    assert_eq!(
        app.requete("GET", &format!("/api/documents/{}", m.doc_b), None, c)
            .await
            .code,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_b),
            Some(json!({ "type": "bl", "societe_id": null })),
            c
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    let r = app
        .requete("GET", &format!("/api/documents/{}", m.doc_a), None, c)
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["texte"], "FACTURE F-1");
    // La fiche mail montre l'état du document de chaque pièce.
    let r = app
        .requete("GET", &format!("/api/mails/{}", m.mail_a), None, c)
        .await;
    assert_eq!(r.json["pieces"][0]["document_id"], m.doc_a);
    assert_eq!(r.json["pieces"][0]["document_etat"], "lu");
    // Bilan limité à sa structure.
    let r = app.requete("GET", "/api/documents/bilan", None, c).await;
    assert_eq!(r.json["par_etat"][0]["n"], 1);
    // L'admin voit les deux.
    let r = app
        .requete("GET", "/api/documents", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.json["total"], 2);
    m.app.fermer().await;
}

#[tokio::test]
async fn verification_et_taux_de_justesse() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    // Confirmé tel quel : juste.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_a),
            Some(json!({ "type": "facture", "societe_id": m.sa })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    // Type corrigé : faux.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_b),
            Some(json!({ "type": "facture", "societe_id": m.sb })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    // Vérifié de nouveau sans changement : la correction reste comptée.
    app.requete(
        "POST",
        &format!("/api/documents/{}/verifier", m.doc_b),
        Some(json!({ "type": "facture", "societe_id": m.sb })),
        admin,
    )
    .await;
    let r = app
        .requete("GET", "/api/documents/bilan", None, admin)
        .await;
    assert_eq!(
        (
            r.json["verifies"].as_i64(),
            r.json["type_justes"].as_i64(),
            r.json["societe_justes"].as_i64()
        ),
        (Some(2), Some(1), Some(2))
    );
    let r = app
        .requete("GET", &format!("/api/documents/{}", m.doc_b), None, admin)
        .await;
    assert_eq!(r.json["type_corrige"], true);
    assert_eq!(r.json["verifie_par"], "admin");
    assert_eq!(
        app.compter("select count(*) from journal where action = 'correction'")
            .await,
        1
    );
    assert_eq!(
        app.compter("select count(*) from journal where action = 'verification'")
            .await,
        2
    );
    // Filtre « à vérifier ».
    let r = app
        .requete("GET", "/api/documents?verifie=non", None, admin)
        .await;
    assert_eq!(r.json["total"], 0);
    // Données refusées : type inconnu, structure d'un autre (pour un responsable), document non lu.
    let resp = Some(m.c_resp_a.as_str());
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_a),
            Some(json!({ "type": "bon", "societe_id": null })),
            resp
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_a),
            Some(json!({ "type": "facture", "societe_id": m.sb })),
            resp
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );
    sqlx::query("update document set etat = 'erreur' where id = $1")
        .bind(m.doc_a)
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/documents/{}/verifier", m.doc_a),
            Some(json!({ "type": "facture", "societe_id": m.sa })),
            resp
        )
        .await
        .code,
        StatusCode::CONFLICT
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn relecture_reservee_a_l_admin() {
    let m = monde().await;
    let app = &m.app;
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/admin/documents/{}/relire", m.doc_a),
            None,
            Some(&m.c_resp_a)
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    app.requete(
        "POST",
        &format!("/api/documents/{}/verifier", m.doc_a),
        Some(json!({ "type": "avoir", "societe_id": m.sa })),
        Some(&m.c_admin),
    )
    .await;
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/documents/{}/relire", m.doc_a),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    let r = app
        .requete(
            "GET",
            &format!("/api/documents/{}", m.doc_a),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.json["etat"], "a_lire");
    assert_eq!(r.json["verifie_le"], serde_json::Value::Null);
    assert_eq!(r.json["type_corrige"], false);
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'lecture' and convert_from(job, 'UTF8') like '%\"forcer\":true%'").await,
        1
    );
    sqlx::query("update document set etat = 'en_cours' where id = $1")
        .bind(m.doc_a)
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(
        app.requete(
            "POST",
            &format!("/api/admin/documents/{}/relire", m.doc_a),
            None,
            Some(&m.c_admin)
        )
        .await
        .code,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.requete(
            "POST",
            "/api/admin/documents/999999/relire",
            None,
            Some(&m.c_admin)
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn reglages_claude_et_journal_des_appels() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    assert_eq!(
        app.requete("GET", "/api/admin/claude/reglages", None, Some(&m.c_resp_a))
            .await
            .code,
        StatusCode::FORBIDDEN
    );
    let r = app
        .requete("GET", "/api/admin/claude/reglages", None, admin)
        .await;
    assert_eq!(
        r.json["extraction_texte"],
        json!({ "modele": "haiku", "effort": "low" })
    );
    assert_eq!(
        r.json["ocr_scan"],
        json!({ "modele": "haiku", "effort": "medium" })
    );
    assert_eq!(
        r.json["nouvel_essai"],
        json!({ "modele": "sonnet", "effort": "medium" })
    );
    assert_eq!(r.json["quota_lecture_jour"], 100);
    assert_eq!(r.json["lecture_auto"], true);
    let mut nouveau = json!({
        "extraction_texte": { "modele": "haiku", "effort": "low" },
        "ocr_scan": { "modele": "sonnet", "effort": "low" },
        "nouvel_essai": { "modele": "opus", "effort": "medium" },
        "lecture_auto": false, "quota_lecture_jour": 40
    });
    let r = app
        .requete(
            "PUT",
            "/api/admin/claude/reglages",
            Some(nouveau.clone()),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let r = app
        .requete("GET", "/api/admin/claude/reglages", None, admin)
        .await;
    assert_eq!(r.json["ocr_scan"]["modele"], "sonnet");
    assert_eq!(r.json["quota_lecture_jour"], 40);
    assert_eq!(
        app.compter("select count(*) from journal where action = 'reglages_claude'")
            .await,
        1
    );
    nouveau["ocr_scan"]["modele"] = json!("gpt");
    assert_eq!(
        app.requete(
            "PUT",
            "/api/admin/claude/reglages",
            Some(nouveau.clone()),
            admin
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );
    nouveau["ocr_scan"]["modele"] = json!("haiku");
    nouveau["quota_lecture_jour"] = json!(-1);
    assert_eq!(
        app.requete("PUT", "/api/admin/claude/reglages", Some(nouveau), admin)
            .await
            .code,
        StatusCode::BAD_REQUEST
    );

    sqlx::query(
        "insert into appel_claude (tache, modele, effort, duree_ms, succes) values
         ('extraction_texte', 'haiku', 'low', 4000, true), ('extraction_texte', 'haiku', 'low', 6000, false),
         ('test_jeton', 'haiku', 'low', 1000, true)",
    )
    .execute(app.pool())
    .await
    .unwrap();
    let r = app
        .requete("GET", "/api/admin/claude/appels", None, admin)
        .await;
    assert_eq!(r.json["total"], 3);
    let stat = r.json["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tache"] == "extraction_texte")
        .unwrap()
        .clone();
    assert_eq!(
        (
            stat["n"].as_i64(),
            stat["reussis"].as_i64(),
            stat["duree_moyenne_ms"].as_i64()
        ),
        (Some(2), Some(1), Some(5000))
    );
    let r = app
        .requete("GET", "/api/admin/claude/reglages", None, admin)
        .await;
    assert_eq!(r.json["appels_lecture_du_jour"], 2);
    m.app.fermer().await;
}
