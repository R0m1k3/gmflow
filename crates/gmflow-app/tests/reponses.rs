//! Lot 5 côté API : file « à répondre », catégories, brouillons et clic d'envoi, expéditeurs
//! marqués, clé API Claude. Droits : un responsable ne voit que sa structure.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

fn eml(id: &str, de: &str, objet: &str) -> Vec<u8> {
    format!(
        "From: {de}\r\nTo: compta@frouard.test\r\nSubject: {objet}\r\nMessage-ID: <{id}>\r\n\
         MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nUne question ?\r\n"
    )
    .into_bytes()
}

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp: String,
    boite_a: i64,
    mail_a: i64,
    mail_a2: i64,
    mail_b: i64,
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
    let mail_a = app
        .mail_vu(
            &eml("a@x.test", "jean@client.test", "Livraison"),
            &[boite_a],
        )
        .await;
    let mail_a2 = app
        .mail_vu(
            &eml("a2@x.test", "paul@client.test", "Urgent : retour"),
            &[boite_a],
        )
        .await;
    let mail_b = app
        .mail_vu(&eml("b@x.test", "luc@autre.test", "Devis"), &[boite_b])
        .await;
    for (id, prio) in [(mail_a, "normale"), (mail_a2, "haute"), (mail_b, "basse")] {
        sqlx::query(
            "update mail set categorie = 'question', categorie_par = 'claude', priorite = $2,
                resume = 'Résumé', statut_reponse = 'a_repondre', tri_etat = 'trie' where id = $1",
        )
        .bind(id)
        .bind(prio)
        .execute(app.pool())
        .await
        .unwrap();
    }
    let c_admin = app.connecter("admin", MDP).await;
    let c_resp = app.connecter("resp-a", MDP).await;
    Monde {
        app,
        c_admin,
        c_resp,
        boite_a,
        mail_a,
        mail_a2,
        mail_b,
    }
}

/// (id, catégorie, posée par, statut de réponse)
type Classe = (i64, Option<String>, Option<String>, Option<String>);

fn ids(v: &serde_json::Value) -> Vec<i64> {
    v["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn file_a_repondre_et_droits() {
    let m = monde().await;
    let a = &m.app;
    let r = a
        .requete("GET", "/api/a-repondre", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    // Priorité haute d'abord.
    assert_eq!(ids(&r.json), [m.mail_a2, m.mail_a, m.mail_b]);
    let r = a
        .requete("GET", "/api/a-repondre", None, Some(&m.c_resp))
        .await;
    assert_eq!(ids(&r.json), [m.mail_a2, m.mail_a]);
    let r = a
        .requete(
            "GET",
            "/api/a-repondre?priorite=haute",
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(ids(&r.json), [m.mail_a2]);
    let r = a
        .requete("GET", "/api/a-repondre?priorite=x", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let r = a
        .requete("GET", "/api/a-repondre/compte", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.json["n"], 2);
    let r = a
        .requete("GET", "/api/tableau-de-bord", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.json["a_repondre"], 2);
    assert!(r.json.get("tri").is_none());
    let r = a
        .requete("GET", "/api/tri/semaines", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json.as_array().unwrap().len(), 2);

    // Mail d'une autre structure : invisible pour le responsable.
    for (methode, chemin, corps) in [
        (
            "PUT",
            format!("/api/mails/{}/categorie", m.mail_b),
            json!({ "categorie": "information" }),
        ),
        (
            "POST",
            format!("/api/mails/{}/statut-reponse", m.mail_b),
            json!({ "statut": "sans_suite" }),
        ),
        (
            "POST",
            format!("/api/mails/{}/brouillons", m.mail_b),
            json!({}),
        ),
    ] {
        let r = a
            .requete(methode, &chemin, Some(corps), Some(&m.c_resp))
            .await;
        assert_eq!(r.code, StatusCode::NOT_FOUND, "{chemin}");
    }

    // Catégorie changée à la main : sort de la file, figée.
    let r = a
        .requete(
            "PUT",
            &format!("/api/mails/{}/categorie", m.mail_a),
            Some(json!({ "categorie": "spam" })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let r = a
        .requete(
            "PUT",
            &format!("/api/mails/{}/categorie", m.mail_a),
            Some(json!({ "categorie": "information" })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = a
        .requete(
            "GET",
            &format!("/api/mails/{}", m.mail_a),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.json["categorie"], "information");
    assert_eq!(r.json["categorie_par"], "humain");
    assert!(r.json["statut_reponse"].is_null());
    let r = a
        .requete(
            "GET",
            "/api/mails?categorie=question",
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(ids(&r.json), [m.mail_a2]);
    let r = a
        .requete("GET", "/api/mails?categorie=x", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);

    // Sans suite.
    let r = a
        .requete(
            "POST",
            &format!("/api/mails/{}/statut-reponse", m.mail_a2),
            Some(json!({ "statut": "sans_suite" })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = a
        .requete("GET", "/api/a-repondre/compte", None, Some(&m.c_resp))
        .await;
    assert_eq!(r.json["n"], 0);
    assert_eq!(
        a.compter("select count(*) from journal where action in ('categorie', 'statut_reponse')")
            .await,
        2
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn brouillon_envoye_seulement_au_clic() {
    let m = monde().await;
    let a = &m.app;
    let r = a
        .requete(
            "POST",
            &format!("/api/mails/{}/brouillons", m.mail_a),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{:?}", r.json);
    let id = r.json["id"].as_i64().unwrap();
    // Déjà en préparation : refusé.
    let r = a
        .requete(
            "POST",
            &format!("/api/mails/{}/brouillons", m.mail_a),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    // Pas encore généré : ni modifiable ni envoyable.
    let r = a
        .requete(
            "POST",
            &format!("/api/brouillons/{id}/envoyer"),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);

    // Le worker l'a généré.
    sqlx::query(
        "update brouillon_reponse set statut = 'propose', texte = 'Bonjour, jeudi.' where id = $1",
    )
    .bind(id)
    .execute(a.pool())
    .await
    .unwrap();
    let r = a
        .requete(
            "GET",
            &format!("/api/mails/{}", m.mail_a),
            None,
            Some(&m.c_resp),
        )
        .await;
    let b = &r.json["brouillons"][0];
    assert_eq!(b["statut"], "propose");
    assert_eq!(b["smtp"], false);
    let r = a
        .requete(
            "PUT",
            &format!("/api/brouillons/{id}"),
            Some(json!({ "texte": "Bonjour, livraison jeudi." })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);

    // SMTP absent : refusé avant toute mise en file.
    let r = a
        .requete(
            "POST",
            &format!("/api/brouillons/{id}/envoyer"),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert_eq!(
        a.compter("select count(*) from journal where action = 'reponse_demandee'")
            .await,
        0
    );
    sqlx::query("update boite_mail set smtp_serveur = 'smtp.test', smtp_port = 465, smtp_securite = 'tls' where id = $1")
        .bind(m.boite_a)
        .execute(a.pool())
        .await
        .unwrap();
    // Brouillon d'une autre structure : invisible pour le responsable.
    let autre: i64 = sqlx::query_scalar(
        "insert into brouillon_reponse (mail_id, boite_id, statut, texte)
         select $1, boite_id, 'propose', 'x' from mail_boite where mail_id = $1 returning id",
    )
    .bind(m.mail_b)
    .fetch_one(a.pool())
    .await
    .unwrap();
    let r = a
        .requete(
            "POST",
            &format!("/api/brouillons/{autre}/envoyer"),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);

    let r = a
        .requete(
            "POST",
            &format!("/api/brouillons/{id}/envoyer"),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    let statut: (String, String) =
        sqlx::query_as("select statut, texte from brouillon_reponse where id = $1")
            .bind(id)
            .fetch_one(a.pool())
            .await
            .unwrap();
    assert_eq!(statut, ("envoi".into(), "Bonjour, livraison jeudi.".into()));
    assert_eq!(
        a.compter("select count(*) from journal where action = 'reponse_demandee'")
            .await,
        1
    );
    // En cours d'envoi : figé.
    let r = a
        .requete(
            "PUT",
            &format!("/api/brouillons/{id}"),
            Some(json!({ "texte": "autre" })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    let r = a
        .requete(
            "POST",
            &format!("/api/brouillons/{id}/abandonner"),
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    m.app.fermer().await;
}

#[tokio::test]
async fn expediteurs_marques_publicite() {
    let m = monde().await;
    let a = &m.app;
    let promo = a
        .mail_vu(
            &eml("p1@x.test", "offres@news.client.test", "Promo"),
            &[m.boite_a],
        )
        .await;
    let r = a
        .requete(
            "GET",
            "/api/admin/expediteurs-marques",
            None,
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    // Une catégorie fixée à la main n'est pas reclassée.
    sqlx::query("update mail set categorie_par = 'humain' where id = $1")
        .bind(m.mail_a2)
        .execute(a.pool())
        .await
        .unwrap();
    let r = a
        .requete(
            "POST",
            "/api/admin/expediteurs-marques",
            Some(json!({ "mail_id": m.mail_a, "portee": "domaine" })),
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    // mail_a (client.test) et promo (sous-domaine news.client.test), pas mail_a2 (humain).
    assert_eq!(r.json["reclasses"], 2);
    let cat: Vec<Classe> = sqlx::query_as(
        "select id, categorie, categorie_par, statut_reponse from mail where id = any($1) order by id",
    )
    .bind(vec![m.mail_a, m.mail_a2, promo])
    .fetch_all(a.pool())
    .await
    .unwrap();
    assert_eq!(
        cat,
        [
            (
                m.mail_a,
                Some("publicite".into()),
                Some("humain".into()),
                None
            ),
            (
                m.mail_a2,
                Some("question".into()),
                Some("humain".into()),
                Some("a_repondre".into())
            ),
            (promo, Some("publicite".into()), Some("regle".into()), None),
        ]
    );
    let r = a
        .requete(
            "GET",
            "/api/admin/expediteurs-marques",
            None,
            Some(&m.c_admin),
        )
        .await;
    let liste = r.json.as_array().unwrap();
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0]["domaine"], "client.test");
    // Même domaine une deuxième fois : pas de doublon.
    a.requete(
        "POST",
        "/api/admin/expediteurs-marques",
        Some(json!({ "mail_id": m.mail_a2, "portee": "domaine" })),
        Some(&m.c_admin),
    )
    .await;
    assert_eq!(a.compter("select count(*) from expediteur_marque").await, 1);
    let id = liste[0]["id"].as_i64().unwrap();
    let r = a
        .requete(
            "DELETE",
            &format!("/api/admin/expediteurs-marques/{id}"),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    assert_eq!(a.compter("select count(*) from expediteur_marque").await, 0);
    m.app.fermer().await;
}

#[tokio::test]
async fn cle_api_claude() {
    let m = monde().await;
    let a = &m.app;
    const CLE: &str = "sk-ant-api03-cle_de_test_l5_XYZW";
    let r = a
        .requete(
            "PUT",
            "/api/admin/claude/cle-api",
            Some(json!({ "cle": "pas une clé" })),
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let r = a
        .requete(
            "PUT",
            "/api/admin/claude/cle-api",
            Some(json!({ "cle": CLE })),
            Some(&m.c_resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    // Mode clé API sans clé : refusé.
    let r = a
        .requete(
            "PUT",
            "/api/admin/claude/acces",
            Some(json!({ "mode": "cle_api" })),
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    let r = a
        .requete(
            "PUT",
            "/api/admin/claude/cle-api",
            Some(json!({ "cle": CLE })),
            Some(&m.c_admin),
        )
        .await;
    assert!(r.code.is_success());
    let r = a
        .requete(
            "PUT",
            "/api/admin/claude/acces",
            Some(json!({ "mode": "cle_api" })),
            Some(&m.c_admin),
        )
        .await;
    assert!(r.code.is_success(), "{:?}", r.json);
    let r = a
        .requete("GET", "/api/admin/claude", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.json["acces"]["mode"], "cle_api");
    assert_eq!(r.json["acces"]["cle_api"], true);
    assert_eq!(r.json["acces"]["cle_api_fin"], "XYZW");
    assert!(!String::from_utf8_lossy(&r.octets).contains("cle_de_test"));
    // Clé stockée chiffrée.
    assert_eq!(
        a.compter("select count(*) from parametre where valeur_chiffree is not null and position(convert_to('cle_de_test', 'UTF8') in valeur_chiffree) > 0")
            .await,
        0
    );
    // Suppression : retour au setup token.
    let r = a
        .requete(
            "DELETE",
            "/api/admin/claude/cle-api",
            None,
            Some(&m.c_admin),
        )
        .await;
    assert!(r.code.is_success());
    let r = a
        .requete("GET", "/api/admin/claude", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.json["acces"]["mode"], "jeton");
    assert_eq!(r.json["acces"]["cle_api"], false);
    m.app.fermer().await;
}
