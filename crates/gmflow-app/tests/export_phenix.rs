//! Export d'un BL vers BO Phénix : état, attente de la recherche au magasin, fichiers, droits.

mod commun;

use std::io::Cursor;

use axum::http::StatusCode;
use calamine::{Data, Reader, Xlsx};
use commun::Appli;
use serde_json::json;

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

#[tokio::test]
async fn export_du_bl_apres_la_recherche_au_magasin() {
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
    app.utilisateur("resp-b", MDP, "responsable", Some(sb))
        .await;
    let eml = gmflow_core::fixtures::eml(
        "bl-7@test",
        "BL 7",
        &[("bl.pdf", "application/pdf", b"%PDF-1.4 x")],
        false,
    );
    let mail = app.mail_vu(&eml, &[ba]).await;
    let piece = app.piece_de(mail).await;
    let bl: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, lignes_lues, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'bl', $2, $3, 'BL/7', true, now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(sa)
    .bind(f)
    .fetch_one(app.pool())
    .await
    .unwrap();
    sqlx::query(
        "insert into ligne_document (document_id, rang, reference, designation, quantite,
             prix_unitaire_ht, gencod, colis, pcb, ecomob_ht)
         values ($1, 0, '03/3484', 'BOITE LOT DE 3', 2, 18, null, 2, 1, 0.19),
                ($1, 1, 'AB123', 'SEAU', 4, 5.2, '3532156987412', null, null, null)",
    )
    .bind(bl)
    .execute(app.pool())
    .await
    .unwrap();
    let admin = app.connecter("admin", MDP).await;
    let resp_b = app.connecter("resp-b", MDP).await;
    let chemin = format!("/api/documents/{bl}/export-phenix");

    // Sans serveur du magasin : prêt, avec l'alerte.
    let r = app.requete("GET", &chemin, None, Some(&admin)).await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["pret"], true);
    assert_eq!(r.json["code_fournisseur"], "R0041");
    assert!(
        r.json["matrice"]["alertes"][0]["message"]
            .as_str()
            .unwrap()
            .contains("non configuré")
    );

    // Serveur configuré : rien tant que la recherche n'est pas faite.
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::serveur_api::CLE,
        &json!({ "adresse": "https://api.example.test" }),
    )
    .await
    .unwrap();
    let r = app.requete("GET", &chemin, None, Some(&admin)).await;
    assert_eq!(r.json["pret"], false);
    let r = app
        .requete("GET", &format!("{chemin}/matrice"), None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    let r = app
        .requete("POST", &format!("{chemin}/verifier"), None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED);
    let r = app.requete("GET", &chemin, None, Some(&admin)).await;
    assert_eq!(r.json["en_cours"], true);

    // Le worker a trouvé le lot.
    sqlx::query(
        "update ligne_document set magasin = $2 where document_id = $1 and rang = 0;
         ",
    )
    .bind(bl)
    .bind(json!({
        "par": "reference", "no_id": "132319", "codein": "440088", "libelle": "LOT", "gencod": null,
        "composants": [
            { "no_id": "1", "codein": "440110", "libelle": "S", "lib_ticket": null, "gencod": "3262313080538",
              "reference": "03/3484-S", "quantite": 1.0, "pa": 2.0, "pv": 4.99, "pv_conseille": null,
              "eco_ht": 0.04, "famille": "310902", "pourc_marge": null },
            { "no_id": "2", "codein": "440112", "libelle": "L", "lib_ticket": null, "gencod": "3262313080552",
              "reference": "03/3484-L", "quantite": 2.0, "pa": 3.5, "pv": 7.99, "pv_conseille": null,
              "eco_ht": 0.07, "famille": "310902", "pourc_marge": null }
        ]
    }))
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("update document set magasin_verifie_le = now() where id = $1")
        .bind(bl)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app.requete("GET", &chemin, None, Some(&admin)).await;
    assert_eq!(r.json["pret"], true, "{:?}", r.json);
    assert_eq!(r.json["lots"], 1);
    assert_eq!(r.json["composants"], 2);
    assert_eq!(r.json["matrice"]["rangees"], 3);
    assert_eq!(r.json["trame"]["rangees"], 4);

    let r = app
        .requete("GET", &format!("{chemin}/matrice"), None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let dispo = r.entetes["content-disposition"].to_str().unwrap();
    assert!(
        dispo.contains("MATRICE INTEGRATION BACKOFFICE - Revimport - BL-7.xlsx"),
        "{dispo}"
    );
    let l = lire(&r.octets);
    assert_eq!(l[0][5], Data::String("REMISE ".into()));
    // Lot : 2 lots à 18 € = 2 × (2 + 2 × 3,5 = 9) au magasin → prix doublés.
    assert_eq!(l[1][0], Data::String("440110".into()));
    assert_eq!(l[1][3], Data::Float(2.0));
    assert_eq!(l[1][4], Data::Float(4.0));
    assert_eq!(l[2][3], Data::Float(4.0));
    assert_eq!(l[2][4], Data::Float(7.0));
    assert_eq!(l[3][1], Data::String("3532156987412".into()));

    let r = app
        .requete("GET", &format!("{chemin}/trame"), None, Some(&admin))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let l = lire(&r.octets);
    let col = |n: &str| {
        l[0].iter()
            .position(|c| *c == Data::String(n.into()))
            .unwrap()
    };
    assert_eq!(l[1][col("Codein")], Data::String("440088".into()));
    assert_eq!(l[2][col("NUM LOT VENTE")], Data::Float(1.0));
    assert_eq!(l[3][col("Ecomob HT")], Data::Float(0.07));
    assert_eq!(l[4][col("Fournisseur")], Data::String("R0041".into()));

    // BL lu en partie seulement : alerte en tête des deux fichiers.
    sqlx::query("update document set confiance = '{\"lignes_tronquees\": true}' where id = $1")
        .bind(bl)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app.requete("GET", &chemin, None, Some(&admin)).await;
    for f in ["matrice", "trame"] {
        assert!(
            r.json[f]["alertes"][0]["message"]
                .as_str()
                .unwrap()
                .contains("trop long pour être lu en entier"),
            "{:?}",
            r.json[f]
        );
    }

    // Un responsable d'une autre société ne voit pas ce BL.
    let r = app.requete("GET", &chemin, None, Some(&resp_b)).await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app
        .requete("GET", &format!("{chemin}/trame"), None, Some(&resp_b))
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    app.fermer().await;
}
