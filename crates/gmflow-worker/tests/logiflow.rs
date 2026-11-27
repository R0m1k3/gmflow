//! Logiflow contre un faux serveur local : essai de connexion, puis report d'une facture
//! envoyée sur la livraison de son BL (n° exact, n° écrit autrement, autre fournisseur,
//! livraison déjà liée à une autre facture, plusieurs BL) ; vérification régulière (livraison
//! de chaque BL, montants répartis entre les BL, échéance, nouvel essai).

mod commun;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, patch};
use commun::Banc;
use gmflow_core::montant::{Decimal, dec};
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
    .into_iter()
    .map(|mut l| {
        // Montants des BL saisis dans Logiflow.
        l["blAmount"] = match l["id"].as_i64() {
            Some(5) => json!(60),
            Some(6) => json!("40,00"),
            _ => Value::Null,
        };
        l["deliveredDate"] = json!("2026-11-20T07:00:00.000Z");
        l
    })
    .collect()
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

#[tokio::test]
async fn redirection_jamais_suivie() {
    let (base, recus) = commun::redirecteur().await;
    let c = logiflow::Client::new(&base, Some("bon")).unwrap();
    let e = c.essayer().await.unwrap_err();
    assert!(e.contains("redirige (HTTP 307)"), "{e}");
    assert_eq!(
        recus.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "clé partie ailleurs"
    );
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

async fn bl(b: &Banc, societe: i64, fournisseur: i64, numero: &str, ht: f64) -> i64 {
    let eml = gmflow_core::fixtures::eml(
        &format!("bl-{numero}@test"),
        numero,
        &[(&format!("{numero}.pdf"), "application/pdf", b"%PDF-1.4 bl")],
        false,
    );
    let mail = commun::mail(b, societe, &eml).await;
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, montant_ht, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'bl', $2, $3, $4, $5, now()
         from piece_jointe where mail_id = $1 returning id",
    )
    .bind(mail)
    .bind(societe)
    .bind(fournisseur)
    .bind(numero)
    .bind(ht)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn verification_reguliere() {
    let (base, ecrits) = faux_logiflow().await;
    let b = Banc::new().await;
    let frouard = b.societe("Frouard").await;
    let jja: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, statut, code_gestion) values ('JJA SA', 'jja sa', 'valide', 'J009') returning id",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    let bl100 = bl(&b, frouard, jja, "BL 100", 10.0).await;
    let bl401 = bl(&b, frouard, jja, "BL-401", 60.0).await;
    let bl777 = bl(&b, frouard, jja, "BL-777", 5.0).await;

    // Pas configuré : rien n'est fait, et la vérification n'est pas due.
    gmflow_worker::logiflow::suivre(&b.ctx).await.unwrap();
    assert!(!gmflow_worker::logiflow::suivi_du(b.pool()).await.unwrap());
    assert_eq!(
        b.compter("select count(*) from logiflow_livraison").await,
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
    assert!(gmflow_worker::logiflow::suivi_du(b.pool()).await.unwrap());

    // Facture de deux BL avec remise : chaque livraison reçoit sa part, avec l'échéance.
    let f5 = facture(&b, frouard, jja, "F-5", &["BL-401", "BL-402"]).await;
    // Un BL manque dans Logiflow : référence seule, nouvel essai au passage suivant.
    let f6 = facture(&b, frouard, jja, "F-6", &["BL-100", "BL-999"]).await;
    sqlx::query(
        "update document set montant_ht = 90, montant_ttc = 108, date_echeance = '2026-12-31'
         where id = $1",
    )
    .bind(f5)
    .execute(b.pool())
    .await
    .unwrap();

    gmflow_worker::logiflow::suivre(&b.ctx).await.unwrap();
    assert!(!gmflow_worker::logiflow::suivi_du(b.pool()).await.unwrap());
    let suivi = parametre::lire(b.pool(), logiflow::CLE_SUIVI)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(suivi["ok"], true, "{suivi}");
    assert!(
        suivi["message"].as_str().unwrap().starts_with("2 BL sur 3"),
        "{suivi}"
    );

    // BL retrouvés : celui du même fournisseur parmi deux « BL-100 » ; BL-777 absent.
    let livraison = |id: i64| {
        let pool = b.pool().clone();
        async move {
            sqlx::query_as::<_, (String, Option<i64>, Option<Decimal>)>(
                "select etat, livraison_id, montant_bl from logiflow_livraison where bl_id = $1",
            )
            .bind(id)
            .fetch_optional(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        livraison(bl100).await,
        Some(("trouve".into(), Some(1), None))
    );
    assert_eq!(
        livraison(bl401).await,
        Some(("trouve".into(), Some(5), Some(dec!(60))))
    );
    assert_eq!(livraison(bl777).await, None);

    let mut e = ecrits.lock().unwrap().clone();
    e.sort_by_key(|x| x.0);
    assert_eq!(
        e,
        vec![
            (1, json!({ "invoiceReference": "F-6" })),
            (
                5,
                json!({ "invoiceReference": "F-5", "invoiceAmount": 54.0,
                        "invoiceAmountTTC": 64.8, "dueDate": "2026-12-31" })
            ),
            (
                6,
                json!({ "invoiceReference": "F-5", "invoiceAmount": 36.0,
                        "invoiceAmountTTC": 43.2, "dueDate": "2026-12-31" })
            ),
        ]
    );
    let (ok, _, _) = resultat(&b, f5).await;
    assert!(ok);
    let (ok, _, l) = resultat(&b, f6).await;
    assert!(!ok);
    assert_eq!(l[1]["etat"], "introuvable", "{l}");

    // Passage suivant : seule la facture incomplète est reprise.
    ecrits.lock().unwrap().clear();
    sqlx::query("delete from parametre where cle = $1")
        .bind(logiflow::CLE_SUIVI)
        .execute(b.pool())
        .await
        .unwrap();
    gmflow_worker::logiflow::suivre(&b.ctx).await.unwrap();
    let e = ecrits.lock().unwrap().clone();
    assert_eq!(e, vec![(1, json!({ "invoiceReference": "F-6" }))]);

    // BL disparu de Logiflow (n° corrigé) : le lien est retiré.
    sqlx::query("update document set numero = 'BL-778' where id = $1")
        .bind(bl401)
        .execute(b.pool())
        .await
        .unwrap();
    gmflow_worker::logiflow::suivre(&b.ctx).await.unwrap();
    assert_eq!(livraison(bl401).await, None);
    b.fermer().await;
}
