mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

#[tokio::test]
async fn adresse_et_jeton_du_serveur_api() {
    let app = Appli::new().await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;

    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert!(r.json["serveur"].is_null());
    // Essai sans adresse : refusé.
    let r = app
        .requete("POST", "/api/admin/serveur-api/test", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);

    let r = app
        .requete(
            "PUT",
            "/api/admin/serveur-api",
            Some(json!({ "adresse": "api.exemple.fr", "jeton": "abc" })),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);

    let r = app
        .requete(
            "PUT",
            "/api/admin/serveur-api",
            Some(
                json!({ "adresse": " https://api.exemple.fr/v1 ", "jeton": "secret-du-test-WXYZ" }),
            ),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT, "{:?}", r.json);
    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert_eq!(r.json["serveur"]["adresse"], "https://api.exemple.fr/v1");
    assert_eq!(r.json["serveur"]["fin"], "WXYZ");
    assert_eq!(r.json["jeton_enregistre"], true);
    assert!(!r.json.to_string().contains("secret-du-test"));

    // Adresse changée sans jeton : le jeton reste.
    app.requete(
        "PUT",
        "/api/admin/serveur-api",
        Some(json!({ "adresse": "https://api2.exemple.fr" })),
        Some(&c),
    )
    .await;
    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert_eq!(r.json["serveur"]["adresse"], "https://api2.exemple.fr");
    assert_eq!(r.json["jeton_enregistre"], true);
    assert_eq!(r.json["serveur"]["fin"], "WXYZ");

    let r = app
        .requete("POST", "/api/admin/serveur-api/test", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);

    // Jeton vide : supprimé.
    app.requete(
        "PUT",
        "/api/admin/serveur-api",
        Some(json!({ "adresse": "https://api2.exemple.fr", "jeton": "" })),
        Some(&c),
    )
    .await;
    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert_eq!(r.json["jeton_enregistre"], false);
    assert!(r.json["serveur"]["fin"].is_null());

    let r = app
        .requete("DELETE", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c))
        .await;
    assert!(r.json["serveur"].is_null());

    // Un responsable n'y a pas accès.
    let sa = app.societe("A").await;
    app.utilisateur("resp", MDP, "responsable", Some(sa)).await;
    let c2 = app.connecter("resp", MDP).await;
    let r = app
        .requete("GET", "/api/admin/serveur-api", None, Some(&c2))
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    app.fermer().await;
}
