#![allow(dead_code)]

use std::time::Duration;

use gmflow_core::claude::DossierTemp;
use gmflow_core::crypto::Chiffreur;
use gmflow_core::imap::Transport;
use gmflow_core::test_utils::{self, BaseTest, CLE_TEST, MDP_IMAP};
use gmflow_worker::{Contexte, DelaisSeance};

pub struct Banc {
    pub base: BaseTest,
    pub ctx: Contexte,
    pub fichiers: DossierTemp,
}

pub fn fixture(nom: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{nom}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

pub fn faux_claude() -> String {
    format!("{}/tests/faux-claude.sh", env!("CARGO_MANIFEST_DIR"))
}

impl Banc {
    pub async fn new() -> Self {
        Self::avec_delais(DelaisSeance::default()).await
    }

    pub async fn avec_delais(delais: DelaisSeance) -> Self {
        let base = BaseTest::nouvelle().await;
        let fichiers = test_utils::dossier_fichiers();
        let mut config = base.config(fichiers.0.clone());
        config.claude_bin = faux_claude();
        let ctx = Contexte::new(base.pool.clone(), config, Transport::ClairTest, delais);
        Self {
            base,
            ctx,
            fichiers,
        }
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        &self.base.pool
    }

    pub async fn societe(&self, nom: &str) -> i64 {
        sqlx::query_scalar("insert into societe (raison_sociale) values ($1) returning id")
            .bind(nom)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    /// Boîte active sur le serveur de test, qui surveille `dossiers`.
    pub async fn boite(&self, societe_id: i64, compte: &str, mdp: &str, dossiers: &[&str]) -> i64 {
        let (h, p) = test_utils::imap_test();
        let chiffre = Chiffreur::new(&CLE_TEST).chiffrer(mdp.as_bytes());
        let dossiers: Vec<String> = dossiers.iter().map(|d| d.to_string()).collect();
        sqlx::query_scalar(
            "insert into boite_mail (societe_id, libelle, serveur, port, identifiant, mot_de_passe_chiffre, dossiers, active)
             values ($1, $2, $3, $4, $5, $6, $7, true) returning id",
        )
        .bind(societe_id)
        .bind(format!("Boîte {compte}"))
        .bind(h)
        .bind(i32::from(p))
        .bind(compte)
        .bind(chiffre)
        .bind(dossiers)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    pub async fn boite_ok(&self, societe_id: i64, compte: &str, dossiers: &[&str]) -> i64 {
        self.boite(societe_id, compte, MDP_IMAP, dossiers).await
    }

    pub async fn compter(&self, requete: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(requete.to_string()))
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    pub async fn fermer(self) {
        self.base.fermer().await;
    }
}

pub async fn attendre<F, Fut>(max: Duration, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let debut = std::time::Instant::now();
    while debut.elapsed() < max {
        if condition().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Workers apalis sur un fil à part (leur future n'est pas `Send` pour `tokio::spawn`).
pub struct WorkersTest {
    arret: tokio::sync::watch::Sender<bool>,
    fil: Option<std::thread::JoinHandle<()>>,
}

impl WorkersTest {
    pub fn demarrer(ctx: Contexte) -> Self {
        let (arret, rx) = tokio::sync::watch::channel(false);
        let fil = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            // Pool propre à ce runtime : une connexion ouverte ici meurt avec lui, elle ne
            // doit pas revenir dans le pool du test.
            rt.block_on(async move {
                let mut ctx = ctx;
                ctx.pool = gmflow_core::db::connecter(&ctx.config.database_url)
                    .await
                    .unwrap();
                let pool = ctx.pool.clone();
                let _ = gmflow_worker::lancer_files(ctx, rx).await;
                let _ = tokio::time::timeout(Duration::from_secs(5), pool.close()).await;
            });
        });
        Self {
            arret,
            fil: Some(fil),
        }
    }

    pub fn arreter(mut self) {
        let _ = self.arret.send(true);
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}
