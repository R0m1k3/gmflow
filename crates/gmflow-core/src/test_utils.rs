//! Outils de test : une base PostgreSQL jetable par test et l'accès au serveur IMAP de test
//! (Dovecot en local, GreenMail en CI), en clair.
//!
//! Variables : `GMFLOW_TEST_DATABASE_URL` (URL d'une base d'administration, rôle avec CREATEDB)
//! et `GMFLOW_TEST_IMAP` (`hôte:port`, comptes `a@test.local` et `b@test.local`, mot de passe `mdp`).

use std::path::PathBuf;

use sqlx::{Connection, PgConnection, PgPool};

use crate::config::Config;
use crate::imap::{self, Session, Transport};

pub const MDP_IMAP: &str = "mdp";
pub const CLE_TEST: [u8; 32] = [7u8; 32];

pub struct BaseTest {
    pub pool: PgPool,
    pub url: String,
    nom: String,
    admin: String,
}

fn url_admin() -> String {
    std::env::var("GMFLOW_TEST_DATABASE_URL")
        .expect("GMFLOW_TEST_DATABASE_URL manquante (voir CLAUDE.md, section Tests)")
}

fn url_base(admin: &str, nom: &str) -> String {
    let (debut, _) = admin.rsplit_once('/').expect("URL PostgreSQL avec un nom de base");
    format!("{debut}/{nom}")
}

impl BaseTest {
    /// Base vide, sans migrations.
    pub async fn vide() -> Self {
        let admin = url_admin();
        let nom = format!("gmflow_test_{}", uuid::Uuid::new_v4().simple());
        let mut c = PgConnection::connect(&admin).await.expect("connexion à la base d'administration");
        sqlx::query(sqlx::AssertSqlSafe(format!("create database {nom}"))).execute(&mut c).await.expect("create database");
        c.close().await.ok();
        let url = url_base(&admin, &nom);
        let pool = crate::db::connecter(&url).await.expect("connexion à la base de test");
        Self { pool, url, nom, admin }
    }

    /// Base initialisée (migrations GMFlow, apalis, sessions).
    pub async fn nouvelle() -> Self {
        let b = Self::vide().await;
        crate::db::initialiser(&b.pool).await.expect("initialisation");
        b
    }

    pub fn config(&self, fichiers: PathBuf) -> Config {
        Config {
            database_url: self.url.clone(),
            cle: CLE_TEST,
            url_publique: "http://localhost:8080".into(),
            fichiers,
            claude_bin: "claude".into(),
        }
    }

    /// Supprime la base. À appeler en fin de test (une base reste en cas d'échec, pour l'examiner).
    pub async fn fermer(self) {
        self.pool.close().await;
        if let Ok(mut c) = PgConnection::connect(&self.admin).await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("drop database if exists {} with (force)", self.nom)))
                .execute(&mut c)
                .await;
        }
    }
}

/// Dossier temporaire pour les fichiers d'un test.
pub fn dossier_fichiers() -> crate::claude::DossierTemp {
    crate::claude::DossierTemp::creer("gmflow-test-fichiers").expect("dossier temporaire")
}

pub fn imap_test() -> (String, u16) {
    let v = std::env::var("GMFLOW_TEST_IMAP").expect("GMFLOW_TEST_IMAP manquante (hôte:port)");
    let (h, p) = v.rsplit_once(':').expect("GMFLOW_TEST_IMAP = hôte:port");
    (h.to_string(), p.parse().expect("port IMAP"))
}

pub async fn session(compte: &str) -> Session {
    let (h, p) = imap_test();
    imap::connecter(&h, p, compte, MDP_IMAP, Transport::ClairTest)
        .await
        .expect("connexion IMAP de test")
}

/// Nom de dossier unique : les tests tournent en parallèle sur les mêmes comptes.
pub fn dossier_unique() -> String {
    format!("T{}", uuid::Uuid::new_v4().simple())
}

pub async fn creer_dossier(compte: &str, dossier: &str) {
    let mut s = session(compte).await;
    s.create(dossier).await.expect("CREATE");
    s.logout().await.ok();
}

pub async fn supprimer_dossier(compte: &str, dossier: &str) {
    let mut s = session(compte).await;
    s.delete(dossier).await.expect("DELETE");
    s.logout().await.ok();
}

/// Dépose un message dans un dossier (APPEND), à la place d'un envoi SMTP.
pub async fn deposer(compte: &str, dossier: &str, eml: &[u8]) {
    let mut s = session(compte).await;
    s.append(dossier, None, None, eml).await.expect("APPEND");
    s.logout().await.ok();
}

/// Drapeaux de tous les messages d'un dossier.
pub async fn drapeaux(compte: &str, dossier: &str) -> Vec<Vec<String>> {
    use futures::TryStreamExt;
    let mut s = session(compte).await;
    s.examine(dossier).await.expect("EXAMINE");
    let fetches: Vec<_> = s
        .fetch("1:*", "FLAGS")
        .await
        .expect("FETCH")
        .try_collect()
        .await
        .expect("FETCH");
    let v = fetches
        .iter()
        .map(|f| f.flags().map(|d| format!("{d:?}")).collect())
        .collect();
    drop(fetches);
    s.logout().await.ok();
    v
}
