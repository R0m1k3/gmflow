//! Essai de connexion à l'API du serveur contre un faux serveur local.

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use gmflow_worker::serveur_api::essayer;

async fn faux_serveur() -> String {
    let app = Router::new().route(
        "/v1",
        get(|h: HeaderMap| async move {
            if h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer bon") {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    adresse
}

#[tokio::test]
async fn essai_de_connexion() {
    let base = faux_serveur().await;
    let v1 = format!("{base}/v1");
    assert!(
        essayer(&v1, Some("bon"))
            .await
            .unwrap()
            .contains("HTTP 200")
    );
    assert!(
        essayer(&v1, Some("mauvais"))
            .await
            .unwrap_err()
            .contains("jeton refusé")
    );
    assert!(
        essayer(&v1, None)
            .await
            .unwrap_err()
            .contains("jeton refusé")
    );
    assert!(
        essayer(&format!("{base}/autre"), Some("bon"))
            .await
            .unwrap_err()
            .contains("404")
    );
    // Port fermé.
    let e = essayer("http://127.0.0.1:1/", Some("bon"))
        .await
        .unwrap_err();
    assert!(e.contains("connexion impossible"), "{e}");
}
