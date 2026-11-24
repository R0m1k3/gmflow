//! Logiflow contre un faux serveur local : essai de connexion, puis report d'une facture
//! envoyée sur la livraison de son BL (n° exact, n° écrit autrement, autre fournisseur,
//! livraison déjà liée à une autre facture, plusieurs BL).

mod commun;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, patch};
use commun::Banc;
use gmflow_core::{logiflow, parametre};
use serde_json::{Value, json};

type Ecrits = Arc<Mutex<Vec<(i64, Value)>>>;

fn livraisons() -> Vec<Value> {
    let l = |id: i64,
             code: &str,
             nom: &str,
             magasin: &str,
             bl: &str,
             facture: Value,
             rapprochee: bool| {
        json!({ "id": id, "storeName": magasin, "supplierId": id * 10, "supplierName": nom,
                "supplierCode": code, "blNumber": bl, "invoiceReference": facture,
                "reconciled": rapprochee, "automaticReconciliation": false, "status": "delivered" })
    };
    vec![
        l(
            1,
            "J009",
            "JJA",
            "Magasin Frouard",
            "BL-100",
            Value::Null,
            false,
        ),
        l(
            2,
            "L0091",
            "L3C",
            "Magasin Frouard",
            "BL-100",
            Value::Null,
            false,
        ),
        l(
            3,
            "J009",
            "JJA",
            "Magasin Frouard",
            "000200",
            Value::Null,
            false,
        ),
        l(
            4,
            "J009",
            "JJA",
            "Magasin Frouard",
            "BL-300",
            json!("F-AUTRE"),
            true,
        ),
        l(
            5,
            "J009",
            "JJA",
            "Magasin Frouard",
            "BL-401",
            Value::Null,
            false,
        ),
        l(
            6,
            "J009",
            "JJA",
            "Magasin Frouard",
            "BL-402",
            Value::Null,
            false,
        ),
    ]
}

fn autorise(h: &HeaderMap) -> bool {
    h.get("x-api-key").and_then(|v| v.to_str().ok()) == Some("bon")
}

async fn faux_logiflow() -> (String, Ecrits) {
    let ecrits: Ecrits = Arc::default();
    let app = Router::new()
        .route(
            "/api/ext/v1/stores",
            get(|h: HeaderMap| async move {
                if !autorise(&h) {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                axum::Json(json!([{ "id": 1, "name": "Magasin Frouard" }, { "id": 2, "name": "Magasin Houdemont" }])).into_response()
            }),
        )
        .route(
            "/api/ext/v1/suppliers",
            get(|| async {
                axum::Json(json!([{ "id": 10, "code": "J009", "name": "JJA" }, { "id": 20, "code": "L0091", "name": "L3C" }]))
            }),
        )
        .route(
            "/api/ext/v1/deliveries",
            get(|Query(q): Query<HashMap<String, String>>| async move {
                let items: Vec<Value> = livraisons()
                    .into_iter()
                    .filter(|l| {
                        q.get("blNumber").is_none_or(|b| {
                            l["blNumber"].as_str().unwrap().to_lowercase() == b.trim().to_lowercase()
                        })
                    })
                    .filter(|l| {
                        q.get("supplierId").is_none_or(|s| l["supplierCode"] == "J009" && s == "10")
                    })
                    .filter(|l| q.get("hasInvoice").is_none_or(|_| l["invoiceReference"].is_null()))
                    .collect();
                axum::Json(json!({ "total": items.len(), "items": items }))
            }),
        )
        .route(
            "/api/ext/v1/deliveries/{id}",
            patch(
                |State(e): State<Ecrits>, h: HeaderMap, Path(id): Path<i64>, axum::Json(corps): axum::Json<Value>| async move {
                    if !autorise(&h) {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    e.lock().unwrap().push((id, corps.clone()));
                    let mut l = livraisons().into_iter().find(|l| l["id"] == id).unwrap();
                    l["invoiceReference"] = corps["invoiceReference"].clone();
                    axum::Json(l).into_response()
                },
            ),
        )
        .with_state(ecrits.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (adresse, ecrits)
}

#[tokio::test]
async fn essai_de_connexion() {
    let (base, _) = faux_logiflow().await;
    let c = logiflow::Client::new(&base, Some("bon")).unwrap();
    let m = c.essayer().await.unwrap();
    assert!(m.contains("2 magasins") && m.contains("Houdemont"), "{m}");
    let c = logiflow::Client::new(&format!("{base}/api/ext/v1/"), Some("mauvais")).unwrap();
    assert!(c.essayer().await.unwrap_err().contains("401"));
}

async fn facture(b: &Banc, societe: i64, fournisseur: i64, numero: &str, bls: &[&str]) -> i64 {
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}@test"),
        numero,
        &[(&format!("{numero}.pdf"), "application/pdf", b"%PDF-1.4 x")],
        false,
    );
    let mail = commun::mail(b, societe, &eml).await;
    let bls: Vec<String> = bls.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, montant_ht, montant_ttc, numeros_bl, statut_depot, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'facture', $2, $3, $4, 100.5, 120.6, $5,
                'depose', now()
         from piece_jointe where mail_id = $1 returning id",
    )
    .bind(mail)
    .bind(societe)
    .bind(fournisseur)
    .bind(numero)
    .bind(bls)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

async fn resultat(b: &Banc, document: i64) -> (bool, String, Value) {
    sqlx::query_as(
        "select ok, message, livraisons from logiflow_rapprochement where document_id = $1",
    )
    .bind(document)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn facture_envoyee_reportee_sur_la_livraison() {
    let (base, ecrits) = faux_logiflow().await;
    let b = Banc::new().await;
    let frouard = b.societe("Frouard").await;
    let jja: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, statut, code_gestion) values ('JJA SA', 'jja sa', 'valide', 'J009') returning id",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    let f1 = facture(&b, frouard, jja, "F-1", &["BL-100"]).await;

    // Pas configuré : rien n'est fait.
    gmflow_worker::logiflow::rapprocher(&b.ctx, f1)
        .await
        .unwrap();
    assert_eq!(
        b.compter("select count(*) from logiflow_rapprochement")
            .await,
        0
    );

    parametre::ecrire(
        b.pool(),
        logiflow::CLE,
        &json!({ "adresse": base, "fin": "bon" }),
    )
    .await
    .unwrap();
    parametre::ecrire_secret(b.pool(), logiflow::CLE, &b.ctx.chiffreur.chiffrer(b"bon"))
        .await
        .unwrap();

    // N° exact, deux fournisseurs : celui du code BO Phénix ; montants écrits (un seul BL).
    gmflow_worker::logiflow::rapprocher(&b.ctx, f1)
        .await
        .unwrap();
    let (ok, m, l) = resultat(&b, f1).await;
    assert!(ok, "{m} {l}");
    assert_eq!(l[0]["livraison_id"], 1);
    assert_eq!(
        ecrits.lock().unwrap().pop().unwrap(),
        (
            1,
            json!({ "invoiceReference": "F-1", "invoiceAmount": 100.5, "invoiceAmountTTC": 120.6 })
        )
    );

    // N° écrit autrement (« BL 200 » contre « 000200 ») : retrouvé parmi les livraisons sans facture.
    let f2 = facture(&b, frouard, jja, "F-2", &["BL 200"]).await;
    gmflow_worker::logiflow::rapprocher(&b.ctx, f2)
        .await
        .unwrap();
    let (ok, _, l) = resultat(&b, f2).await;
    assert!(ok, "{l}");
    assert_eq!(ecrits.lock().unwrap().pop().unwrap().0, 3);

    // Livraison déjà rapprochée avec une autre facture : rien n'est écrit.
    let f3 = facture(&b, frouard, jja, "F-3", &["BL-300"]).await;
    gmflow_worker::logiflow::rapprocher(&b.ctx, f3)
        .await
        .unwrap();
    let (ok, _, l) = resultat(&b, f3).await;
    assert!(!ok);
    assert_eq!(l[0]["etat"], "autre_facture", "{l}");
    assert!(ecrits.lock().unwrap().is_empty());

    // Deux BL : la référence sur chaque livraison, sans les montants ; un BL inconnu signalé.
    let f4 = facture(&b, frouard, jja, "F-4", &["BL-401", "BL-402", "BL-999"]).await;
    gmflow_worker::logiflow::rapprocher(&b.ctx, f4)
        .await
        .unwrap();
    let (ok, m, l) = resultat(&b, f4).await;
    assert!(!ok);
    assert!(m.contains("2 BL sur 3"), "{m}");
    let etats: Vec<&str> = l
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["etat"].as_str().unwrap())
        .collect();
    assert_eq!(etats, ["complete", "complete", "introuvable"]);
    let e = ecrits.lock().unwrap().clone();
    assert_eq!(e.len(), 2);
    assert!(
        e.iter()
            .all(|(_, c)| *c == json!({ "invoiceReference": "F-4" }))
    );

    // Pas une facture envoyée : ignorée.
    sqlx::query("update document set statut_depot = 'erreur' where id = $1")
        .bind(f1)
        .execute(b.pool())
        .await
        .unwrap();
    sqlx::query("delete from logiflow_rapprochement")
        .execute(b.pool())
        .await
        .unwrap();
    gmflow_worker::logiflow::rapprocher(&b.ctx, f1)
        .await
        .unwrap();
    assert_eq!(
        b.compter("select count(*) from logiflow_rapprochement")
            .await,
        0
    );
    b.fermer().await;
}
