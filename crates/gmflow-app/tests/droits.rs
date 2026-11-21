mod commun;

use axum::http::StatusCode;
use commun::{Appli, fixture};
use serde_json::json;

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
    boite_a: i64,
    boite_b: i64,
    mail_b: i64,
    mail_ab: i64,
    mail_a: i64,
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
    let mail_b = app.mail_vu(&fixture("inline.eml"), &[boite_b]).await;
    let mail_ab = app.mail_vu(&fixture("pdf.eml"), &[boite_a, boite_b]).await;
    let mail_a = app.mail_vu(&fixture("simple.eml"), &[boite_a]).await;
    let c_admin = app.connecter("admin", MDP).await;
    let c_resp_a = app.connecter("resp-a", MDP).await;
    Monde {
        app,
        c_admin,
        c_resp_a,
        boite_a,
        boite_b,
        mail_b,
        mail_ab,
        mail_a,
    }
}

#[tokio::test]
async fn responsable_limite_a_sa_structure() {
    let m = monde().await;
    let app = &m.app;
    let c = Some(m.c_resp_a.as_str());

    assert_eq!(
        app.requete("GET", &format!("/api/mails/{}", m.mail_b), None, c)
            .await
            .code,
        StatusCode::NOT_FOUND
    );
    let piece_b = app.piece_de(m.mail_b).await;
    assert_eq!(
        app.requete("GET", &format!("/api/pieces/{piece_b}"), None, c)
            .await
            .code,
        StatusCode::NOT_FOUND
    );

    let r = app
        .requete("GET", &format!("/api/mails/{}", m.mail_ab), None, c)
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let boites = r.json["boites"].as_array().unwrap();
    assert_eq!(boites.len(), 1);
    assert_eq!(boites[0]["boite_id"], m.boite_a);

    let r = app.requete("GET", "/api/mails", None, c).await;
    assert_eq!(r.json["total"], 2);
    let ids: Vec<i64> = r.json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&m.mail_ab) && ids.contains(&m.mail_a));
    for e in r.json["elements"].as_array().unwrap() {
        assert_eq!(e["boites"], json!(["Compta Frouard"]));
    }
    // Filtrer sur la boîte de B ne montre rien.
    let r = app
        .requete("GET", &format!("/api/mails?boite={}", m.boite_b), None, c)
        .await;
    assert_eq!(r.json["total"], 0);

    assert_eq!(
        app.requete("GET", "/api/admin/boites", None, c).await.code,
        StatusCode::FORBIDDEN
    );
    let r = app.requete("GET", "/api/boites", None, c).await;
    assert_eq!(r.json.as_array().unwrap().len(), 1);
    let r = app.requete("GET", "/api/societes", None, c).await;
    assert_eq!(r.json.as_array().unwrap().len(), 1);

    let r = app.requete("GET", "/api/tableau-de-bord", None, c).await;
    assert_eq!(r.code, StatusCode::OK);
    assert!(r.json.get("claude").is_none() && r.json.get("stockage").is_none());
    assert_eq!(r.json["mails_du_jour"].as_array().unwrap().len(), 1);
    m.app.fermer().await;
}

#[tokio::test]
async fn admin_voit_tout_et_telecharge() {
    let m = monde().await;
    let app = &m.app;
    let c = Some(m.c_admin.as_str());
    let r = app.requete("GET", "/api/mails", None, c).await;
    assert_eq!(r.json["total"], 3);
    let r = app
        .requete("GET", &format!("/api/mails/{}", m.mail_ab), None, c)
        .await;
    assert_eq!(r.json["boites"].as_array().unwrap().len(), 2);
    assert_eq!(r.json["objet"], "Facture n°2026-001");

    let fiche = app
        .requete("GET", &format!("/api/mails/{}", m.mail_b), None, c)
        .await;
    for p in fiche.json["pieces"].as_array().unwrap() {
        let r = app
            .requete("GET", &format!("/api/pieces/{}", p["id"]), None, c)
            .await;
        assert_eq!(r.code, StatusCode::OK);
        assert_eq!(r.octets.len() as i64, p["taille"].as_i64().unwrap());
        let disp = r
            .entetes
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disp.starts_with("attachment;"), "{disp}");
    }

    let r = app.requete("GET", "/api/mails?q=facture", None, c).await;
    assert_eq!(r.json["total"], 2, "{:?}", r.json);
    let r = app.requete("GET", "/api/mails?q=%25", None, c).await;
    assert_eq!(r.json["total"], 0);
    let r = app
        .requete("GET", "/api/mails?du=2026-11-18&au=2026-11-18", None, c)
        .await;
    assert_eq!(r.json["total"], 1, "{:?}", r.json);
    assert_eq!(
        app.requete("GET", "/api/mails?du=hier", None, c).await.code,
        StatusCode::BAD_REQUEST
    );

    let r = app.requete("GET", "/api/tableau-de-bord", None, c).await;
    assert!(r.json["stockage"]["octets"].as_i64().unwrap() > 0);
    assert_eq!(r.json["claude"]["etat"], "absent");
    m.app.fermer().await;
}

#[tokio::test]
async fn piece_alteree() {
    let m = monde().await;
    let app = &m.app;
    let piece = app.piece_de(m.mail_ab).await;
    sqlx::query("update piece_jointe set sha256 = '\\x00' where id = $1")
        .bind(piece)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{piece}"),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::INTERNAL_SERVER_ERROR);
    let n: i64 = sqlx::query_scalar("select count(*) from journal where action = 'piece_alteree'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    m.app.fermer().await;
}

#[tokio::test]
async fn administration_des_boites() {
    let m = monde().await;
    let app = &m.app;
    let c = Some(m.c_admin.as_str());
    let soc: i64 = sqlx::query_scalar("select id from societe limit 1")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let r = app
        .requete("POST", "/api/admin/boites", Some(json!({"societe_id": soc, "libelle": "Nouvelle", "serveur": "imap.ovh.net", "identifiant": "x@y", "mot_de_passe": "secret-imap"})), c)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert!(r.json.get("mot_de_passe_chiffre").is_none());
    assert_eq!(r.json["dossiers"], json!(["INBOX"]));
    let id = r.json["id"].as_i64().unwrap();
    // Le mot de passe est chiffré en base.
    let brut: Vec<u8> =
        sqlx::query_scalar("select mot_de_passe_chiffre from boite_mail where id = $1")
            .bind(id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        app.etat.chiffreur.dechiffrer_texte(&brut).unwrap(),
        "secret-imap"
    );

    // Activation refusée sans test réussi.
    assert_eq!(
        app.requete("POST", &format!("/api/admin/boites/{id}/activer"), None, c)
            .await
            .code,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.requete("POST", &format!("/api/admin/boites/{id}/tester"), None, c)
            .await
            .code,
        StatusCode::ACCEPTED
    );
    let n: i64 = sqlx::query_scalar("select count(*) from apalis.jobs where job_type = 'boites'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
    sqlx::query("update boite_mail set dernier_test = '{\"ok\": true}' where id = $1")
        .bind(id)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete("POST", &format!("/api/admin/boites/{id}/activer"), None, c)
        .await;
    assert_eq!(r.json["active"], true);
    assert_eq!(
        app.requete("POST", &format!("/api/admin/boites/{id}/relever"), None, c)
            .await
            .code,
        StatusCode::ACCEPTED
    );

    // Changer le serveur annule le test.
    let r = app
        .requete("PUT", &format!("/api/admin/boites/{id}"), Some(json!({"societe_id": soc, "libelle": "Nouvelle", "serveur": "imap.autre.net", "identifiant": "x@y", "dossiers": ["INBOX", "Factures"]})), c)
        .await;
    assert_eq!(r.json["dernier_test"], json!(null));
    assert_eq!(r.json["dossiers"], json!(["Factures", "INBOX"]));

    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/{}/compteurs", m.boite_a),
            None,
            c,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json[0]["dossier"], "INBOX");
    assert_eq!(r.json[0]["messages_vus_total"], 2);

    let r = app
        .requete("GET", "/api/admin/journal?objet_type=boite_mail", None, c)
        .await;
    assert!(r.json["total"].as_i64().unwrap() >= 4);
    assert_eq!(r.json["elements"][0]["auteur"], "admin");
    let _ = m.mail_a;
    m.app.fermer().await;
}
