//! IBAN des factures (fraude au RIB) : premier IBAN retenu à la validation, alerte « IBAN changé »
//! qui empêche le dépôt automatique, « C'est le bon IBAN » (administrateur), marques d'un groupe,
//! liste masquée sur la fiche fournisseur et retrait.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use gmflow_core::depot;
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";
const IBAN_1: &str = "FR7630006000011234567890189";
const IBAN_2: &str = "FR1420041010050500013M02606";
const IBAN_LT: &str = "LT121000011101001000";

struct Monde {
    app: Appli,
    admin: String,
    resp: String,
    sa: i64,
    boite: i64,
    fournisseur: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    sqlx::query(
        "update societe set siret = '12345678900014', adresse_depot = 'depot@dext.test',
             outil_depot = 'dext', depot_auto = true where id = $1",
    )
    .bind(sa)
    .execute(app.pool())
    .await
    .unwrap();
    let boite = app.boite(sa, "Compta Frouard").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp", MDP, "responsable", Some(sa)).await;
    let fournisseur = fournisseur(&app, "Grossiste Durand", None).await;
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::expedition::CLE,
        &json!({ "serveur": "smtp.test", "port": 465, "securite": "tls", "adresse": "gmflow@test.local" }),
    )
    .await
    .unwrap();
    let admin = app.connecter("admin", MDP).await;
    let resp = app.connecter("resp", MDP).await;
    Monde {
        app,
        admin,
        resp,
        sa,
        boite,
        fournisseur,
    }
}

async fn fournisseur(app: &Appli, nom: &str, groupe: Option<i64>) -> i64 {
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut, groupe_id)
         values ($1, lower($1), '55210055400013', 'valide', $2) returning id",
    )
    .bind(nom)
    .bind(groupe)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

/// Facture lue, complète (aucun autre contrôle en échec), avec cet IBAN lu (via `resultat`,
/// comme après une lecture).
async fn facture(m: &Monde, fournisseur: i64, numero: &str, iban: Option<&str>) -> i64 {
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        &format!("{numero}-{}@test", suffixe()),
        "Facture",
        &[("f.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[m.boite]).await;
    let piece = app.piece_de(mail).await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, fournisseur_id, numero, date_document, montant_ht, montant_tva,
             montant_ttc, confiance, texte, lu_le, resultat)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', 'facture', $2,
                'Durand', $3, $4, current_date, 50.00, 10.00, 60.00,
                '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now(), $5
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(m.sa)
    .bind(fournisseur)
    .bind(numero)
    .bind(json!({ "iban": iban, "bic": "AGRIFRPP" }))
    .fetch_one(app.pool())
    .await
    .unwrap();
    let mut conn = app.pool().acquire().await.unwrap();
    gmflow_core::iban::noter_lecture(&mut conn, id)
        .await
        .unwrap();
    id
}

async fn etat(app: &Appli, id: i64) -> (Option<String>, Value) {
    sqlx::query_as("select statut_depot, controles from document where id = $1")
        .bind(id)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

fn codes(controles: &Value) -> Vec<String> {
    controles
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap().to_string())
        .collect()
}

async fn connus(app: &Appli, fournisseur: i64) -> Vec<(String, String)> {
    sqlx::query_as(
        "select iban, origine from fournisseur_iban where fournisseur_id = $1 order by id",
    )
    .bind(fournisseur)
    .fetch_all(app.pool())
    .await
    .unwrap()
}

async fn envois_en_file(app: &Appli) -> i64 {
    app.compter(
        "select count(*) from apalis.jobs where job_type = 'envoi' and convert_from(job, 'UTF8') like '%Deposer%'",
    )
        .await
}

#[tokio::test]
async fn premier_iban_retenu_a_la_validation_puis_iban_change_en_alerte() {
    let m = monde().await;
    let app = &m.app;
    // Premier IBAN : aucune alerte, la facture est prête et part seule (dépôt automatique).
    let f1 = facture(&m, m.fournisseur, "F-1", Some(IBAN_1)).await;
    let avant = envois_en_file(app).await;
    assert_eq!(
        depot::evaluer_et_deposer(app.pool(), f1)
            .await
            .unwrap()
            .as_deref(),
        Some("pret")
    );
    assert_eq!(envois_en_file(app).await, avant + 1);
    // Lu seulement : pas encore connu.
    assert!(connus(app, m.fournisseur).await.is_empty());

    // Validation humaine d'une autre facture au même IBAN : il devient connu, journalisé.
    let f2 = facture(&m, m.fournisseur, "F-2", Some(IBAN_1)).await;
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f2}/valider"),
            Some(json!({})),
            Some(&m.resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        connus(app, m.fournisseur).await,
        [(IBAN_1.to_string(), "validation".to_string())]
    );
    assert_eq!(
        app.compter("select count(*) from journal where action = 'iban_ajoute' and details->>'iban' = 'FR76…0189'")
            .await,
        1
    );

    // IBAN différent : alerte forte, en tête, et pas de dépôt automatique.
    let f3 = facture(&m, m.fournisseur, "F-3", Some(IBAN_LT)).await;
    let avant = envois_en_file(app).await;
    assert_eq!(
        depot::evaluer_et_deposer(app.pool(), f3)
            .await
            .unwrap()
            .as_deref(),
        Some("a_verifier")
    );
    assert_eq!(envois_en_file(app).await, avant);
    let (_, controles) = etat(app, f3).await;
    assert_eq!(codes(&controles), ["iban"]);
    assert_eq!(
        controles[0]["message"],
        "IBAN différent de celui connu pour ce fournisseur (LT12…1000 au lieu de FR76…0189), et \
         dans un autre pays : Lituanie au lieu de France. Vérifiez par téléphone auprès du \
         fournisseur avant de payer"
    );

    // Valider la facture ne fait pas connaître le nouvel IBAN.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f3}/valider"),
            Some(json!({})),
            Some(&m.resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);

    // Une autre facture au nouvel IBAN : en alerte ; un responsable ne confirme pas.
    let f4 = facture(&m, m.fournisseur, "F-4", Some(IBAN_LT)).await;
    depot::evaluer(app.pool(), f4).await.unwrap();
    let confirmer = format!("/api/admin/documents/{f4}/iban/confirmer");
    assert_eq!(
        app.requete("POST", &confirmer, None, Some(&m.resp))
            .await
            .code,
        StatusCode::FORBIDDEN
    );
    // « C'est le bon IBAN » : connu, journalisé avec l'administrateur, alerte levée.
    let avant = envois_en_file(app).await;
    let r = app.requete("POST", &confirmer, None, Some(&m.admin)).await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["ajoute"], true);
    assert_eq!(r.json["statut_depot"], "pret");
    let (_, controles) = etat(app, f4).await;
    assert!(codes(&controles).is_empty());
    assert_eq!(
        connus(app, m.fournisseur).await[1],
        (IBAN_LT.to_string(), "confirmation".to_string())
    );
    assert_eq!(
        app.compter(
            "select count(*) from journal j join utilisateur u on u.id = j.auteur_id
             where j.action = 'iban_confirme' and u.identifiant = 'admin'"
        )
        .await,
        1
    );
    // Confirmé, il ne déclenche pas de dépôt automatique de lui-même.
    assert_eq!(envois_en_file(app).await, avant);

    // Fiche fournisseur : IBAN masqués, du plus récent au plus ancien ; retrait par l'admin.
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/ibans", m.fournisseur),
            None,
            Some(&m.resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let liste = r.json.as_array().unwrap();
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[0]["iban"], "LT12…1000");
    assert_eq!(liste[0]["pays"], "Lituanie");
    assert_eq!(liste[0]["ajoute_par"], "admin");
    assert_eq!(liste[1]["iban"], "FR76…0189");
    assert_eq!(liste[1]["origine"], "validation");
    assert_eq!(liste[1]["document_id"], f2);
    assert!(liste[1]["premiere_vue_le"].as_str().unwrap().len() == 10);
    assert!(!r.json.to_string().contains(IBAN_1));
    let retrait = format!(
        "/api/admin/fournisseurs/{}/ibans/{}",
        m.fournisseur, liste[0]["id"]
    );
    assert_eq!(
        app.requete("DELETE", &retrait, None, Some(&m.resp))
            .await
            .code,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.requete("DELETE", &retrait, None, Some(&m.admin))
            .await
            .code,
        StatusCode::NO_CONTENT
    );
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);
    assert_eq!(
        app.compter("select count(*) from journal where action = 'iban_retire'")
            .await,
        1
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn iban_a_la_cle_fausse_ignore_sans_alerte() {
    let m = monde().await;
    let app = &m.app;
    let f1 = facture(&m, m.fournisseur, "F-1", Some(IBAN_1)).await;
    sqlx::query("update document set verifie_le = now() where id = $1")
        .bind(f1)
        .execute(app.pool())
        .await
        .unwrap();
    depot::evaluer(app.pool(), f1).await.unwrap();
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);
    // Un chiffre mal lu : champ vide, aucune alerte de fraude.
    let f2 = facture(
        &m,
        m.fournisseur,
        "F-2",
        Some("FR7630006000011234567890188"),
    )
    .await;
    let iban: Option<String> = sqlx::query_scalar("select iban from document where id = $1")
        .bind(f2)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(iban, None);
    assert_eq!(
        depot::evaluer(app.pool(), f2).await.unwrap().as_deref(),
        Some("pret")
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn marques_d_un_groupe_partagent_les_iban_du_groupe() {
    let m = monde().await;
    let app = &m.app;
    let groupe = fournisseur(app, "U10", None).await;
    let l3c = fournisseur(app, "L3C", Some(groupe)).await;
    let homea = fournisseur(app, "Homea", Some(groupe)).await;
    // Première facture validée d'une marque : l'IBAN est appris au groupe.
    let f1 = facture(&m, l3c, "LC-1", Some(IBAN_1)).await;
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f1}/valider"),
            Some(json!({})),
            Some(&m.resp),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(connus(app, groupe).await.len(), 1);
    assert!(connus(app, l3c).await.is_empty());
    // Une autre marque du groupe, même IBAN : pas d'alerte ; autre IBAN : alerte.
    let f2 = facture(&m, homea, "LH-1", Some(IBAN_1)).await;
    assert_eq!(
        depot::evaluer(app.pool(), f2).await.unwrap().as_deref(),
        Some("pret")
    );
    let f3 = facture(&m, homea, "LH-2", Some(IBAN_2)).await;
    depot::evaluer(app.pool(), f3).await.unwrap();
    assert_eq!(codes(&etat(app, f3).await.1), ["iban"]);
    // La fiche de la marque montre l'IBAN du groupe.
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{homea}/ibans"),
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.json[0]["du_groupe"], true);
    // Confirmé sur la marque : appris au groupe, l'alerte tombe.
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/documents/{f3}/iban/confirmer"),
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(connus(app, groupe).await.len(), 2);
    m.app.fermer().await;
}

/// Validation différée et par lot (audit U1, U2) avec l'IBAN : une facture en alerte « IBAN
/// changé » n'est ni cochable ni validée dans un lot ; un premier IBAN appris par une validation
/// annulée n'est plus connu.
#[tokio::test]
async fn iban_change_hors_lot_et_premier_iban_oublie_si_validation_annulee() {
    let m = monde().await;
    let app = &m.app;
    let resp = Some(m.resp.as_str());

    // Premier IBAN appris par la validation, puis « Annuler » : plus connu, journalisé.
    let f1 = facture(&m, m.fournisseur, "A-1", Some(IBAN_1)).await;
    depot::evaluer(app.pool(), f1).await.unwrap();
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f1}/valider"),
            Some(json!({})),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);
    let lot = r.json["annulation"]["lot"].as_str().unwrap().to_string();
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            resp,
        )
        .await;
    assert_eq!(r.json["annulees"], 1, "{:?}", r.json);
    assert!(
        connus(app, m.fournisseur).await.is_empty(),
        "IBAN appris par une validation annulée"
    );
    assert_eq!(
        app.compter(
            "select count(*) from journal where action = 'validation_annulee'
             and details->'ibans_oublies' = '[\"FR76…0189\"]'"
        )
        .await,
        1
    );

    // Validée de nouveau puis prise par le worker : l'IBAN reste connu.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f1}/valider"),
            Some(json!({})),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    sqlx::query("update envoi_prevu set etat = 'pris' where document_id = $1 and etat = 'attente'")
        .bind(f1)
        .execute(app.pool())
        .await
        .unwrap();
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);

    // IBAN changé : alerte, pas de dépôt automatique, ni cochable ni validée dans un lot.
    let f2 = facture(&m, m.fournisseur, "A-2", Some(IBAN_2)).await;
    let ok = facture(&m, m.fournisseur, "A-3", Some(IBAN_1)).await;
    let avant = envois_en_file(app).await;
    for f in [f2, ok] {
        depot::evaluer_et_deposer(app.pool(), f).await.unwrap();
    }
    // Seule la facture sans alerte part d'elle-même (dépôt automatique de la société).
    assert_eq!(envois_en_file(app).await, avant + 1);
    assert_eq!(codes(&etat(app, f2).await.1), ["iban"]);
    let ids = json!({ "ids": [f2, ok] });
    let r = app
        .requete(
            "POST",
            "/api/documents/lot/eligibilite",
            Some(ids.clone()),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    let el = r.json.as_array().unwrap();
    let de = |id: i64| el.iter().find(|e| e["id"] == id).unwrap().clone();
    assert_eq!(de(f2)["possible"], false);
    assert_eq!(de(f2)["raison"], "1 alerte à lire");
    assert_eq!(de(ok)["possible"], true);
    let r = app
        .requete("POST", "/api/documents/lot/valider", Some(ids), resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["validees"], 1, "{:?}", r.json);
    let valide: bool =
        sqlx::query_scalar("select valide_le is not null from document where id = $1")
            .bind(f2)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert!(!valide);
    assert_eq!(connus(app, m.fournisseur).await.len(), 1);

    // Même lot, nouveau fournisseur sans IBAN connu, deux IBAN différents : la première
    // validation apprend le sien, la seconde est alors en alerte et reste à contrôler, même si
    // ses contrôles enregistrés n'avaient encore rien vu.
    let autre = fournisseur(app, "Autre fournisseur", None).await;
    let g1 = facture(&m, autre, "B-1", Some(IBAN_1)).await;
    let g2 = facture(&m, autre, "B-2", Some(IBAN_LT)).await;
    for g in [g1, g2] {
        assert_eq!(
            depot::evaluer(app.pool(), g).await.unwrap().as_deref(),
            Some("pret")
        );
    }
    let r = app
        .requete(
            "POST",
            "/api/documents/lot/valider",
            Some(json!({ "ids": [g1, g2] })),
            resp,
        )
        .await;
    assert_eq!(r.json["validees"], 1, "{:?}", r.json);
    let piece = |id: i64| {
        r.json["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(piece(g1)["validee"], true);
    assert_eq!(piece(g2)["validee"], false);
    assert_eq!(connus(app, autre).await.len(), 1);
    // Le lot annulé : l'IBAN appris par g1 est oublié.
    let lot = r.json["annulation"]["lot"].as_str().unwrap().to_string();
    let r = app
        .requete(
            "POST",
            "/api/documents/annuler-envoi",
            Some(json!({ "lot": lot })),
            resp,
        )
        .await;
    assert_eq!(r.json["annulees"], 1, "{:?}", r.json);
    assert!(connus(app, autre).await.is_empty());
    m.app.fermer().await;
}
