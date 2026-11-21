mod commun;

use std::time::Duration;

use axum::http::StatusCode;
use commun::Appli;
use gmflow_app::DureesSession;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

#[tokio::test]
async fn connexion_et_echecs() {
    let app = Appli::new().await;
    app.utilisateur("michael", MDP, "admin", None).await;

    let r = app
        .requete(
            "POST",
            "/api/connexion",
            Some(json!({"identifiant": "michael", "mot_de_passe": MDP})),
            None,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["role"], "admin");
    let cookie = r.cookie.expect("cookie");
    let set_cookie = r
        .entetes
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_lowercase();
    assert!(
        set_cookie.contains("httponly")
            && set_cookie.contains("secure")
            && set_cookie.contains("samesite=strict"),
        "{set_cookie}"
    );

    let moi = app.requete("GET", "/api/moi", None, Some(&cookie)).await;
    assert_eq!(moi.code, StatusCode::OK);
    assert_eq!(moi.json["identifiant"], "michael");

    for _ in 0..5 {
        let r = app
            .requete(
                "POST",
                "/api/connexion",
                Some(json!({"identifiant": "michael", "mot_de_passe": "faux"})),
                None,
            )
            .await;
        assert_eq!(r.code, StatusCode::UNAUTHORIZED);
    }
    // 6e essai dans les 15 min : refusé même avec le bon mot de passe.
    let r = app
        .requete(
            "POST",
            "/api/connexion",
            Some(json!({"identifiant": "michael", "mot_de_passe": MDP})),
            None,
        )
        .await;
    assert_eq!(r.code, StatusCode::TOO_MANY_REQUESTS);

    let n: i64 =
        sqlx::query_scalar("select count(*) from journal where action = 'connexion_refusee'")
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(n, 5);

    let r = app
        .requete("POST", "/api/deconnexion", None, Some(&cookie))
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&cookie))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    app.fermer().await;
}

#[tokio::test]
async fn sans_session_et_sans_entete() {
    let app = Appli::new().await;
    app.utilisateur("michael", MDP, "admin", None).await;
    assert_eq!(
        app.requete("GET", "/api/moi", None, None).await.code,
        StatusCode::UNAUTHORIZED
    );
    let cookie = app.connecter("michael", MDP).await;
    let r = app
        .requete_brute(
            "POST",
            "/api/admin/societes",
            Some(json!({"raison_sociale": "X"})),
            Some(&cookie),
            false,
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete(
            "POST",
            "/api/admin/societes",
            Some(json!({"raison_sociale": "X"})),
            Some(&cookie),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    app.fermer().await;
}

#[tokio::test]
async fn session_trop_vieille() {
    // Limite absolue réduite à 1 s (7 jours en production).
    let app = Appli::avec_durees(DureesSession {
        inactivite: Duration::from_secs(3600),
        absolue: Duration::from_secs(1),
    })
    .await;
    app.utilisateur("michael", MDP, "admin", None).await;
    let cookie = app.connecter("michael", MDP).await;
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&cookie))
            .await
            .code,
        StatusCode::OK
    );
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&cookie))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    app.fermer().await;
}

#[tokio::test]
async fn session_inactive() {
    // Inactivité réduite à 2 s (8 h en production) ; chaque requête prolonge la session.
    let app = Appli::avec_durees(DureesSession {
        inactivite: Duration::from_secs(2),
        absolue: Duration::from_secs(3600),
    })
    .await;
    app.utilisateur("michael", MDP, "admin", None).await;
    let cookie = app.connecter("michael", MDP).await;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(
            app.requete("GET", "/api/moi", None, Some(&cookie))
                .await
                .code,
            StatusCode::OK
        );
    }
    tokio::time::sleep(Duration::from_millis(3100)).await;
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&cookie))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    app.fermer().await;
}

#[tokio::test]
async fn compte_desactive_et_responsable() {
    let app = Appli::new().await;
    let soc = app.societe("Frouard").await;
    let admin = app.utilisateur("michael", MDP, "admin", None).await;
    let resp = app.utilisateur("resp", MDP, "responsable", Some(soc)).await;
    let c_resp = app.connecter("resp", MDP).await;
    assert_eq!(
        app.requete("GET", "/api/admin/utilisateurs", None, Some(&c_resp))
            .await
            .code,
        StatusCode::FORBIDDEN
    );

    let c_admin = app.connecter("michael", MDP).await;
    let r = app
        .requete("PUT", &format!("/api/admin/utilisateurs/{resp}"), Some(json!({"identifiant": "resp", "role": "responsable", "societe_id": soc, "actif": false})), Some(&c_admin))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    // La session du compte désactivé ne sert plus.
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&c_resp))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    // Un administrateur ne peut pas se retirer ses droits.
    let r = app
        .requete(
            "PUT",
            &format!("/api/admin/utilisateurs/{admin}"),
            Some(json!({"identifiant": "michael", "role": "admin", "actif": false})),
            Some(&c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    // Responsable sans structure : refusé.
    let r = app
        .requete(
            "POST",
            "/api/admin/utilisateurs",
            Some(json!({"identifiant": "x", "mot_de_passe": MDP, "role": "responsable"})),
            Some(&c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let r = app
        .requete("GET", "/api/admin/utilisateurs", None, Some(&c_admin))
        .await;
    assert!(
        r.json
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u.get("hash_mdp").is_none())
    );
    app.fermer().await;
}
