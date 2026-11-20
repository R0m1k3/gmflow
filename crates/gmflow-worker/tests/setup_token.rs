mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, attendre};
use gmflow_core::{file, parametre};
use gmflow_worker::{DelaisSeance, setup_token, test_jeton};
use uuid::Uuid;

async fn seance(banc: &Banc, etat: &str) -> Uuid {
    sqlx::query_scalar("insert into seance_claude (etat) values ($1) returning id")
        .bind(etat)
        .fetch_one(banc.pool())
        .await
        .unwrap()
}

async fn etat_de(
    banc: &Banc,
    id: Uuid,
) -> (String, Option<String>, Option<Vec<u8>>, Option<String>) {
    sqlx::query_as("select etat, url, code_chiffre, message from seance_claude where id = $1")
        .bind(id)
        .fetch_one(banc.pool())
        .await
        .unwrap()
}

async fn coller_code(banc: &Banc, id: Uuid, code: &str) {
    let chiffre = banc.ctx.chiffreur.chiffrer(code.as_bytes());
    sqlx::query("update seance_claude set etat = 'code_recu', code_chiffre = $2 where id = $1 and etat = 'attente_code'")
        .bind(id)
        .bind(chiffre)
        .execute(banc.pool())
        .await
        .unwrap();
}

async fn attendre_url(banc: &Banc, id: Uuid) -> String {
    assert!(
        attendre(Duration::from_secs(10), || async {
            etat_de(banc, id).await.0 == "attente_code"
        })
        .await
    );
    etat_de(banc, id).await.1.unwrap()
}

#[tokio::test]
async fn seance_complete() {
    let banc = Banc::new().await;
    let id = seance(&banc, "demarrage").await;
    let ctx = banc.ctx.clone();
    let tache = tokio::spawn(async move { setup_token::executer(&ctx, id).await });

    let url = attendre_url(&banc, id).await;
    assert_eq!(
        url,
        "https://claude.ai/oauth/authorize?code=true&state=abc123"
    );
    coller_code(&banc, id, "code-de-test").await;
    tache.await.unwrap().unwrap();

    let (etat, _, code, message) = etat_de(&banc, id).await;
    assert_eq!(etat, "terminee", "{message:?}");
    assert!(code.is_none());
    let chiffre = parametre::lire_secret(banc.pool(), parametre::CLAUDE_JETON)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        banc.ctx.chiffreur.dechiffrer_texte(&chiffre).unwrap(),
        "sk-ant-oat01-test_123"
    );
    let etat = parametre::lire(banc.pool(), parametre::CLAUDE_ETAT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(etat["fin"], "_123");
    // Le test du jeton est poussé.
    assert_eq!(
        banc.compter("select count(*) from apalis.jobs where job_type = 'claude'")
            .await,
        1
    );

    // Une séance déjà finie n'est pas relancée.
    setup_token::executer(&banc.ctx, id).await.unwrap();
    assert_eq!(etat_de(&banc, id).await.0, "terminee");
    banc.fermer().await;
}

#[tokio::test]
async fn sans_code_expiree() {
    let banc = Banc::avec_delais(DelaisSeance {
        code: Duration::from_secs(2),
        ..Default::default()
    })
    .await;
    let id = seance(&banc, "demarrage").await;
    setup_token::executer(&banc.ctx, id).await.unwrap();
    let (etat, url, code, _) = etat_de(&banc, id).await;
    assert_eq!(etat, "expiree");
    assert!(url.is_some() && code.is_none());
    banc.fermer().await;
}

#[tokio::test]
async fn redemarrage_du_worker() {
    let banc = Banc::new().await;
    let id = seance(&banc, "attente_code").await;
    setup_token::fermer_seances_ouvertes(banc.pool())
        .await
        .unwrap();
    let (etat, _, _, message) = etat_de(&banc, id).await;
    assert_eq!(etat, "erreur");
    assert_eq!(message.as_deref(), Some("worker redémarré"));
    banc.fermer().await;
}

#[tokio::test]
async fn echec_sans_second_essai() {
    let banc = Banc::new().await;
    let id = seance(&banc, "demarrage").await;
    file::pousser_seance(banc.pool(), id).await.unwrap();
    let worker = WorkersTest::demarrer(banc.ctx.clone());

    attendre_url(&banc, id).await;
    coller_code(&banc, id, "mauvais").await;
    assert!(
        attendre(Duration::from_secs(15), || async {
            etat_de(&banc, id).await.0 == "erreur"
        })
        .await
    );
    let (_, _, code, message) = etat_de(&banc, id).await;
    assert!(code.is_none());
    assert!(message.unwrap().contains("sans donner de jeton"));
    assert!(
        attendre(Duration::from_secs(10), || async {
            banc.compter(
                "select count(*) from apalis.jobs where job_type = 'claude' and status = 'Failed'",
            )
            .await
                == 1
        })
        .await
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        banc.compter("select max(attempts)::bigint from apalis.jobs where job_type = 'claude'")
            .await,
        1
    );
    worker.arreter();
    banc.fermer().await;
}

#[tokio::test]
async fn test_du_jeton() {
    let banc = Banc::new().await;
    // Sans jeton : absent.
    test_jeton::tester(&banc.ctx).await.unwrap();
    assert_eq!(
        parametre::lire(banc.pool(), parametre::CLAUDE_ETAT)
            .await
            .unwrap()
            .unwrap()["etat"],
        "absent"
    );

    parametre::enregistrer_jeton_claude(
        banc.pool(),
        &banc.ctx.chiffreur,
        "sk-ant-oat01-bon-jeton-abcd",
    )
    .await
    .unwrap();
    test_jeton::tester(&banc.ctx).await.unwrap();
    let e = parametre::lire(banc.pool(), parametre::CLAUDE_ETAT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e["etat"], "valide");
    assert_eq!(e["fin"], "abcd");
    assert_eq!(banc.compter("select count(*) from appel_claude where succes and tache = 'test_jeton' and modele = 'haiku'").await, 1);

    // Jeton refusé : erreur, et apalis fait un seul nouvel essai.
    parametre::enregistrer_jeton_claude(
        banc.pool(),
        &banc.ctx.chiffreur,
        "sk-ant-oat01-mauvais-jeton",
    )
    .await
    .unwrap();
    file::pousser_test_jeton(banc.pool()).await.unwrap();
    let worker = WorkersTest::demarrer(banc.ctx.clone());
    assert!(attendre(Duration::from_secs(15), || async {
        banc.compter("select count(*) from apalis.jobs where job_type = 'claude' and status = 'Failed' and attempts = 2").await == 1
    })
    .await);
    worker.arreter();
    let e = parametre::lire(banc.pool(), parametre::CLAUDE_ETAT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e["etat"], "erreur");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("Invalid bearer token"),
        "{e}"
    );
    assert_eq!(
        banc.compter("select count(*) from appel_claude where not succes")
            .await,
        2
    );
    banc.fermer().await;
}

#[tokio::test]
async fn appel_claude_arguments() {
    let banc = Banc::new().await;
    let s = gmflow_core::claude::appeler(
        banc.pool(),
        &banc.ctx.config.claude_bin,
        "essai",
        "sonnet",
        Some("high"),
        "Bonjour",
        &["--json-schema", "{}"],
        "sk-ant-oat01-x",
    )
    .await
    .unwrap();
    let args = s.json["args"].as_str().unwrap();
    assert!(args.starts_with("-p Bonjour --model sonnet --effort high --output-format json --no-session-persistence --json-schema {}"), "{args}");
    assert!(!args.contains("--bare"));
    assert_eq!(s.json["autoupdate"], "1");
    let home = s.json["home"].as_str().unwrap();
    assert!(home.contains("gmflow-claude-"));
    assert!(
        !std::path::Path::new(home).exists(),
        "HOME temporaire supprimé"
    );
    banc.fermer().await;
}
