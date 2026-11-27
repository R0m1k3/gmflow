//! Sécurité et exploitation (constats C9, C10, C23, C25 de l'audit).

mod commun;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use commun::Appli;
use gmflow_core::fixtures;
use serde_json::{Value, json};
use tower::ServiceExt;

const MDP: &str = "un mot de passe solide";

/// Connexion vue depuis `pair` (adresse de la connexion TCP), avec `X-Real-IP` si donné.
async fn connexion_depuis(
    app: &Appli,
    pair: &str,
    vraie_ip: Option<&str>,
    identifiant: &str,
    mdp: &str,
) -> StatusCode {
    let mut r = Request::builder()
        .method("POST")
        .uri("/api/connexion")
        .header("x-gmflow", "1")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(ip) = vraie_ip {
        r = r.header("x-real-ip", ip);
    }
    let mut req = r
        .body(Body::from(
            json!({"identifiant": identifiant, "mot_de_passe": mdp}).to_string(),
        ))
        .unwrap();
    let pair: SocketAddr = format!("{pair}:40000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(pair));
    app.routeur.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn un_tiers_ne_bloque_pas_le_vrai_utilisateur() {
    let app = Appli::new().await;
    app.utilisateur("michael", MDP, "admin", None).await;
    // Un attaquant derrière Nginx Proxy Manager (réseau Docker) : 10 essais ratés.
    for i in 0..10 {
        let code =
            connexion_depuis(&app, "172.18.0.4", Some("203.0.113.66"), "michael", "faux").await;
        let attendu = if i < 5 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(code, attendu, "essai {i}");
    }
    // Il est freiné, même avec le bon mot de passe…
    assert_eq!(
        connexion_depuis(&app, "172.18.0.4", Some("203.0.113.66"), "michael", MDP).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // … mais le vrai utilisateur, depuis son adresse, se connecte.
    assert_eq!(
        connexion_depuis(&app, "172.18.0.4", Some("198.51.100.20"), "michael", MDP).await,
        StatusCode::OK
    );
    // Sans proxy de confiance (connexion directe depuis Internet), l'en-tête n'est pas cru :
    // changer de `X-Real-IP` ne remet pas les compteurs à zéro.
    for i in 0..6 {
        let ip = format!("192.0.2.{i}");
        let code = connexion_depuis(&app, "198.51.100.99", Some(&ip), "michael", "faux").await;
        let attendu = if i < 5 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(code, attendu, "essai direct {i}");
    }
    // Le journal garde l'adresse des essais refusés.
    let adresse: String = sqlx::query_scalar(
        "select details->>'adresse' from journal where action = 'connexion_refusee' order by id limit 1",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(adresse, "203.0.113.66");
    app.fermer().await;
}

#[tokio::test]
async fn changer_son_mot_de_passe_coupe_les_autres_sessions() {
    let app = Appli::new().await;
    let id = app
        .utilisateur(
            "marie",
            MDP,
            "responsable",
            Some(app.societe("Magasin").await),
        )
        .await;
    let ici = app.connecter("marie", MDP).await;
    let ailleurs = app.connecter("marie", MDP).await;
    let changer = |ancien: &str, nouveau: &str| json!({ "ancien": ancien, "nouveau": nouveau });

    // Ancien mot de passe faux : refusé, sous le champ « ancien ».
    let r = app
        .requete(
            "POST",
            "/api/moi/mot-de-passe",
            Some(changer("pas le bon", "un nouveau mot de passe")),
            Some(&ici),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "ancien");
    // Trop court : mêmes règles qu'à la création.
    let r = app
        .requete(
            "POST",
            "/api/moi/mot-de-passe",
            Some(changer(MDP, "court")),
            Some(&ici),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "nouveau");
    assert!(r.json["erreur"].as_str().unwrap().contains("12 caractères"));

    let r = app
        .requete(
            "POST",
            "/api/moi/mot-de-passe",
            Some(changer(MDP, "un nouveau mot de passe")),
            Some(&ici),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT, "{:?}", r.json);
    // La session en cours continue (nouveau cookie), l'autre est coupée.
    let ici = r.cookie.unwrap_or(ici);
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&ici)).await.code,
        StatusCode::OK
    );
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&ailleurs))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    // Le nouveau mot de passe sert, l'ancien non.
    app.connecter("marie", "un nouveau mot de passe").await;
    let r = app
        .requete(
            "POST",
            "/api/connexion",
            Some(json!({"identifiant": "marie", "mot_de_passe": MDP})),
            None,
        )
        .await;
    assert_eq!(r.code, StatusCode::UNAUTHORIZED);
    // Journalisé (refus et changement), sans le mot de passe.
    let actions: Vec<(String, Value)> = sqlx::query_as(
        "select action, details from journal where objet_id = $1 and action like 'mot_de_passe%' order by id",
    )
    .bind(id.to_string())
    .fetch_all(app.pool())
    .await
    .unwrap();
    let noms: Vec<&str> = actions.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(noms, ["mot_de_passe_refuse", "mot_de_passe_change"]);
    assert!(!actions[1].1.to_string().contains("nouveau mot de passe"));
    app.fermer().await;
}

#[tokio::test]
async fn mot_de_passe_change_par_l_admin_coupe_les_sessions() {
    let app = Appli::new().await;
    let soc = app.societe("Magasin").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let marie = app
        .utilisateur("marie", MDP, "responsable", Some(soc))
        .await;
    let admin = app.connecter("admin", MDP).await;
    let session_marie = app.connecter("marie", MDP).await;
    let modifier = |mdp: Option<&str>| {
        json!({ "identifiant": "marie", "role": "responsable", "societe_id": soc,
                "actif": true, "mot_de_passe": mdp })
    };
    // Modifier sans changer le mot de passe ne coupe rien.
    let r = app
        .requete(
            "PUT",
            &format!("/api/admin/utilisateurs/{marie}"),
            Some(modifier(None)),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&session_marie))
            .await
            .code,
        StatusCode::OK
    );
    let r = app
        .requete(
            "PUT",
            &format!("/api/admin/utilisateurs/{marie}"),
            Some(modifier(Some("un autre mot de passe"))),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&session_marie))
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    // L'admin reste connecté.
    assert_eq!(
        app.requete("GET", "/api/moi", None, Some(&admin))
            .await
            .code,
        StatusCode::OK
    );
    app.fermer().await;
}

#[tokio::test]
async fn apercu_d_un_tableur_piege_refuse_avec_un_message_clair() {
    let app = Appli::new().await;
    let soc = app.societe("Magasin").await;
    let boite = app.boite(soc, "Boîte").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let c = app.connecter("admin", MDP).await;
    let piege = fixtures::xlsx_cellules(&[(0, 0, "BL"), (1_048_575, 16_383, "x")]);
    let eml = fixtures::eml(
        "piege@test",
        "BL",
        &[(
            "bl.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            &piege,
        )],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    let piece = app.piece_de(mail).await;
    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{piece}/tableau"),
            None,
            Some(&c),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let m = r.json["erreur"].as_str().unwrap();
    assert!(m.starts_with("Pièce trop volumineuse"), "{m}");
    // Le fichier reste téléchargeable.
    let r = app
        .requete("GET", &format!("/api/pieces/{piece}"), None, Some(&c))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.octets, piege);
    app.fermer().await;
}

async fn executer(app: &Appli, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(app.pool())
        .await
        .unwrap();
}

/// Le worker vient de battre (planificateur et six files).
async fn worker_qui_bat(app: &Appli) {
    executer(
        app,
        r#"insert into parametre (cle, valeur)
           values ('worker_battement',
                   jsonb_build_object('quand', to_char(now() at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')))
           on conflict (cle) do update set valeur = excluded.valeur"#,
    )
    .await;
    for f in gmflow_core::traitements::FILES {
        executer(
            app,
            &format!(
                "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
                 values ('gmflow-{f}', '{f}', 'PgStorage', '', now())
                 on conflict (id) do update set last_seen = now()"
            ),
        )
        .await;
    }
}

#[tokio::test]
async fn sonde_publique_ok_ou_degradee() {
    let app = Appli::new().await;
    // Worker jamais vu : dégradé, sans session ni détail.
    let r = app.requete("GET", "/api/sante", None, None).await;
    assert_eq!(r.code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json, json!({"etat": "degrade", "causes": ["worker"]}));

    worker_qui_bat(&app).await;
    let soc = app.societe("Magasin").await;
    let boite = app.boite(soc, "Factures Frouard").await;
    executer(
        &app,
        &format!("update boite_mail set active = true, derniere_releve = now() - interval '4 minutes' where id = {boite}"),
    )
    .await;
    let r = app.requete("GET", "/api/sante", None, None).await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["etat"], "ok");

    // Relève en retard (40 min) : dégradé.
    executer(
        &app,
        &format!("update boite_mail set derniere_releve = now() - interval '40 minutes', cree_le = now() - interval '1 day' where id = {boite}"),
    )
    .await;
    let r = app.requete("GET", "/api/sante", None, None).await;
    assert_eq!(r.code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(r.json["causes"], json!(["releve"]));
    assert!(!r.octets.windows(8).any(|w| w == b"Frouard\""));

    // Planificateur arrêté depuis 11 min.
    executer(
        &app,
        r#"update parametre set valeur = jsonb_build_object('quand',
             to_char((now() - interval '11 minutes') at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'))
           where cle = 'worker_battement'"#,
    )
    .await;
    let r = app.requete("GET", "/api/sante", None, None).await;
    assert_eq!(r.json["causes"], json!(["worker", "releve"]));
    app.fermer().await;
}

#[tokio::test]
async fn metriques_reservees_au_jeton_ou_a_l_admin() {
    let app = Appli::new().await;
    let mut etat = app.etat.clone();
    etat.jeton_metriques = Some("jeton-de-prometheus".into());
    let routeur = gmflow_app::routes::routeur(
        etat,
        tower_sessions_sqlx_store::PostgresStore::new(app.pool().clone()),
        None,
    );
    let soc = app.societe("Magasin").await;
    app.boite(soc, "Factures \"Frouard\"").await;
    worker_qui_bat(&app).await;
    executer(
        &app,
        "insert into apalis.jobs (job, id, job_type, status, run_at)
         values ('\\x00', 'j1', 'lecture', 'Pending', now() - interval '90 seconds'),
                ('\\x00', 'j2', 'lecture', 'Failed', now())",
    )
    .await;
    executer(
        &app,
        "update apalis.jobs set attempts = 3, max_attempts = 3 where id = 'j2'",
    )
    .await;
    executer(
        &app,
        "insert into appel_claude (tache, modele, duree_ms, succes, cout_usd) values ('lecture', 'haiku', 1, true, 0.25), ('lecture', 'haiku', 1, false, 0)",
    )
    .await;

    let appel = |jeton: Option<&'static str>, cookie: Option<String>| {
        let routeur = routeur.clone();
        async move {
            let mut r = Request::builder().uri("/api/metriques");
            if let Some(j) = jeton {
                r = r.header(header::AUTHORIZATION, format!("Bearer {j}"));
            }
            if let Some(c) = cookie {
                r = r.header(header::COOKIE, c);
            }
            let rep = routeur
                .oneshot(r.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let code = rep.status();
            let texte = String::from_utf8(
                axum::body::to_bytes(rep.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            (code, texte)
        }
    };
    assert_eq!(appel(None, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(appel(Some("faux"), None).await.0, StatusCode::UNAUTHORIZED);
    app.utilisateur("marie", MDP, "responsable", Some(soc))
        .await;
    let marie = app.connecter("marie", MDP).await;
    assert_eq!(appel(None, Some(marie)).await.0, StatusCode::FORBIDDEN);
    app.utilisateur("admin", MDP, "admin", None).await;
    let admin = app.connecter("admin", MDP).await;
    assert_eq!(appel(None, Some(admin)).await.0, StatusCode::OK);

    let (code, t) = appel(Some("jeton-de-prometheus"), None).await;
    assert_eq!(code, StatusCode::OK);
    for attendu in [
        "gmflow_file_taches{file=\"lecture\",etat=\"en_attente\"} 1",
        "gmflow_file_taches{file=\"lecture\",etat=\"echouees\"} 1",
        "gmflow_file_taches{file=\"envoi\",etat=\"en_attente\"} 0",
        "gmflow_claude_appels_jour{resultat=\"reussi\"} 1",
        "gmflow_claude_appels_jour{resultat=\"echec\"} 1",
        "gmflow_claude_cout_jour_dollars 0.25",
        "boite=\"Factures \\\"Frouard\\\"\"",
        "gmflow_envois_echec_24h 0",
        "# TYPE gmflow_documents gauge",
    ] {
        assert!(t.contains(attendu), "{attendu} absent de :\n{t}");
    }
    let vieille = t
        .lines()
        .find(|l| l.starts_with("gmflow_file_plus_vieille_tache_secondes{file=\"lecture\"}"))
        .unwrap();
    let age: f64 = vieille.rsplit(' ').next().unwrap().parse().unwrap();
    assert!((80.0..300.0).contains(&age), "{vieille}");
    app.fermer().await;
}
