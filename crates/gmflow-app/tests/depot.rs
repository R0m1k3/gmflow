//! Lot 3 côté API : droits sur la correction et le dépôt, corrections enregistrées, contrôles
//! relancés, fiches fournisseurs (validation, fusion), suivi des dépôts.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::json;

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
    sa: i64,
    sb: i64,
    boite_a: i64,
    boite_b: i64,
    fournisseur: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    sqlx::query("update societe set siret = '12345678900014', adresse_depot = 'depot@dext.test', outil_depot = 'dext' where id = $1")
        .bind(sa)
        .execute(app.pool())
        .await
        .unwrap();
    let boite_a = app.boite(sa, "Compta Frouard").await;
    let boite_b = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    let fournisseur: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut)
         values ('Grossiste Durand', 'grossiste durand', '55210055400013', 'a_confirmer') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    let c_admin = app.connecter("admin", MDP).await;
    let c_resp_a = app.connecter("resp-a", MDP).await;
    Monde {
        app,
        c_admin,
        c_resp_a,
        sa,
        sb,
        boite_a,
        boite_b,
        fournisseur,
    }
}

/// Facture lue, complète et datée d'aujourd'hui, rattachée au fournisseur du monde.
async fn facture(m: &Monde, boites: &[i64], societe_id: i64, numero: &str) -> i64 {
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}-{}@test", rand_suffixe()),
        "Facture",
        &[("f.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, boites).await;
    let piece = app.piece_de(mail).await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, fournisseur_id, numero, date_document, montant_ht, montant_tva,
             montant_ttc, confiance, texte, lu_le)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', 'facture', $2,
                'Durand', $3, $4, current_date, 50.00, 10.00, 60.00,
                '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(societe_id)
    .bind(m.fournisseur)
    .bind(numero)
    .fetch_one(app.pool())
    .await
    .unwrap();
    gmflow_core::depot::evaluer(app.pool(), id).await.unwrap();
    id
}

fn rand_suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

async fn statut(app: &Appli, id: i64) -> Option<String> {
    sqlx::query_scalar("select statut_depot from document where id = $1")
        .bind(id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

async fn configurer_expedition(app: &Appli) {
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::expedition::CLE,
        &json!({ "serveur": "smtp.test", "port": 465, "securite": "tls", "adresse": "gmflow@test.local" }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn correction_droits_et_controles_relances() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.c_resp_a.as_str());
    let doc = facture(&m, &[m.boite_a], m.sa, "F-1").await;
    assert_eq!(statut(app, doc).await.as_deref(), Some("a_verifier"));

    // Mail vu dans les deux boîtes, document classé à Houdemont : visible, pas modifiable.
    let doc_b = facture(&m, &[m.boite_a, m.boite_b], m.sb, "F-2").await;
    assert_eq!(
        app.requete("GET", &format!("/api/documents/{doc_b}"), None, resp)
            .await
            .code,
        StatusCode::OK
    );
    let aujourdhui: String = sqlx::query_scalar("select current_date::text")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let correction = json!({
        "type": "facture", "societe_id": m.sa, "fournisseur_id": m.fournisseur,
        "numero": "F-2", "date_document": aujourdhui, "numero_commande": null,
        "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0
    });
    assert_eq!(
        app.requete(
            "PUT",
            &format!("/api/documents/{doc_b}"),
            Some(correction.clone()),
            resp
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    // Vers une autre structure que la sienne : refusé aussi.
    let mut vers_b = correction.clone();
    vers_b["societe_id"] = json!(m.sb);
    vers_b["numero"] = json!("F-1");
    assert_ne!(
        app.requete("PUT", &format!("/api/documents/{doc}"), Some(vers_b), resp)
            .await
            .code,
        StatusCode::NO_CONTENT
    );

    // Correction des montants sur son propre document : enregistrée avec la valeur lue.
    let mut c = correction.clone();
    c["numero"] = json!("F-1");
    c["montant_ht"] = json!(40.0);
    c["montant_tva"] = json!(20.0);
    let r = app
        .requete("PUT", &format!("/api/documents/{doc}"), Some(c), resp)
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT, "{}", r.json);
    let r = app
        .requete("GET", &format!("/api/documents/{doc}"), None, resp)
        .await;
    assert_eq!(r.json["corrections"]["montant_ht"]["avant"], 50.0);
    assert_eq!(r.json["corrections"]["montant_ht"]["apres"], 40.0);
    assert!(r.json["verifie_le"].is_string());
    let codes: Vec<&str> = r.json["controles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["fournisseur"], "fiche encore à confirmer");

    // Un responsable ne gère pas le référentiel ; l'administrateur valide la fiche et la
    // facture devient prête.
    let fiche =
        json!({ "nom": "Grossiste Durand", "siret": "552 100 554 00013", "statut": "valide" });
    assert_eq!(
        app.requete(
            "PUT",
            &format!("/api/admin/fournisseurs/{}", m.fournisseur),
            Some(fiche.clone()),
            resp
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    let r = app
        .requete(
            "PUT",
            &format!("/api/admin/fournisseurs/{}", m.fournisseur),
            Some(fiche),
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["siret"], "55210055400013");
    assert_eq!(statut(app, doc).await.as_deref(), Some("pret"));
    // SIRET faux : refusé.
    let r = app
        .requete(
            "PUT",
            &format!("/api/admin/fournisseurs/{}", m.fournisseur),
            Some(json!({ "nom": "Durand", "siret": "55210055400014" })),
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    m.app.fermer().await;
}

#[tokio::test]
async fn validation_et_depot() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.c_resp_a.as_str());
    let admin = Some(m.c_admin.as_str());
    let doc = facture(&m, &[m.boite_a], m.sa, "F-10").await;
    let url = format!("/api/documents/{doc}/deposer");

    // Sans adresse d'expédition : impossible.
    let r = app.requete("POST", &url, Some(json!({})), resp).await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    configurer_expedition(app).await;

    // Validation humaine malgré la fiche à confirmer : le dépôt part dans la file.
    let r = app.requete("POST", &url, Some(json!({})), resp).await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'envoi' and convert_from(job, 'UTF8') like '%Deposer%'").await,
        1
    );
    let valide: Option<i64> = sqlx::query_scalar("select valide_par from document where id = $1")
        .bind(doc)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(valide.is_some());

    // Même numéro, même fournisseur, même montant : doublon, bloqué sauf forçage admin.
    let double = facture(&m, &[m.boite_a], m.sa, "F-10").await;
    assert_eq!(statut(app, double).await.as_deref(), Some("doublon"));
    let url2 = format!("/api/documents/{double}/deposer");
    assert_eq!(
        app.requete("POST", &url2, Some(json!({})), resp).await.code,
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.requete("POST", &url2, Some(json!({ "forcer": true })), resp)
            .await
            .code,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.requete("POST", &url2, Some(json!({ "forcer": true })), admin)
            .await
            .code,
        StatusCode::ACCEPTED
    );

    // Structure sans adresse de dépôt : refusé avec le motif.
    let sans = facture(&m, &[m.boite_b], m.sb, "F-11").await;
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{sans}/deposer"),
            Some(json!({})),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);

    // Envoyé : plus de correction possible.
    sqlx::query("update document set statut_depot = 'depose' where id = $1")
        .bind(doc)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "PUT",
            &format!("/api/documents/{doc}"),
            Some(
                json!({ "type": "facture", "societe_id": m.sa, "fournisseur_id": m.fournisseur,
                          "numero": "F-99", "date_document": null, "numero_commande": null,
                          "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0 }),
            ),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);

    // Suivi du mois.
    let r = app.requete("GET", "/api/depots/suivi", None, admin).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let frouard = r.json["lignes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["societe_id"] == m.sa)
        .unwrap()
        .clone();
    assert_eq!(frouard["deposees"], 1);
    assert_eq!(frouard["montant_depose"], 60.0);
    m.app.fermer().await;
}

#[tokio::test]
async fn fusion_de_fiches() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    let doc = facture(&m, &[m.boite_a], m.sa, "F-20").await;
    let r = app
        .requete(
            "POST",
            "/api/admin/fournisseurs",
            Some(json!({ "nom": "Durand SAS", "domaines": ["factures@durand.fr"], "statut": "valide" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let cible = r.json["id"].as_i64().unwrap();
    assert_eq!(r.json["domaines"], json!(["durand.fr"]));
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/fournisseurs/{}/fusionner", m.fournisseur),
            Some(json!({ "cible_id": cible })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["documents"], 1);
    let fiche = app
        .requete("GET", &format!("/api/fournisseurs/{cible}"), None, admin)
        .await;
    assert_eq!(fiche.json["siret"], "55210055400013", "identifiants repris");
    let d: Option<i64> = sqlx::query_scalar("select fournisseur_id from document where id = $1")
        .bind(doc)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(d, Some(cible));
    // La fiche cible est validée : le contrôle « fournisseur » ne bloque plus.
    assert_eq!(statut(app, doc).await.as_deref(), Some("pret"));
    // La fiche fusionnée disparaît de la liste.
    let r = app
        .requete("GET", "/api/fournisseurs", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(r.json["total"], 1);
    m.app.fermer().await;
}
