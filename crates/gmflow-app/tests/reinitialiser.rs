mod commun;

use axum::http::StatusCode;
use commun::{Appli, fixture};
use serde_json::json;

const CHEMIN: &str = "/api/admin/reinitialiser";

/// Une société, une boîte active déjà relevée (avec une erreur), un admin, un responsable, des
/// réglages, et des données tirées des mails dans toutes les tables. Rend (app, cookie admin,
/// cookie responsable, boîte).
async fn preparer() -> (Appli, String, String, i64) {
    let app = Appli::new().await;
    let soc = app.societe("Frouard").await;
    app.utilisateur("admin", "mdp-admin-1234", "admin", None)
        .await;
    app.utilisateur("resp", "mdp-resp-1234", "responsable", Some(soc))
        .await;
    let boite = app.boite(soc, "Compta").await;
    let boite_pause = app.boite(soc, "Archives").await;
    sqlx::query(
        "update boite_mail set active = true, derniere_releve = now(), derniere_erreur = 'refus',
             erreur_depuis = now(), alerte_le = now(), echecs_auth = 2,
             releve_jusqu_a = now() - interval '1 minute'
         where id = $1",
    )
    .bind(boite)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query(
        "update boite_mail set active = true, pause_auto = true, echecs_auth = 3 where id = $1",
    )
    .bind(boite_pause)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("insert into boite_dossier (boite_id, nom, uidvalidity, dernier_uid) values ($1, 'INBOX', 7, 42)")
        .bind(boite)
        .execute(app.pool())
        .await
        .unwrap();
    gmflow_core::parametre::ecrire(
        app.pool(),
        "claude_reglages",
        &json!({ "lecture_auto": false }),
    )
    .await
    .unwrap();
    sqlx::query("insert into expediteur_marque (domaine) values ('pub.example')")
        .execute(app.pool())
        .await
        .unwrap();

    let mail = app.mail_vu(&fixture("pdf.eml"), &[boite]).await;
    let piece = app.piece_de(mail).await;
    let fourn: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, code_gestion) values ('Vileda', 'vileda', 'V01') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    let autre: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise) values ('Vileda SAS', 'vileda sas') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    sqlx::query("insert into fournisseur_distinct (a, b) values ($1, $2)")
        .bind(fourn.min(autre))
        .bind(fourn.max(autre))
        .execute(app.pool())
        .await
        .unwrap();
    let doc: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, type, societe_id, fournisseur_id, statut_depot)
         select id, mail_id, sha256, 'lu', 'facture', $2, $3, 'depose' from piece_jointe where id = $1
         returning id",
    )
    .bind(piece)
    .bind(soc)
    .bind(fourn)
    .fetch_one(app.pool())
    .await
    .unwrap();
    sqlx::query(
        "insert into ligne_document (document_id, rang, designation) values ($1, 1, 'Seau')",
    )
    .bind(doc)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("insert into envoi_depot (document_id, societe_id, adresse, etat, quand) values ($1, $2, 'x@dext.cc', 'envoye', now())")
        .bind(doc)
        .bind(soc)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into brouillon_reponse (mail_id, boite_id, statut) values ($1, $2, 'propose')",
    )
    .bind(mail)
    .bind(boite)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("insert into mail_hors_societe (mail_id, societe_id) values ($1, $2)")
        .bind(mail)
        .bind(soc)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query("insert into appel_claude (tache, modele, duree_ms, succes) values ('tri_mail', 'haiku', 10, true)")
        .execute(app.pool())
        .await
        .unwrap();
    gmflow_core::journal::ecrire(app.pool(), None, "creation", "societe", None, json!({}))
        .await
        .unwrap();
    gmflow_core::file::pousser_lecture(app.pool(), doc, false)
        .await
        .unwrap();
    gmflow_core::file::pousser_tri(app.pool(), mail)
        .await
        .unwrap();
    gmflow_core::file::pousser_releve(app.pool(), boite)
        .await
        .unwrap();
    gmflow_core::file::pousser_test_jeton(app.pool())
        .await
        .unwrap();
    gmflow_core::file::pousser_envoi(app.pool(), gmflow_core::file::TacheEnvoi::Alertes)
        .await
        .unwrap();

    let admin = app.connecter("admin", "mdp-admin-1234").await;
    let resp = app.connecter("resp", "mdp-resp-1234").await;
    (app, admin, resp, boite)
}

#[tokio::test]
async fn efface_les_donnees_et_garde_la_configuration() {
    let (app, admin, _resp, boite) = preparer().await;

    let r = app
        .requete(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["mails"], 1);
    assert_eq!(r.json["documents"], 1);
    assert_eq!(r.json["fournisseurs"], 2);
    // Lecture, tri et relève en attente ; le test du jeton et les alertes restent.
    assert_eq!(r.json["taches"], 3);
    // Les deux boîtes actives (la pause automatique est levée).
    assert_eq!(r.json["boites_relancees"], 2);

    for t in gmflow_core::reinitialisation::TABLES_EFFACEES {
        assert_eq!(
            app.compter(&format!("select count(*) from {t}")).await,
            0,
            "{t}"
        );
    }
    // Preuves gardées : l'historique (avec la réinitialisation), les appels à Claude, et les
    // dépôts copiés avec ce qui désigne la pièce.
    assert!(
        app.compter(
            "select count(*) from journal where action = 'creation' and objet_type = 'societe'"
        )
        .await
            >= 1,
        "historique d'avant gardé"
    );
    assert_eq!(app.compter("select count(*) from appel_claude").await, 1);
    let (adresse, etat_envoi, societe, type_doc, fournisseur, fichier): (
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "select adresse, etat, societe_id, document_type, fournisseur_nom, nom_fichier
         from envoi_depot_archive",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        (adresse.as_str(), etat_envoi.as_str(), type_doc.as_deref()),
        ("x@dext.cc", "envoye", Some("facture"))
    );
    assert!(societe > 0);
    assert_eq!(fournisseur.as_deref(), Some("Vileda"));
    assert!(fichier.is_some());
    // Gardés : sociétés, boîtes, utilisateurs, réglages, expéditeurs publicitaires, sessions.
    assert_eq!(app.compter("select count(*) from societe").await, 1);
    assert_eq!(app.compter("select count(*) from boite_mail").await, 2);
    assert_eq!(app.compter("select count(*) from utilisateur").await, 2);
    assert_eq!(
        app.compter("select count(*) from expediteur_marque").await,
        1
    );
    assert_eq!(
        app.compter("select count(*) from parametre where cle = 'claude_reglages'")
            .await,
        1
    );
    let r = app.requete("GET", "/api/moi", None, Some(&admin)).await;
    assert_eq!(r.code, StatusCode::OK, "session gardée");

    // Relève remise à zéro.
    assert_eq!(
        app.compter(
            "select count(*) from boite_mail
             where derniere_releve is null and derniere_erreur is null and erreur_depuis is null
               and alerte_le is null and echecs_auth = 0 and not pause_auto and releve_jusqu_a is null"
        )
        .await,
        2
    );
    // Une relève neuve par boîte, et rien d'autre de ce qui portait sur les données.
    assert_eq!(
        app.compter(&format!(
            "select count(*) from apalis.jobs where job_type = 'boites'
               and convert_from(job, 'UTF8') like '%\"boite_id\":{boite}%'"
        ))
        .await,
        1
    );
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type in ('lecture', 'tri')")
            .await,
        0
    );
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'claude'")
            .await,
        1
    );
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'envoi'")
            .await,
        1
    );

    // Une ligne d'historique : qui, et combien.
    let (action, auteur, details): (String, Option<i64>, serde_json::Value) = sqlx::query_as(
        "select action, auteur_id, details from journal where action = 'reinitialisation'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(action, "reinitialisation");
    assert!(auteur.is_some());
    assert_eq!(details["mails"], 1);
    assert_eq!(details["documents"], 1);
    assert_eq!(details["fournisseurs"], 2);
    assert_eq!(details["envois_archives"], 1);
    app.fermer().await;
}

#[tokio::test]
async fn refuse_sans_confirmation_ou_pour_un_responsable() {
    let (app, admin, resp, _boite) = preparer().await;
    for corps in [
        json!({}),
        json!({"confirmation": "oui"}),
        json!({"confirmation": "réinitialiser"}),
    ] {
        let r = app
            .requete("POST", CHEMIN, Some(corps.clone()), Some(&admin))
            .await;
        assert_eq!(r.code, StatusCode::BAD_REQUEST, "{corps}");
    }
    let r = app
        .requete(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete_brute(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&admin),
            false,
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN, "sans X-GMFlow");
    assert_eq!(app.compter("select count(*) from mail").await, 1);
    assert_eq!(app.compter("select count(*) from document").await, 1);
    assert_eq!(app.compter("select count(*) from fournisseur").await, 2);
    app.fermer().await;
}

#[tokio::test]
async fn refuse_pendant_une_releve_ou_un_envoi() {
    let (app, admin, _resp, boite) = preparer().await;
    sqlx::query(
        "update boite_mail set releve_jusqu_a = now() + interval '10 minutes' where id = $1",
    )
    .bind(boite)
    .execute(app.pool())
    .await
    .unwrap();
    let r = app
        .requete(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert!(
        r.json["erreur"]
            .as_str()
            .unwrap()
            .contains("relève est en cours"),
        "{:?}",
        r.json
    );
    assert_eq!(app.compter("select count(*) from mail").await, 1);
    assert_eq!(app.compter("select count(*) from boite_dossier").await, 1);

    sqlx::query("update boite_mail set releve_jusqu_a = null")
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query("update envoi_depot set etat = 'en_cours', quand = now()")
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert!(
        r.json["erreur"]
            .as_str()
            .unwrap()
            .contains("envoi est en cours"),
        "{:?}",
        r.json
    );
    assert_eq!(app.compter("select count(*) from document").await, 1);

    // Un envoi resté « en cours » depuis longtemps (worker arrêté) ne bloque pas.
    sqlx::query("update envoi_depot set quand = now() - interval '1 hour'")
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            CHEMIN,
            Some(json!({"confirmation": "RÉINITIALISER"})),
            Some(&admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(app.compter("select count(*) from document").await, 0);
    app.fermer().await;
}
