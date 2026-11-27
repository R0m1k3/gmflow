#![allow(dead_code)]

use std::time::Duration;

use gmflow_core::claude::DossierTemp;
use gmflow_core::crypto::Chiffreur;
use gmflow_core::imap::Transport;
use gmflow_core::test_utils::{self, BaseTest, CLE_TEST, MDP_IMAP};
use gmflow_core::{eml, stockage};
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

/// Dépose un mail (fichier `.eml`, lignes `mail`, `piece_jointe`, `mail_boite`) reçu par une
/// boîte de `societe_id`. Rend l'id du mail.
pub async fn mail(b: &Banc, societe_id: i64, brut: &[u8]) -> i64 {
    let boite: i64 = sqlx::query_scalar(
        "insert into boite_mail (societe_id, libelle, serveur, identifiant, mot_de_passe_chiffre)
         values ($1, 'Boîte', 'imap.test', 'x', '\\x00') returning id",
    )
    .bind(societe_id)
    .fetch_one(b.pool())
    .await
    .unwrap();
    let (chemin, sha) = stockage::ecrire_eml(&b.ctx.config.fichiers, brut).unwrap();
    let d = eml::decouper(brut).unwrap();
    let id: i64 = sqlx::query_scalar(
        "insert into mail (message_id, empreinte_contenu, expediteur, destinataires, objet, date_mail,
                           corps_texte, chemin_eml, sha256, taille)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning id",
    )
    .bind(&d.message_id)
    .bind(&d.empreinte_contenu)
    .bind(&d.expediteur)
    .bind(&d.destinataires)
    .bind(&d.objet)
    .bind(d.date)
    .bind(&d.corps_texte)
    .bind(&chemin)
    .bind(&sha)
    .bind(brut.len() as i64)
    .fetch_one(b.pool())
    .await
    .unwrap();
    for p in &d.pieces {
        sqlx::query("insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256) values ($1, $2, $3, $4, $5, $6)")
            .bind(id)
            .bind(p.rang)
            .bind(&p.nom)
            .bind(&p.type_mime)
            .bind(p.taille)
            .bind(&p.sha256)
            .execute(b.pool())
            .await
            .unwrap();
    }
    sqlx::query(
        "insert into mail_boite (mail_id, boite_id, dossier, uid) values ($1, $2, 'INBOX', $1)",
    )
    .bind(id)
    .bind(boite)
    .execute(b.pool())
    .await
    .unwrap();
    id
}

/// Pompe, puis attente que plus aucun document de `mail_ids` ne soit à lire, en cours, ou lu
/// sans ses contrôles de dépôt (fait juste après la lecture).
pub async fn lire_tout(b: &Banc, mail_ids: &[i64]) {
    gmflow_worker::lecture::pompe(b.pool()).await.unwrap();
    let pool = b.pool().clone();
    let ids = mail_ids.to_vec();
    // `a_verifier` sans contrôle : état transitoire de `depot::evaluer` (écrit avant la
    // recherche de doublon), l'évaluation n'est pas finie.
    let fini = attendre(Duration::from_secs(30), || {
        let (pool, ids) = (pool.clone(), ids.clone());
        async move {
            let n: i64 = sqlx::query_scalar(
                "select count(*) from document where mail_id = any($1) and (etat = 'en_cours' or (etat = 'a_lire' and planifie_le is not null and message is null)
                   or (etat = 'lu' and type in ('facture', 'avoir')
                       and (statut_depot is null or (statut_depot = 'a_verifier' and controles = '[]'))))",
            )
            .bind(&ids)
            .fetch_one(&pool)
            .await
            .unwrap();
            n == 0
        }
    })
    .await;
    assert!(fini, "lectures non terminées");
}

/// Un serveur qui redirige tout (HTTP 307) vers un second, qui compte les appels qu'il reçoit
/// avec une clé `X-API-Key`. Rend (adresse du premier, compteur du second).
pub async fn redirecteur() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let recus = Arc::new(AtomicUsize::new(0));
    let r = recus.clone();
    let cible = axum::Router::new().fallback(move |h: axum::http::HeaderMap| {
        let r = r.clone();
        async move {
            if h.contains_key("x-api-key") {
                r.fetch_add(1, Ordering::SeqCst);
            }
            axum::Json(serde_json::json!({ "total": 1, "data": [] }))
        }
    });
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let vers = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, cible).await.unwrap() });
    let source = axum::Router::new().fallback(move |uri: axum::http::Uri| {
        let vers = vers.clone();
        async move { axum::response::Redirect::temporary(&format!("{vers}{uri}")) }
    });
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, source).await.unwrap() });
    (adresse, recus)
}
