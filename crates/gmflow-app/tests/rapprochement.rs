//! API du rapprochement : vue par fournisseur, détail d'un document, liens manuels, « pas de
//! correspondance attendue », relecture des BL sans lignes, et droits d'un responsable.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    admin: String,
    resp_a: String,
    resp_b: String,
    facture: i64,
    bl: i64,
    bl_autre: i64,
    bl_b: i64,
}

/// Document lu de type `type_doc` pour un nouveau mail reçu par `boite`.
#[allow(clippy::too_many_arguments)]
async fn doc(
    app: &Appli,
    boite: i64,
    societe: i64,
    fournisseur: i64,
    type_doc: &str,
    numero: &str,
    commande: &str,
    ht: f64,
) -> i64 {
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}@test"),
        numero,
        &[(&format!("{numero}.pdf"), "application/pdf", b"%PDF-1.4 x")],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    let piece = app.piece_de(mail).await;
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_id, numero, numero_commande, date_document, montant_ht, lignes_lues, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', $2, $3, $4, $5, $6, date '2026-11-12', $7,
                $2 = 'facture', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(type_doc)
    .bind(societe)
    .bind(fournisseur)
    .bind(numero)
    .bind(commande)
    .bind(ht)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    let ba = app.boite(sa, "Compta Frouard").await;
    let bb = app.boite(sb, "Compta Houdemont").await;
    let f: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, statut) values ('Durand', 'durand', 'valide') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    app.utilisateur("resp-b", MDP, "responsable", Some(sb))
        .await;
    let facture = doc(&app, ba, sa, f, "facture", "FA-1", "CMD-1", 75.0).await;
    let bl = doc(&app, ba, sa, f, "bl", "BL-1", "CMD-1", 50.0).await;
    let bl_autre = doc(&app, ba, sa, f, "bl", "BL-2", "CMD-2", 25.0).await;
    let bl_b = doc(&app, bb, sb, f, "bl", "BL-3", "CMD-1", 10.0).await;
    sqlx::query(
        "insert into ligne_document (document_id, rang, reference, designation, quantite, montant_ht)
         values ($1, 0, 'REF-1', 'Seau', 20, 50), ($2, 0, 'REF-1', 'Seau', 20, 50),
                ($2, 1, 'REF-2', 'Balai', 5, 25)",
    )
    .bind(bl)
    .bind(facture)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("update document set lignes_lues = true where id = $1")
        .bind(bl)
        .execute(app.pool())
        .await
        .unwrap();
    for id in [facture, bl, bl_autre, bl_b] {
        gmflow_core::rapprochement::recalculer(app.pool(), id)
            .await
            .unwrap();
    }
    let admin = app.connecter("admin", MDP).await;
    let resp_a = app.connecter("resp-a", MDP).await;
    let resp_b = app.connecter("resp-b", MDP).await;
    Monde {
        app,
        admin,
        resp_a,
        resp_b,
        facture,
        bl,
        bl_autre,
        bl_b,
    }
}

#[tokio::test]
async fn vue_par_fournisseur_et_droits_du_responsable() {
    let m = monde().await;
    let app = &m.app;
    // Admin : la facture est rapprochée du BL de même commande et de même structure ; le BL de
    // l'autre structure n'est pas relié malgré la même commande.
    let r = app
        .requete("GET", "/api/rapprochement", None, Some(&m.admin))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let v = &r.json;
    assert_eq!(v["avec_ecart"].as_array().unwrap().len(), 1, "{v}");
    let a = &v["avec_ecart"][0];
    assert_eq!(a["facture"]["id"], m.facture);
    assert_eq!(a["bls"][0]["id"], m.bl);
    assert_eq!(a["bls"][0]["methode"], "commande");
    assert_eq!(a["ecart_montant"], 25.0);
    assert_eq!(a["ecarts"][0]["reference"], "REF-2");
    let sans: Vec<i64> = v["bl_sans_facture"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert_eq!(sans.len(), 2);
    assert!(sans.contains(&m.bl_autre) && sans.contains(&m.bl_b));
    assert_eq!(v["resume"][0]["bls"], 3);
    assert_eq!(v["resume"][0]["factures_avec_ecart"], 1);
    assert_eq!(v["bl_sans_lignes"], 2);

    // Responsable B : seulement son BL.
    let r = app
        .requete("GET", "/api/rapprochement", None, Some(&m.resp_b))
        .await;
    assert_eq!(r.json["avec_ecart"], json!([]));
    assert_eq!(r.json["bl_sans_facture"].as_array().unwrap().len(), 1);
    assert_eq!(r.json["bl_sans_facture"][0]["id"], m.bl_b);
    assert_eq!(
        app.requete(
            "GET",
            &format!("/api/documents/{}/rapprochement", m.facture),
            None,
            Some(&m.resp_b)
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    // Il ne peut pas relier un document d'une autre structure.
    let r = app
        .requete(
            "POST",
            "/api/rapprochement",
            Some(json!({ "facture_id": m.facture, "bl_id": m.bl_b })),
            Some(&m.resp_b),
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    // Relecture des BL : administrateur seulement.
    assert_eq!(
        app.requete(
            "POST",
            "/api/admin/rapprochement/relire-bl",
            None,
            Some(&m.resp_a)
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn liens_manuels_et_pas_de_correspondance_attendue() {
    let m = monde().await;
    let app = &m.app;
    let c = Some(m.resp_a.as_str());
    // Détail de la facture : lignes, lien, et le second BL du fournisseur comme candidat.
    let r = app
        .requete(
            "GET",
            &format!("/api/documents/{}/rapprochement", m.facture),
            None,
            c,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["lignes"].as_array().unwrap().len(), 2);
    assert_eq!(r.json["liens"][0]["autre"]["id"], m.bl);
    let candidats: Vec<i64> = r.json["candidats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert_eq!(
        candidats,
        [m.bl_autre],
        "le BL de l'autre structure n'est pas proposé"
    );

    // Lien manuel avec le second BL : l'écart de montant tombe à 0.
    let r = app
        .requete(
            "POST",
            "/api/rapprochement",
            Some(json!({ "facture_id": m.facture, "bl_id": m.bl_autre })),
            c,
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = app
        .requete(
            "GET",
            &format!("/api/documents/{}/rapprochement", m.facture),
            None,
            c,
        )
        .await;
    let liens = r.json["liens"].as_array().unwrap();
    assert_eq!(liens.len(), 2);
    assert!(liens.iter().all(|l| l["ecart_montant"] == 0.0), "{liens:?}");
    let manuel = liens.iter().find(|l| l["methode"] == "manuel").unwrap();
    assert_eq!(manuel["auteur"], "resp-a");
    assert_eq!(
        app.compter("select count(*) from journal where action = 'rapprochement'")
            .await,
        1
    );

    // Lien automatique défait : il n'est pas recréé.
    let auto = liens.iter().find(|l| l["methode"] == "commande").unwrap();
    let r = app
        .requete(
            "DELETE",
            &format!("/api/rapprochement/{}", auto["id"]),
            None,
            c,
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    gmflow_core::rapprochement::recalculer(app.pool(), m.facture)
        .await
        .unwrap();
    assert_eq!(
        app.compter("select count(*) from rapprochement where not rejete")
            .await,
        1
    );

    // BL marqué « pas de correspondance attendue » : sort de la vue.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{}/rapprochement-ignore", m.bl),
            Some(json!({ "ignore": true })),
            c,
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let v = app
        .requete("GET", "/api/rapprochement", None, Some(&m.admin))
        .await
        .json;
    let sans: Vec<i64> = v["bl_sans_facture"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    assert_eq!(sans, [m.bl_b]);

    // Relecture des BL lus sans lignes (administrateur).
    let r = app
        .requete(
            "POST",
            "/api/admin/rapprochement/relire-bl",
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["n"], 2);
    assert_eq!(
        app.compter("select count(*) from document where type = 'bl' and etat = 'a_lire'")
            .await,
        2
    );
    m.app.fermer().await;
}
