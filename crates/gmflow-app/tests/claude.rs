mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";
const JETON: &str = "sk-ant-oat01-secret-du-test-WXYZ";

#[tokio::test]
async fn seance_unique_et_code() {
    let app = Appli::new().await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;
    let r = app
        .requete("POST", "/api/admin/claude/seances", None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    let id = r.json["id"].as_str().unwrap().to_string();
    assert_eq!(
        app.requete("POST", "/api/admin/claude/seances", None, Some(&c))
            .await
            .code,
        StatusCode::CONFLICT
    );

    let r = app
        .requete(
            "GET",
            &format!("/api/admin/claude/seances/{id}"),
            None,
            Some(&c),
        )
        .await;
    assert_eq!(r.json["etat"], "demarrage");
    // Pas encore d'URL : le code est refusé.
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/claude/seances/{id}/code"),
            Some(json!({"code": "abc"})),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    sqlx::query("update seance_claude set etat = 'attente_code', url = 'https://claude.ai/x'")
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/claude/seances/{id}/code"),
            Some(json!({"code": "abc"})),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let chiffre: Vec<u8> = sqlx::query_scalar("select code_chiffre from seance_claude")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(
        app.etat.chiffreur.dechiffrer_texte(&chiffre).unwrap(),
        "abc"
    );

    // Une séance restée ouverte au-delà de son délai ne bloque plus.
    sqlx::query("update seance_claude set expire_le = now() - interval '1 second'")
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(
        app.requete("POST", "/api/admin/claude/seances", None, Some(&c))
            .await
            .code,
        StatusCode::OK
    );
    app.fermer().await;
}

#[tokio::test]
async fn jeton_jamais_renvoye() {
    let app = Appli::new().await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;
    assert_eq!(
        app.requete(
            "PUT",
            "/api/admin/claude/jeton",
            Some(json!({"jeton": "pas un jeton"})),
            Some(&c)
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );
    let r = app
        .requete(
            "PUT",
            "/api/admin/claude/jeton",
            Some(json!({"jeton": JETON})),
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    assert!(!String::from_utf8_lossy(&r.octets).contains("secret-du-test"));

    let r = app
        .requete("GET", "/api/admin/claude", None, Some(&c))
        .await;
    assert_eq!(r.json["fin"], "WXYZ");
    assert_eq!(r.json["etat"], "en_test");
    let r2 = app
        .requete("GET", "/api/tableau-de-bord", None, Some(&c))
        .await;
    let r3 = app
        .requete("GET", "/api/admin/journal", None, Some(&c))
        .await;
    for rep in [&r, &r2, &r3] {
        assert!(!String::from_utf8_lossy(&rep.octets).contains("secret-du-test"));
    }
    // Chiffré en base, test poussé.
    let brut: Vec<u8> =
        sqlx::query_scalar("select valeur_chiffree from parametre where cle = 'claude_jeton'")
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&brut).contains("secret-du-test"));
    let n: i64 = sqlx::query_scalar("select count(*) from apalis.jobs where job_type = 'claude'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);

    assert_eq!(
        app.requete("DELETE", "/api/admin/claude/jeton", None, Some(&c))
            .await
            .code,
        StatusCode::NO_CONTENT
    );
    let r = app
        .requete("GET", "/api/admin/claude", None, Some(&c))
        .await;
    assert_eq!(r.json["etat"], "absent");
    app.fermer().await;
}

async fn en_cours(app: &Appli, c: &str) -> serde_json::Value {
    let r = app
        .requete("GET", "/api/admin/claude/seances/en-cours", None, Some(c))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    r.json
}

#[tokio::test]
async fn seance_reprise_et_annulee() {
    let app = Appli::new().await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;
    assert!(en_cours(&app, &c).await.is_null());

    let id = app
        .requete("POST", "/api/admin/claude/seances", None, Some(&c))
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    sqlx::query("update seance_claude set etat = 'attente_code', url = 'https://claude.ai/x'")
        .execute(app.pool())
        .await
        .unwrap();
    // Page rechargée : la séance ouverte est retrouvée avec son lien.
    let r = en_cours(&app, &c).await;
    assert_eq!(r["id"], id.as_str());
    assert_eq!(r["etat"], "attente_code");
    assert_eq!(r["url"], "https://claude.ai/x");

    let annuler = format!("/api/admin/claude/seances/{id}/annuler");
    assert_eq!(
        app.requete("POST", &annuler, None, Some(&c)).await.code,
        StatusCode::NO_CONTENT
    );
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/claude/seances/{id}"),
            None,
            Some(&c),
        )
        .await;
    assert_eq!(r.json["etat"], "erreur");
    assert_eq!(r.json["message"], "annulée");
    assert!(en_cours(&app, &c).await.is_null());
    // Annuler deux fois ne fait rien ; une séance inconnue est absente.
    assert_eq!(
        app.requete("POST", &annuler, None, Some(&c)).await.code,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.requete(
            "POST",
            "/api/admin/claude/seances/00000000-0000-0000-0000-000000000000/annuler",
            None,
            Some(&c)
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    // Une nouvelle séance peut partir tout de suite.
    assert_eq!(
        app.requete("POST", "/api/admin/claude/seances", None, Some(&c))
            .await
            .code,
        StatusCode::OK
    );

    // Une séance ouverte au-delà de son délai n'est plus « en cours ».
    sqlx::query(
        "update seance_claude set expire_le = now() - interval '1 second' where etat = 'demarrage'",
    )
    .execute(app.pool())
    .await
    .unwrap();
    assert!(en_cours(&app, &c).await.is_null());
    app.fermer().await;
}
