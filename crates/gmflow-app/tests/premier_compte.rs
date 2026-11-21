mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

#[tokio::test]
async fn jeton_a_usage_unique() {
    let app = Appli::new().await;
    let jeton = gmflow_app::premier_compte::annoncer_si_besoin(&app.etat)
        .await
        .unwrap()
        .expect("jeton");
    assert_eq!(
        app.requete("GET", &format!("/api/premier-compte/{jeton}"), None, None)
            .await
            .code,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.requete("GET", "/api/premier-compte/faux", None, None)
            .await
            .code,
        StatusCode::NOT_FOUND
    );

    let corps = json!({"jeton": jeton, "identifiant": "michael", "mot_de_passe": "court"});
    assert_eq!(
        app.requete("POST", "/api/premier-compte", Some(corps), None)
            .await
            .code,
        StatusCode::BAD_REQUEST
    );

    let corps = json!({"jeton": jeton, "identifiant": "michael", "mot_de_passe": MDP});
    let r = app
        .requete("POST", "/api/premier-compte", Some(corps.clone()), None)
        .await;
    assert_eq!(r.code, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(
        app.requete("POST", "/api/premier-compte", Some(corps), None)
            .await
            .code,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.requete("GET", &format!("/api/premier-compte/{jeton}"), None, None)
            .await
            .code,
        StatusCode::NOT_FOUND
    );
    app.connecter("michael", MDP).await;
    let n: i64 = sqlx::query_scalar("select count(*) from journal where action = 'premier_compte'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    app.fermer().await;
}

#[tokio::test]
async fn deux_creations_simultanees() {
    let app = Appli::new().await;
    let jeton = gmflow_app::premier_compte::annoncer_si_besoin(&app.etat)
        .await
        .unwrap()
        .unwrap();
    let a = json!({"jeton": jeton, "identifiant": "un", "mot_de_passe": MDP});
    let b = json!({"jeton": jeton, "identifiant": "deux", "mot_de_passe": MDP});
    let (ra, rb) = tokio::join!(
        app.requete("POST", "/api/premier-compte", Some(a), None),
        app.requete("POST", "/api/premier-compte", Some(b), None)
    );
    let mut codes = [ra.code, rb.code];
    codes.sort();
    assert_eq!(codes, [StatusCode::CREATED, StatusCode::NOT_FOUND]);
    let n: i64 = sqlx::query_scalar("select count(*) from utilisateur")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    app.fermer().await;
}

#[tokio::test]
async fn utilisateur_existant() {
    let app = Appli::new().await;
    app.utilisateur("michael", MDP, "admin", None).await;
    assert!(
        gmflow_app::premier_compte::annoncer_si_besoin(&app.etat)
            .await
            .unwrap()
            .is_none()
    );
    // Même avec un jeton resté en mémoire, la création est refusée.
    *app.etat.premier_compte.lock().unwrap() = Some(gmflow_app::etat::JetonPremierCompte {
        jeton: "abc".into(),
        cree: std::time::Instant::now(),
    });
    let r = app
        .requete(
            "POST",
            "/api/premier-compte",
            Some(json!({"jeton": "abc", "identifiant": "x", "mot_de_passe": MDP})),
            None,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    app.fermer().await;
}
