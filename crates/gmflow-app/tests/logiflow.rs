//! Réglage Logiflow (adresse, clé jamais renvoyée), résultat sur la fiche d'une facture et
//! « Relancer ».

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

#[tokio::test]
async fn reglage_et_relance() {
    let app = Appli::new().await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;

    let r = app
        .requete("GET", "/api/admin/logiflow", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert!(r.json["serveur"].is_null());
    let r = app
        .requete("POST", "/api/admin/logiflow/test", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    let r = app
        .requete(
            "PUT",
            "/api/admin/logiflow",
            Some(json!({ "adresse": "logiflow.x" })),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert!(
        !r.json.to_string().contains("ffnancy.fr/api")
            && !r.json.to_string().contains("api.ffnancy")
    );

    let r = app
        .requete(
            "PUT",
            "/api/admin/logiflow",
            Some(json!({ "adresse": "https://logiflow.exemple.fr", "jeton": "cle-du-test-ABCD" })),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT, "{:?}", r.json);
    let r = app
        .requete("GET", "/api/admin/logiflow", None, Some(&c))
        .await;
    assert_eq!(r.json["serveur"]["fin"], "ABCD");
    assert_eq!(r.json["jeton_enregistre"], true);
    assert!(!r.json.to_string().contains("cle-du-test"));

    // Une facture envoyée et son résultat Logiflow, visible sur la fiche.
    let sa = app.societe("Frouard").await;
    let ba = app.boite(sa, "Compta").await;
    let eml = gmflow_core::fixtures::eml(
        "f1@test",
        "F1",
        &[("f1.pdf", "application/pdf", b"%PDF-1.4 x")],
        false,
    );
    let mail = app.mail_vu(&eml, &[ba]).await;
    let piece = app.piece_de(mail).await;
    let doc: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id, numero, statut_depot, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'facture', $2, 'F1', 'erreur', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(sa)
    .fetch_one(app.pool())
    .await
    .unwrap();
    let chemin = format!("/api/documents/{doc}/logiflow");
    // Pas encore envoyée : refusé.
    let r = app.requete("POST", &chemin, None, Some(&c)).await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    sqlx::query("update document set statut_depot = 'depose' where id = $1")
        .bind(doc)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into logiflow_rapprochement (document_id, ok, message, livraisons)
         values ($1, true, '1 livraison complétée dans Logiflow', '[{\"bl\": \"BL1\", \"etat\": \"complete\"}]')",
    )
    .bind(doc)
    .execute(app.pool())
    .await
    .unwrap();
    let r = app
        .requete("GET", &format!("/api/documents/{doc}"), None, Some(&c))
        .await;
    assert_eq!(r.json["logiflow"]["ok"], true, "{:?}", r.json["logiflow"]);
    assert_eq!(r.json["logiflow"]["livraisons"][0]["bl"], "BL1");
    let r = app.requete("POST", &chemin, None, Some(&c)).await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    let r = app
        .requete("GET", &format!("/api/documents/{doc}"), None, Some(&c))
        .await;
    assert!(r.json["logiflow"].is_null());

    let r = app
        .requete("DELETE", "/api/admin/logiflow", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = app.requete("POST", &chemin, None, Some(&c)).await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    app.fermer().await;
}
