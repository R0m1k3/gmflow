//! Lignes articles corrigées à la main (audit L16) : export Phénix et écarts facture/BL
//! recalculés, totaux calculés d'un BL suivis, valeurs vérifiées, journal, droits.

mod commun;

use std::io::Cursor;

use axum::http::StatusCode;
use calamine::{Data, Reader, Xlsx};
use commun::Appli;
use gmflow_core::montant;
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";

fn lire(octets: &[u8]) -> Vec<Vec<Data>> {
    let mut x: Xlsx<_> = Xlsx::new(Cursor::new(octets.to_vec())).unwrap();
    let nom = x.sheet_names()[0].clone();
    x.worksheet_range(&nom)
        .unwrap()
        .rows()
        .map(|l| l.to_vec())
        .collect()
}

struct Monde {
    app: Appli,
    admin: String,
    resp_a: String,
    resp_b: String,
    sa: i64,
    ba: i64,
    f: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    let ba = app.boite(sa, "Compta Frouard").await;
    let f: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, statut, code_gestion)
         values ('Revimport', 'revimport', 'valide', 'R0041') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    app.utilisateur("resp-b", MDP, "responsable", Some(sb))
        .await;
    let admin = app.connecter("admin", MDP).await;
    let resp_a = app.connecter("resp-a", MDP).await;
    let resp_b = app.connecter("resp-b", MDP).await;
    Monde {
        app,
        admin,
        resp_a,
        resp_b,
        sa,
        ba,
        f,
    }
}

type L<'a> = (
    &'a str,
    &'a str,
    f64,
    f64,
    Option<f64>,
    Option<f64>,
    Option<&'a str>,
);

/// Document lu (`bl` ou `facture`) avec ses lignes `(référence, désignation, qté, PU, colis, PCB, gencod)`.
async fn piece(m: &Monde, type_doc: &str, numero: &str, lignes: &[L<'_>]) -> i64 {
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}@test"),
        numero,
        &[(&format!("{numero}.pdf"), "application/pdf", b"%PDF-1.4 x")],
        false,
    );
    let mail = m.app.mail_vu(&eml, &[m.ba]).await;
    let pj = m.app.piece_de(mail).await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, lignes_lues, lu_le, date_document)
         select id, mail_id, sha256, 'lu', 'pdf_texte', $4, $2, $3, $5, true, now(), current_date
         from piece_jointe where id = $1 returning id",
    )
    .bind(pj)
    .bind(m.sa)
    .bind(m.f)
    .bind(type_doc)
    .bind(numero)
    .fetch_one(m.app.pool())
    .await
    .unwrap();
    for (i, (r, d, q, pu, c, p, g)) in lignes.iter().enumerate() {
        sqlx::query(
            "insert into ligne_document (document_id, rang, reference, designation, quantite,
                 prix_unitaire_ht, montant_ht, colis, pcb, gencod)
             values ($1, $2, $3, $4, $5::numeric, $6::numeric, round($5::numeric * $6::numeric, 2), $7::numeric, $8::numeric, $9)",
        )
        .bind(id)
        .bind(i as i32)
        .bind(r)
        .bind(d)
        .bind(montant::depuis_f64(*q))
        .bind(montant::depuis_f64(*pu))
        .bind(c.and_then(montant::depuis_f64))
        .bind(p.and_then(montant::depuis_f64))
        .bind(g)
        .execute(m.app.pool())
        .await
        .unwrap();
    }
    id
}

async fn lignes(m: &Monde, id: i64) -> Value {
    let r = m
        .app
        .requete(
            "GET",
            &format!("/api/documents/{id}/lignes"),
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    r.json
}

/// Corps de PUT : la ligne lue, avec les champs changés.
fn saisie(l: &Value, changes: Value) -> Value {
    let mut s = l.clone();
    for k in ["id", "rang", "corrigee_le", "corrigee_par"] {
        s.as_object_mut().unwrap().remove(k);
    }
    for (k, v) in changes.as_object().unwrap() {
        s[k] = v.clone();
    }
    s
}

#[tokio::test]
async fn pcb_corrige_l_export_phenix_suit() {
    let m = monde().await;
    let bl = piece(
        &m,
        "bl",
        "BL-7",
        &[(
            "AB123",
            "SEAU",
            2.0,
            5.2,
            Some(2.0),
            Some(1.0),
            Some("3532156987418"),
        )],
    )
    .await;
    let chemin = format!("/api/documents/{bl}/export-phenix/matrice");
    let r = m.app.requete("GET", &chemin, None, Some(&m.admin)).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(lire(&r.octets)[1][3], Data::Float(2.0));

    let l = lignes(&m, bl).await["lignes"][0].clone();
    assert_eq!(l["corrigee_le"], Value::Null);
    // PCB mal lu : 6 unités par colis, 2 colis.
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            Some(saisie(&l, json!({ "pcb": 6, "quantite": 12 }))),
            Some(&m.resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["ligne"]["pcb"], 6.0);
    assert!(r.json["ligne"]["corrigee_le"].is_string());
    assert_eq!(r.json["ligne"]["corrigee_par"], "resp-a");
    // Le montant suit la quantité (il était cohérent).
    assert_eq!(r.json["ligne"]["montant_ht"], 62.4);
    let r = m.app.requete("GET", &chemin, None, Some(&m.admin)).await;
    assert_eq!(lire(&r.octets)[1][3], Data::Float(12.0), "2 colis × 6");
    // Journal avant/après.
    let j: Value = sqlx::query_scalar(
        "select details from journal where action = 'ligne_corrigee' and objet_id = $1",
    )
    .bind(bl.to_string())
    .fetch_one(m.app.pool())
    .await
    .unwrap();
    assert_eq!(j["avant"]["pcb"], 1.0);
    assert_eq!(j["apres"]["pcb"], 6.0);
    // La fiche compte la ligne corrigée (avertissement avant une relecture).
    let r = m
        .app
        .requete("GET", &format!("/api/documents/{bl}"), None, Some(&m.admin))
        .await;
    assert_eq!(r.json["lignes_corrigees"], 1, "{}", r.json);

    // Valeurs refusées : gencod à la clé fausse, nombre négatif ; rien n'est écrit.
    for (changes, champ) in [
        (json!({ "gencod": "3532156987419" }), "gencod"),
        (json!({ "colis": -1 }), "colis"),
    ] {
        let r = m
            .app
            .requete(
                "PUT",
                &format!("/api/documents/{bl}/lignes/{}", l["id"]),
                Some(saisie(&l, changes)),
                Some(&m.admin),
            )
            .await;
        assert_eq!(r.code, StatusCode::BAD_REQUEST, "{}", r.json);
        assert_eq!(r.json["champ"], champ);
    }
    assert_eq!(lignes(&m, bl).await["lignes"][0]["pcb"], 6.0);

    // Responsable d'une autre société : ni lecture ni écriture.
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            Some(saisie(&l, json!({ "pcb": 1 }))),
            Some(&m.resp_b),
        )
        .await;
    assert!(
        r.code == StatusCode::NOT_FOUND || r.code == StatusCode::FORBIDDEN,
        "{}",
        r.code
    );
    let r = m
        .app
        .requete(
            "DELETE",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            None,
            Some(&m.resp_b),
        )
        .await;
    assert!(r.code == StatusCode::NOT_FOUND || r.code == StatusCode::FORBIDDEN);
    assert_eq!(lignes(&m, bl).await["lignes"][0]["pcb"], 6.0);
    m.app.fermer().await;
}

#[tokio::test]
async fn prix_corrige_l_ecart_facture_bl_est_recalcule() {
    let m = monde().await;
    let bl = piece(
        &m,
        "bl",
        "BL-8",
        &[
            ("AB123", "SEAU", 4.0, 5.2, None, None, None),
            ("CD456", "PELLE", 2.0, 3.0, None, None, None),
        ],
    )
    .await;
    let facture = piece(
        &m,
        "facture",
        "FA-8",
        &[
            ("AB123", "SEAU", 4.0, 5.5, None, None, None),
            ("CD456", "PELLE", 2.0, 3.0, None, None, None),
        ],
    )
    .await;
    sqlx::query(
        "update document set montant_ht = 28, montant_tva = 5.6, montant_ttc = 33.6,
             numeros_bl = '{BL-8}' where id = $1",
    )
    .bind(facture)
    .execute(m.app.pool())
    .await
    .unwrap();
    gmflow_core::rapprochement::recalculer(m.app.pool(), facture)
        .await
        .unwrap();
    let ecarts = || async {
        sqlx::query_scalar::<_, Value>(
            "select ecarts from rapprochement where facture_id = $1 and bl_id = $2",
        )
        .bind(facture)
        .bind(bl)
        .fetch_one(m.app.pool())
        .await
        .unwrap()
    };
    let e = ecarts().await;
    assert_eq!(e.as_array().unwrap().len(), 1, "{e}");
    assert_eq!(e[0]["genre"], "prix");

    // Le prix du BL était mal lu : 5,50.
    let l = lignes(&m, bl).await["lignes"][0].clone();
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            Some(saisie(&l, json!({ "prix_unitaire_ht": 5.5 }))),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["ligne"]["montant_ht"], 22.0);
    let e = ecarts().await;
    assert_eq!(e, json!([]), "écart disparu");

    // Ligne ajoutée sur la facture : article en trop côté facture, écart « manquant ».
    let r = m
        .app
        .requete(
            "POST",
            &format!("/api/documents/{facture}/lignes"),
            Some(json!({ "reference": "EF789", "designation": "BALAI", "quantite": 1, "prix_unitaire_ht": 4 })),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["ligne"]["montant_ht"], 4.0);
    let e = ecarts().await;
    assert_eq!(e.as_array().unwrap().len(), 1, "{e}");
    // Le contrôle « somme des lignes » de la facture voit la ligne en plus (32 € ≠ 28 €).
    let ctrl: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(facture)
        .fetch_one(m.app.pool())
        .await
        .unwrap();
    assert!(ctrl.to_string().contains("somme des lignes"), "{ctrl}");
    // Supprimée : tout revient.
    let id = r.json["ligne"]["id"].clone();
    let r = m
        .app
        .requete(
            "DELETE",
            &format!("/api/documents/{facture}/lignes/{id}"),
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(ecarts().await, json!([]));
    assert_eq!(
        m.app
            .compter(
                "select count(*) from journal where action in ('ligne_ajoutee', 'ligne_supprimee')"
            )
            .await,
        2
    );

    // Facture envoyée : ses lignes ne changent plus.
    sqlx::query("update document set statut_depot = 'depose' where id = $1")
        .bind(facture)
        .execute(m.app.pool())
        .await
        .unwrap();
    let l = lignes(&m, facture).await;
    assert_eq!(l["modifiable"], false);
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{facture}/lignes/{}", l["lignes"][0]["id"]),
            Some(saisie(&l["lignes"][0], json!({ "quantite": 3 }))),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT, "{}", r.json);
    m.app.fermer().await;
}

#[tokio::test]
async fn bl_aux_totaux_calcules_les_totaux_suivent() {
    let m = monde().await;
    let bl = piece(
        &m,
        "bl",
        "BL-9",
        &[("AB123", "SEAU", 4.0, 5.0, None, None, None)],
    )
    .await;
    sqlx::query(
        "update document set montant_ht = 20, montant_tva = 4, montant_ttc = 24,
             confiance = '{\"montants_calcules\": [\"ht\", \"tva\", \"ttc\"]}' where id = $1",
    )
    .bind(bl)
    .execute(m.app.pool())
    .await
    .unwrap();
    let l = lignes(&m, bl).await["lignes"][0].clone();
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            Some(saisie(&l, json!({ "quantite": 6 }))),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["totaux"]["montant_ht"], 30.0);
    assert_eq!(r.json["totaux"]["montant_tva"], 6.0);
    assert_eq!(r.json["totaux"]["montant_ttc"], 36.0);
    assert_eq!(
        r.json["totaux"]["montants_calcules"],
        json!(["ht", "tva", "ttc"])
    );
    m.app.fermer().await;
}

/// Lignes lues dans les cellules d'un tableur (audit L15, `lignes_source = cellules`) : elles se
/// corrigent comme les autres, la marque de leur origine reste, la fiche compte la correction
/// (« Relire » prévient qu'elle serait remplacée).
#[tokio::test]
async fn lignes_lues_dans_les_cellules_se_corrigent() {
    let m = monde().await;
    let bl = piece(
        &m,
        "bl",
        "BL-10",
        &[("AB123", "SEAU", 4.0, 5.0, None, None, None)],
    )
    .await;
    sqlx::query(
        "update document set voie = 'tableur',
             confiance = '{\"lignes_source\": \"cellules\", \"montants_calcules\": [\"ht\", \"tva\", \"ttc\"]}'
         where id = $1",
    )
    .bind(bl)
    .execute(m.app.pool())
    .await
    .unwrap();
    let l = lignes(&m, bl).await["lignes"][0].clone();
    let r = m
        .app
        .requete(
            "PUT",
            &format!("/api/documents/{bl}/lignes/{}", l["id"]),
            Some(saisie(&l, json!({ "quantite": 6 }))),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let fiche = m
        .app
        .requete("GET", &format!("/api/documents/{bl}"), None, Some(&m.admin))
        .await;
    assert_eq!(fiche.json["lignes_corrigees"], 1);
    assert_eq!(fiche.json["confiance"]["lignes_source"], "cellules");
    m.app.fermer().await;
}
