#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use gmflow_app::{DureesSession, Etat, routes};
use gmflow_core::claude::DossierTemp;
use gmflow_core::test_utils::{self, BaseTest};
use serde_json::Value;
use tower::ServiceExt;
use tower_sessions_sqlx_store::PostgresStore;

pub struct Appli {
    pub base: BaseTest,
    pub etat: Etat,
    pub routeur: Router,
    pub fichiers: DossierTemp,
}

pub struct Reponse {
    pub code: StatusCode,
    pub json: Value,
    pub octets: Vec<u8>,
    pub cookie: Option<String>,
    pub entetes: axum::http::HeaderMap,
}

impl Appli {
    pub async fn new() -> Self {
        Self::avec_durees(DureesSession::default()).await
    }

    pub async fn avec_durees(durees: DureesSession) -> Self {
        let base = BaseTest::nouvelle().await;
        let fichiers = test_utils::dossier_fichiers();
        let etat = Etat::new(base.pool.clone(), base.config(fichiers.0.clone()), durees);
        let routeur = routes::routeur(etat.clone(), PostgresStore::new(base.pool.clone()), None);
        Self { base, etat, routeur, fichiers }
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        &self.base.pool
    }

    pub async fn requete(&self, methode: &str, chemin: &str, corps: Option<Value>, cookie: Option<&str>) -> Reponse {
        self.requete_brute(methode, chemin, corps, cookie, true).await
    }

    pub async fn requete_brute(
        &self,
        methode: &str,
        chemin: &str,
        corps: Option<Value>,
        cookie: Option<&str>,
        entete_gmflow: bool,
    ) -> Reponse {
        let mut r = Request::builder().method(methode).uri(chemin);
        if entete_gmflow {
            r = r.header("x-gmflow", "1");
        }
        if let Some(c) = cookie {
            r = r.header(header::COOKIE, c);
        }
        let req = match corps {
            Some(v) => r.header(header::CONTENT_TYPE, "application/json").body(Body::from(v.to_string())),
            None => r.body(Body::empty()),
        }
        .unwrap();
        let rep = self.routeur.clone().oneshot(req).await.unwrap();
        let code = rep.status();
        let entetes = rep.headers().clone();
        let cookie = rep
            .headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string());
        let octets = axum::body::to_bytes(rep.into_body(), usize::MAX).await.unwrap().to_vec();
        let json = serde_json::from_slice(&octets).unwrap_or(Value::Null);
        Reponse { code, json, octets, cookie, entetes }
    }

    pub async fn utilisateur(&self, identifiant: &str, mdp: &str, role: &str, societe_id: Option<i64>) -> i64 {
        let hash = gmflow_app::mdp::hacher(mdp).unwrap();
        sqlx::query_scalar::<_, i64>(
            "insert into utilisateur (identifiant, hash_mdp, role, societe_id) values ($1, $2, $3, $4) returning id",
        )
        .bind(identifiant)
        .bind(hash)
        .bind(role)
        .bind(societe_id)
        .fetch_one(self.pool())
        .await
        .unwrap()
    }

    pub async fn societe(&self, nom: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("insert into societe (raison_sociale) values ($1) returning id")
            .bind(nom)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    /// Connecte et rend le cookie de session.
    pub async fn connecter(&self, identifiant: &str, mdp: &str) -> String {
        let r = self
            .requete("POST", "/api/connexion", Some(serde_json::json!({"identifiant": identifiant, "mot_de_passe": mdp})), None)
            .await;
        assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
        r.cookie.expect("cookie de session")
    }

    pub async fn fermer(self) {
        self.base.fermer().await;
    }
}
