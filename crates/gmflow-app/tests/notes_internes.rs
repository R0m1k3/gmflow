//! Notes internes et motif d'écartement (audit U23) : notes signées, droits par société,
//! indicateur des listes, journal ; motif « publicité » et écartement par lot.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use gmflow_core::montant::{self, dec};
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
    c_resp_a2: String,
    c_resp_b: String,
    sa: i64,
    sb: i64,
    boite_a: i64,
    boite_b: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("La Foir'Fouille Frouard").await;
    let sb = app.societe("La Foir'Fouille Houdemont").await;
    sqlx::query("update societe set outil_depot = 'dext', adresse_depot = 'depot@dext.test'")
        .execute(app.pool())
        .await
        .unwrap();
    let boite_a = app.boite(sa, "Compta Frouard").await;
    let boite_b = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    app.utilisateur("resp-a2", MDP, "responsable", Some(sa))
        .await;
    app.utilisateur("resp-b", MDP, "responsable", Some(sb))
        .await;
    Monde {
        c_admin: app.connecter("admin", MDP).await,
        c_resp_a: app.connecter("resp-a", MDP).await,
        c_resp_a2: app.connecter("resp-a2", MDP).await,
        c_resp_b: app.connecter("resp-b", MDP).await,
        app,
        sa,
        sb,
        boite_a,
        boite_b,
    }
}

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

struct Piece<'a> {
    type_piece: &'a str,
    numero: &'a str,
    societe: Option<i64>,
    statut: Option<&'a str>,
    date: &'a str,
    ttc: f64,
    valide: bool,
}

/// Pièce lue reçue par `boite`, TVA à 20 %.
async fn piece(m: &Monde, boite: i64, p: Piece<'_>) -> i64 {
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        &format!("{}-{}@test", p.numero, suffixe()),
        "Facture",
        &[("f.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    let pj = app.piece_de(mail).await;
    let ttc = montant::depuis_f64(p.ttc).unwrap();
    let ht = montant::centimes(ttc / dec!(1.2));
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, date_document, date_echeance, montant_ht, montant_tva,
             montant_ttc, statut_depot, valide_le, verifie_le, confiance, texte, lu_le)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', $2, $3,
                'Durand', $4, $5::date, $5::date + 30, $6, $7 - $6, $7, $8,
                case when $9 then now() end, case when $9 then now() end,
                '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(pj)
    .bind(p.type_piece)
    .bind(p.societe)
    .bind(p.numero)
    .bind(p.date)
    .bind(ht)
    .bind(ttc)
    .bind(p.statut)
    .bind(p.valide)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

async fn facture_simple(m: &Monde, boite: i64, societe: i64, numero: &str) -> i64 {
    piece(
        m,
        boite,
        Piece {
            type_piece: "facture",
            numero,
            societe: Some(societe),
            statut: Some("a_verifier"),
            date: "2026-11-10",
            ttc: 60.0,
            valide: false,
        },
    )
    .await
}

fn ligne_liste(liste: &Value, id: i64) -> &Value {
    liste["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .expect("pièce dans la liste")
}

#[tokio::test]
async fn notes_internes_signees_droits_et_journal() {
    let m = monde().await;
    let app = &m.app;
    let f = facture_simple(&m, m.boite_a, m.sa, "N-1").await;
    let url = format!("/api/documents/{f}/notes");

    // Vide : refusé ; sans l'en-tête X-GMFlow : refusé.
    let r = app
        .requete(
            "POST",
            &url,
            Some(json!({ "texte": "   " })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "texte");
    let r = app
        .requete_brute(
            "POST",
            &url,
            Some(json!({ "texte": "x" })),
            Some(&m.c_resp_a),
            false,
        )
        .await;
    assert_ne!(r.code, StatusCode::CREATED);

    let r = app
        .requete(
            "POST",
            &url,
            Some(json!({ "texte": "  Appeler Durand : remise oubliée  " })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::CREATED, "{}", r.json);
    let n1 = r.json["id"].as_i64().unwrap();
    assert_eq!(r.json["texte"], "Appeler Durand : remise oubliée");
    assert_eq!(r.json["auteur_nom"], "resp-a");
    assert!(r.json["cree_le"].as_str().is_some());
    // Un collègue de la même société ajoute la sienne.
    let r = app
        .requete(
            "POST",
            &url,
            Some(json!({ "texte": "Fait, avoir demandé" })),
            Some(&m.c_resp_a2),
        )
        .await;
    assert_eq!(r.code, StatusCode::CREATED);
    let n2 = r.json["id"].as_i64().unwrap();

    // Liste : la plus récente d'abord, « supprimable » selon l'auteur.
    let r = app.requete("GET", &url, None, Some(&m.c_resp_a)).await;
    assert_eq!(r.code, StatusCode::OK);
    let notes = r.json.as_array().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0]["id"], n2);
    assert_eq!(notes[0]["supprimable"], false);
    assert_eq!(notes[1]["supprimable"], true);
    let r = app.requete("GET", &url, None, Some(&m.c_admin)).await;
    assert!(
        r.json
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["supprimable"] == true)
    );

    // Indicateur dans les listes : nombre et début de la dernière note.
    let liste = app
        .requete(
            "GET",
            "/api/documents?type=facture",
            None,
            Some(&m.c_resp_a),
        )
        .await
        .json;
    let l = ligne_liste(&liste, f);
    assert_eq!(l["notes"], 2);
    assert_eq!(l["derniere_note"], "Fait, avoir demandé");

    // Autre société : ni lecture, ni ajout, ni suppression.
    for (methode, chemin, corps) in [
        ("GET", url.clone(), None),
        ("POST", url.clone(), Some(json!({ "texte": "intrus" }))),
        ("DELETE", format!("/api/notes/{n1}"), None),
    ] {
        assert_eq!(
            app.requete(methode, &chemin, corps, Some(&m.c_resp_b))
                .await
                .code,
            StatusCode::NOT_FOUND,
            "{methode} {chemin}"
        );
    }
    // Le collègue ne supprime pas la note d'un autre ; l'auteur et l'admin le peuvent.
    assert_eq!(
        app.requete(
            "DELETE",
            &format!("/api/notes/{n1}"),
            None,
            Some(&m.c_resp_a2)
        )
        .await
        .code,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.requete(
            "DELETE",
            &format!("/api/notes/{n1}"),
            None,
            Some(&m.c_resp_a)
        )
        .await
        .code,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.requete(
            "DELETE",
            &format!("/api/notes/{n2}"),
            None,
            Some(&m.c_admin)
        )
        .await
        .code,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.requete(
            "DELETE",
            &format!("/api/notes/{n2}"),
            None,
            Some(&m.c_admin)
        )
        .await
        .code,
        StatusCode::NOT_FOUND
    );
    let liste = app
        .requete(
            "GET",
            "/api/documents?type=facture",
            None,
            Some(&m.c_resp_a),
        )
        .await
        .json;
    let l = ligne_liste(&liste, f);
    assert_eq!(l["notes"], 0);
    assert!(l["derniere_note"].is_null());

    assert_eq!(
        app.compter(&format!(
            "select count(*) from journal where action = 'note_ajoutee' and objet_id = '{f}'"
        ))
        .await,
        2
    );
    assert_eq!(
        app.compter(&format!(
            "select count(*) from journal where action = 'note_supprimee' and objet_id = '{f}'
               and details->>'texte' = 'Appeler Durand : remise oubliée'"
        ))
        .await,
        1
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn ecarter_avec_motif_publicite_et_par_lot() {
    let m = monde().await;
    let app = &m.app;
    let f1 = facture_simple(&m, m.boite_a, m.sa, "P-1").await;
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{f1}/ecarter"),
            Some(json!({ "motif": "publicite", "texte": "Catalogue de Noël" })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let fiche = app
        .requete(
            "GET",
            &format!("/api/documents/{f1}"),
            None,
            Some(&m.c_resp_a),
        )
        .await
        .json;
    assert_eq!(fiche["ecartement"]["motif"], "publicite");
    // Le motif est aussi dans le journal.
    assert_eq!(
        app.compter(&format!(
            "select count(*) from journal where action = 'document_ecarte' and objet_id = '{f1}'
               and details->>'motif' = 'publicite' and details->>'texte' = 'Catalogue de Noël'"
        ))
        .await,
        1
    );

    // Par lot : motif obligatoire, droits vérifiés pièce par pièce.
    let f2 = facture_simple(&m, m.boite_a, m.sa, "P-2").await;
    let f3 = facture_simple(&m, m.boite_a, m.sa, "P-3").await;
    let fb = facture_simple(&m, m.boite_b, m.sb, "P-B").await;
    let r = app
        .requete(
            "POST",
            "/api/documents/lot/ecarter",
            Some(json!({ "ids": [f2, f3] })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "motif");
    let r = app
        .requete(
            "POST",
            "/api/documents/lot/ecarter",
            Some(json!({ "ids": [f2, f3, fb, f1], "motif": "pas_pour_nous" })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["ecartees"], 2);
    assert_eq!(r.json["laissees"], 2);
    let raison = |id: i64| {
        r.json["pieces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()["raison"]
            .clone()
    };
    assert_eq!(raison(fb), "Introuvable");
    assert!(
        raison(f1).as_str().unwrap().contains("écarté"),
        "{}",
        raison(f1)
    );
    for id in [f2, f3] {
        let e: (String, Value) =
            sqlx::query_as("select etat, ecartement from document where id = $1")
                .bind(id)
                .fetch_one(app.pool())
                .await
                .unwrap();
        assert_eq!(e.0, "ignore");
        assert_eq!(e.1["motif"], "pas_pour_nous");
        assert_eq!(e.1["par_nom"], "resp-a");
    }
    let etat_b: String = sqlx::query_scalar("select etat from document where id = $1")
        .bind(fb)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(etat_b, "lu");
    m.app.fermer().await;
}
