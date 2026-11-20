mod commun;

use commun::{Banc, fixture};
use gmflow_core::test_utils::{self as tu};
use gmflow_worker::{planif, releve, sante};

const A: &str = "a@test.local";
const B: &str = "b@test.local";

#[tokio::test]
async fn releve_simple_puis_increment() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml", "html-seul.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;

    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert!(b.bail_obtenu);
    assert_eq!((b.messages_lus, b.nouveaux_mails), (3, 3));
    assert_eq!(banc.compter("select count(*) from mail").await, 3);
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 3);
    assert_eq!(banc.compter("select count(*) from piece_jointe").await, 1);
    assert_eq!(banc.compter("select count(*) from boite_mail where derniere_releve is not null and releve_jusqu_a is null").await, 1);

    // Le .eml est sur disque, avec le bon SHA-256.
    let (chemin, sha): (String, Vec<u8>) =
        sqlx::query_as("select chemin_eml, sha256 from mail limit 1")
            .fetch_one(banc.pool())
            .await
            .unwrap();
    let octets = std::fs::read(banc.fichiers.0.join(chemin)).unwrap();
    assert_eq!(gmflow_core::eml::sha256(&octets), sha);

    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!((b.messages_lus, b.nouveaux_mails), (0, 0));
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 3);

    tu::deposer(A, &d, &fixture("rfc2231.eml")).await;
    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!((b.messages_lus, b.nouveaux_mails), (1, 1));
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 4);

    // Rien n'est marqué comme lu.
    let drapeaux = tu::drapeaux(A, &d).await;
    assert_eq!(drapeaux.len(), 4);
    assert!(
        drapeaux
            .iter()
            .all(|f| !f.iter().any(|x| x.contains("Seen"))),
        "{drapeaux:?}"
    );

    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}

#[tokio::test]
async fn uidvalidity_change() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml", "html-seul.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;
    releve::relever(&banc.ctx, boite).await.unwrap();
    let avant: i64 = sqlx::query_scalar("select uidvalidity from boite_dossier")
        .fetch_one(banc.pool())
        .await
        .unwrap();

    tu::supprimer_dossier(A, &d).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    let apres: i64 = sqlx::query_scalar("select uidvalidity from boite_dossier")
        .fetch_one(banc.pool())
        .await
        .unwrap();
    assert_ne!(
        avant, apres,
        "le serveur de test n'a pas changé d'UIDVALIDITY"
    );
    assert_eq!(b.nouveaux_mails, 0);
    assert_eq!(banc.compter("select count(*) from mail").await, 3);
    assert_eq!(
        banc.compter("select count(*) from mail_boite where uid is not null")
            .await,
        2
    );
    assert_eq!(
        banc.compter("select count(*) from mail_boite where uid is null")
            .await,
        1
    );

    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}

#[tokio::test]
async fn meme_mail_deux_structures() {
    let banc = Banc::new().await;
    let (da, db) = (tu::dossier_unique(), tu::dossier_unique());
    tu::creer_dossier(A, &da).await;
    tu::creer_dossier(B, &db).await;
    tu::deposer(A, &da, &fixture("pdf.eml")).await;
    tu::deposer(B, &db, &fixture("meme-mail-boite2.eml")).await;
    let s1 = banc.societe("Frouard").await;
    let s2 = banc.societe("Houdemont").await;
    let b1 = banc.boite_ok(s1, A, &[&da]).await;
    let b2 = banc.boite_ok(s2, B, &[&db]).await;
    releve::relever(&banc.ctx, b1).await.unwrap();
    releve::relever(&banc.ctx, b2).await.unwrap();
    assert_eq!(banc.compter("select count(*) from mail").await, 1);
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 2);
    tu::supprimer_dossier(A, &da).await;
    tu::supprimer_dossier(B, &db).await;
    banc.fermer().await;
}

#[tokio::test]
async fn message_id_reutilise_et_sans_id() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    tu::deposer(A, &d, &fixture("meme-id-a.eml")).await;
    tu::deposer(A, &d, &fixture("meme-id-b.eml")).await;
    tu::deposer(A, &d, &fixture("sans-message-id.eml")).await;
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;
    releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!(banc.compter("select count(*) from mail").await, 3);
    assert_eq!(
        banc.compter("select count(*) from journal where action = 'message_id_reutilise'")
            .await,
        1
    );
    assert_eq!(
        banc.compter("select count(*) from mail where message_id like 'sha256:%'")
            .await,
        1
    );

    // Relève suivante après remise à zéro du dossier : aucun mail en double.
    sqlx::query("update boite_dossier set dernier_uid = 0")
        .execute(banc.pool())
        .await
        .unwrap();
    releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!(banc.compter("select count(*) from mail").await, 3);
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 3);
    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}

#[tokio::test]
async fn pause_apres_trois_refus() {
    let banc = Banc::new().await;
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite(soc, A, "faux", &["INBOX"]).await;
    for _ in 0..3 {
        assert!(releve::relever(&banc.ctx, boite).await.is_err());
    }
    let (echecs, pause, erreur): (i32, bool, Option<String>) = sqlx::query_as(
        "select echecs_auth, pause_auto, derniere_erreur from boite_mail where id = $1",
    )
    .bind(boite)
    .fetch_one(banc.pool())
    .await
    .unwrap();
    assert_eq!((echecs, pause), (3, true));
    assert!(erreur.unwrap().contains("authentification"));
    // Le planificateur ne pousse plus de relève pour cette boîte.
    assert_eq!(planif::tour(banc.pool()).await.unwrap(), 0);
    // Une relève poussée à la main ne fait rien non plus.
    assert!(!releve::relever(&banc.ctx, boite).await.unwrap().bail_obtenu);
    banc.fermer().await;
}

#[tokio::test]
async fn bail_unique() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml", "html-seul.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;
    let (r1, r2) = tokio::join!(
        releve::relever(&banc.ctx, boite),
        releve::relever(&banc.ctx, boite)
    );
    let (r1, r2) = (r1.unwrap(), r2.unwrap());
    assert_eq!(r1.messages_lus + r2.messages_lus, 3);
    assert!(r1.bail_obtenu != r2.bail_obtenu);

    // Bail resté pris (worker arrêté net) puis expiré : repris.
    sqlx::query("update boite_mail set releve_jusqu_a = now() + interval '10 minutes'")
        .execute(banc.pool())
        .await
        .unwrap();
    assert!(!releve::relever(&banc.ctx, boite).await.unwrap().bail_obtenu);
    sqlx::query("update boite_mail set releve_jusqu_a = now() - interval '1 minute'")
        .execute(banc.pool())
        .await
        .unwrap();
    assert!(releve::relever(&banc.ctx, boite).await.unwrap().bail_obtenu);
    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}

#[tokio::test]
async fn test_de_boite() {
    let banc = Banc::new().await;
    let soc = banc.societe("Frouard").await;
    let bonne = banc.boite_ok(soc, A, &["INBOX"]).await;
    let mauvaise = banc.boite(soc, A, "faux", &["INBOX"]).await;
    gmflow_worker::test_boite::tester(&banc.ctx, bonne)
        .await
        .unwrap();
    gmflow_worker::test_boite::tester(&banc.ctx, mauvaise)
        .await
        .unwrap();
    let t: serde_json::Value =
        sqlx::query_scalar("select dernier_test from boite_mail where id = $1")
            .bind(bonne)
            .fetch_one(banc.pool())
            .await
            .unwrap();
    assert_eq!(t["ok"], true);
    assert!(
        t["dossiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == "INBOX")
    );
    let t: serde_json::Value =
        sqlx::query_scalar("select dernier_test from boite_mail where id = $1")
            .bind(mauvaise)
            .fetch_one(banc.pool())
            .await
            .unwrap();
    assert_eq!(t["ok"], false);
    // Un test raté ne compte pas comme refus de relève.
    assert_eq!(
        banc.compter(&format!(
            "select echecs_auth::bigint from boite_mail where id = {mauvaise}"
        ))
        .await,
        0
    );
    banc.fermer().await;
}

#[tokio::test]
async fn sante_et_battement() {
    let banc = Banc::new().await;
    assert!(!sante::controle(banc.pool()).await);
    planif::tour(banc.pool()).await.unwrap();
    assert!(sante::controle(banc.pool()).await);
    let vieux = (time::OffsetDateTime::now_utc() - time::Duration::minutes(11))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    gmflow_core::parametre::ecrire(
        banc.pool(),
        gmflow_core::parametre::WORKER_BATTEMENT,
        &serde_json::json!({ "quand": vieux }),
    )
    .await
    .unwrap();
    assert!(!sante::controle(banc.pool()).await);

    use tower::ServiceExt;
    let rep = sante::routeur(banc.pool().clone())
        .oneshot(
            axum::http::Request::get("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rep.status(), 503);
    banc.fermer().await;
}

#[test]
fn date_reprise() {
    let d = releve::date_reprise(3);
    assert!(d.split('-').count() == 3, "{d}");
}
