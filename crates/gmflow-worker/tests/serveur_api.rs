//! Serveur du magasin (Apiflow) contre un faux serveur local : essai de connexion, copie des
//! fournisseurs (deux pages) et codes BO Phénix proposés pour une fiche.

mod commun;

use axum::Router;
use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use commun::Banc;
use gmflow_core::{parametre, serveur_api};
use serde_json::{Value, json};

fn tous() -> Vec<Value> {
    vec![
        json!({ "codefou": "J009", "nom": "JJA", "email": "thanh.nguyen@jja-sa.com", "nb_articles": "50631", "nb_actifs": "40000" }),
        json!({ "codefou": "L0091", "nom": "L3C", "email": null, "nb_articles": "12347" }),
        json!({ "codefou": "D028", "nom": "DECOR 10", "email": null, "nb_articles": 900 }),
        json!({ "codefou": "S024", "nom": "STYLE DECOR", "email": null, "nb_articles": 50 }),
    ]
}

async fn faux_apiflow() -> String {
    let app = Router::new().route(
        "/api/fournisseurs",
        get(|h: HeaderMap, Query(q): Query<std::collections::HashMap<String, String>>| async move {
            if h.get("x-api-key").and_then(|v| v.to_str().ok()) != Some("bon") {
                return (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "Clé d'API requise" })))
                    .into_response();
            }
            // Pages de 3 au plus, quelle que soit la limite demandée : oblige à paginer.
            let limit: usize = q.get("limit").and_then(|l| l.parse().ok()).unwrap_or(500).min(3);
            let page: usize = q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
            let t = tous();
            let lignes: Vec<Value> = t.iter().skip((page - 1) * limit).take(limit).cloned().collect();
            axum::Json(json!({ "count": lignes.len(), "total": t.len(), "page": page, "limit": limit, "fournisseurs": lignes }))
                .into_response()
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    adresse
}

#[tokio::test]
async fn essai_de_connexion() {
    let base = faux_apiflow().await;
    let ok = serveur_api::essayer(&base, Some("bon")).await.unwrap();
    assert!(ok.contains("4 fournisseurs"), "{ok}");
    assert!(
        serveur_api::essayer(&format!("{base}/api/"), Some("bon"))
            .await
            .is_ok()
    );
    assert!(
        serveur_api::essayer(&base, Some("mauvais"))
            .await
            .unwrap_err()
            .contains("refusée")
    );
    assert!(
        serveur_api::essayer(&base, None)
            .await
            .unwrap_err()
            .contains("refusée")
    );
    assert!(
        serveur_api::essayer(&format!("{base}/autre"), Some("bon"))
            .await
            .unwrap_err()
            .contains("404")
    );
    let e = serveur_api::essayer("http://127.0.0.1:1", Some("bon"))
        .await
        .unwrap_err();
    assert!(e.contains("connexion impossible"), "{e}");
}

#[tokio::test]
async fn redirection_jamais_suivie() {
    let (base, recus) = commun::redirecteur().await;
    let e = serveur_api::essayer(&base, Some("bon")).await.unwrap_err();
    assert!(e.contains("redirige (HTTP 307)"), "{e}");
    assert_eq!(
        recus.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "clé partie ailleurs"
    );
}

async fn fiche(b: &Banc, nom: &str, domaines: &[&str], code: Option<&str>) -> i64 {
    let d: Vec<String> = domaines.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, domaines, statut, code_gestion)
         values ($1, $2, $3, 'valide', $4) returning id",
    )
    .bind(nom)
    .bind(gmflow_core::fournisseurs::normaliser(nom))
    .bind(d)
    .bind(code)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn copie_et_codes_proposes() {
    let base = faux_apiflow().await;
    let b = Banc::new().await;
    // Pas configuré : rien n'est dû, rien n'est copié.
    assert!(
        !gmflow_worker::serveur_api::synchro_due(b.pool())
            .await
            .unwrap()
    );

    parametre::ecrire(
        b.pool(),
        serveur_api::CLE,
        &json!({ "adresse": base, "fin": "bon" }),
    )
    .await
    .unwrap();
    parametre::ecrire_secret(
        b.pool(),
        serveur_api::CLE,
        &b.ctx.chiffreur.chiffrer(b"bon"),
    )
    .await
    .unwrap();
    assert!(
        gmflow_worker::serveur_api::synchro_due(b.pool())
            .await
            .unwrap()
    );
    gmflow_worker::serveur_api::synchroniser(&b.ctx)
        .await
        .unwrap();
    assert!(
        !gmflow_worker::serveur_api::synchro_due(b.pool())
            .await
            .unwrap()
    );
    let s = parametre::lire(b.pool(), serveur_api::CLE_SYNCHRO)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s["ok"], true, "{s}");
    assert_eq!(s["nombre"], 4);
    let (n, art): (i64, Option<i64>) = sqlx::query_as(
        "select (select count(*) from fournisseur_magasin), (select nb_articles from fournisseur_magasin where code = 'J009')",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    assert_eq!((n, art), (4, Some(50631)));

    let jja = fiche(&b, "JJA SA", &["jja-sa.com"], None).await;
    let decor = fiche(&b, "Decor10", &[], None).await;
    fiche(&b, "L3C", &[], Some("L0091")).await;
    let l3c_bis = fiche(&b, "L 3 C Distribution", &[], None).await;

    let s = serveur_api::suggestions(b.pool(), jja, None).await.unwrap();
    assert_eq!(s[0].code, "J009");
    assert!(s[0].raison.as_deref().unwrap().contains("domaine"));
    let s = serveur_api::suggestions(b.pool(), decor, None)
        .await
        .unwrap();
    assert_eq!(s[0].code, "D028");
    assert_eq!(s[0].raison.as_deref(), Some("même nom"));
    // Code déjà porté par une autre fiche : signalé.
    let s = serveur_api::suggestions(b.pool(), l3c_bis, None)
        .await
        .unwrap();
    let l = s.iter().find(|x| x.code == "L0091").expect("L3C proposé");
    assert_eq!(l.deja_pris_par.as_deref(), Some("L3C"));
    // Recherche par code ou par nom.
    let s = serveur_api::suggestions(b.pool(), decor, Some("decor"))
        .await
        .unwrap();
    let codes: Vec<&str> = s.iter().map(|x| x.code.as_str()).collect();
    assert_eq!(codes, ["D028", "S024"]);
    let s = serveur_api::suggestions(b.pool(), decor, Some("j00"))
        .await
        .unwrap();
    assert_eq!(s.len(), 1);

    // Clé refusée : la copie précédente reste, l'échec est noté.
    parametre::ecrire_secret(
        b.pool(),
        serveur_api::CLE,
        &b.ctx.chiffreur.chiffrer(b"mauvais"),
    )
    .await
    .unwrap();
    gmflow_worker::serveur_api::synchroniser(&b.ctx)
        .await
        .unwrap();
    let s = parametre::lire(b.pool(), serveur_api::CLE_SYNCHRO)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s["ok"], false);
    let n: i64 = sqlx::query_scalar("select count(*) from fournisseur_magasin")
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(n, 4);
    b.fermer().await;
}
