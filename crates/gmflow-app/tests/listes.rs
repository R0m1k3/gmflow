//! Listes et accueil (chantier interface) : tri des listes par liste blanche, pagination sans
//! recompter, filtres « BL sans facture » et « échéance proche », file « À valider » par
//! urgence, chiffres de l'accueil, historique des corrections de la fiche, codes d'erreur.

mod commun;

use axum::http::StatusCode;
use commun::Appli;
use serde_json::{Value, json};

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    admin: String,
    sa: i64,
    boite: i64,
    /// Fiche fournisseur valide, rattachée aux factures (sans elle, alerte « fournisseur »).
    durand: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    sqlx::query(
        "update societe set siret = '12345678900014', adresse_depot = 'depot@dext.test', outil_depot = 'dext'",
    )
    .execute(app.pool())
    .await
    .unwrap();
    let boite = app.boite(sa, "Compta Frouard").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    let admin = app.connecter("admin", MDP).await;
    let durand: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut)
         values ('Grossiste Durand', 'grossiste durand', '55210055400013', 'valide') returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    Monde {
        app,
        admin,
        sa,
        boite,
        durand,
    }
}

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

/// Document lu reçu il y a `jours` jours (contrôles évalués).
async fn doc(m: &Monde, type_doc: &str, fournisseur: &str, ttc: f64, jours: i32) -> i64 {
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        &format!("l{}@test", suffixe()),
        &format!("Pièce {fournisseur}"),
        &[("piece.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[m.boite]).await;
    sqlx::query("update mail set date_mail = now() - make_interval(days => $2) where id = $1")
        .bind(mail)
        .bind(jours)
        .execute(app.pool())
        .await
        .unwrap();
    let piece = app.piece_de(mail).await;
    let ht = (ttc / 1.2 * 100.0).round() / 100.0;
    let tva = ((ttc - ht) * 100.0).round() / 100.0;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, fournisseur_id, numero, date_document, montant_ht, montant_tva, montant_ttc,
             confiance, texte, lu_le)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', $2, $3,
                $4, case when $2 = 'facture' then $9 end, 'N' || id, current_date - $5, $6::float8::numeric(14, 2),
                $7::float8::numeric(14, 2), $8::float8::numeric(14, 2),
                '{\"type\": 0.95, \"societe\": 0.95, \"numero\": 0.4}', 'TEXTE', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(type_doc)
    .bind(m.sa)
    .bind(fournisseur)
    .bind(jours)
    .bind(ht)
    .bind(tva)
    .bind(ttc)
    .bind(m.durand)
    .fetch_one(app.pool())
    .await
    .unwrap();
    gmflow_core::depot::evaluer(app.pool(), id).await.unwrap();
    id
}

fn ids(v: &Value) -> Vec<i64> {
    v["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn tri_des_documents_par_liste_blanche() {
    let m = monde().await;
    let app = &m.app;
    let a = doc(&m, "facture", "Bernard", 300.0, 3).await;
    let b = doc(&m, "facture", "alpha", 120.0, 1).await;
    let c = doc(&m, "facture", "Céline", 900.0, 2).await;
    // Tri par le nom lu (sans fiche rattachée, c'est lui que la liste montre).
    sqlx::query("update document set fournisseur_id = null")
        .execute(app.pool())
        .await
        .unwrap();
    let lire = |q: &'static str| {
        let c = m.admin.clone();
        async move {
            app.requete(
                "GET",
                &format!("/api/documents?type=facture{q}"),
                None,
                Some(&c),
            )
            .await
        }
    };
    // Par défaut : les plus récents reçus d'abord.
    assert_eq!(ids(&lire("").await.json), vec![b, c, a]);
    // Montant : du plus grand au plus petit au premier clic, puis l'inverse.
    assert_eq!(ids(&lire("&tri=montant").await.json), vec![c, a, b]);
    assert_eq!(
        ids(&lire("&tri=montant&sens=asc").await.json),
        vec![b, a, c]
    );
    // Fournisseur : de A à Z sans tenir compte des majuscules.
    assert_eq!(ids(&lire("&tri=fournisseur").await.json), vec![b, a, c]);
    assert_eq!(
        ids(&lire("&tri=fournisseur&sens=desc").await.json),
        vec![c, a, b]
    );
    assert_eq!(ids(&lire("&tri=recu&sens=asc").await.json), vec![a, c, b]);
    // Hors liste blanche : refusé, rien n'atteint le SQL.
    let r = lire("&tri=d.id;drop%20table%20document").await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["code"], "champ");
    assert_eq!(r.json["champ"], "tri");
    let r = lire("&tri=montant&sens=haut").await;
    assert_eq!(r.json["champ"], "sens");
    assert_eq!(app.compter("select count(*) from document").await, 3);
}

#[tokio::test]
async fn pages_suivantes_sans_recompter() {
    let m = monde().await;
    let app = &m.app;
    for i in 0..52 {
        doc(&m, "facture", &format!("F{i:02}"), 10.0 + f64::from(i), i).await;
    }
    let r = app
        .requete(
            "GET",
            "/api/documents?type=facture&tri=montant&sens=asc",
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.json["total"], 52);
    assert_eq!(r.json["suivante"], true);
    assert_eq!(r.json["elements"].as_array().unwrap().len(), 50);
    assert_eq!(r.json["elements"][0]["montant_ttc"], 10.0);
    let r = app
        .requete(
            "GET",
            "/api/documents?type=facture&tri=montant&sens=asc&page=2",
            None,
            Some(&m.admin),
        )
        .await;
    // Page 2 : pas de nouveau comptage, la suite garde le même tri.
    assert_eq!(r.json["total"], Value::Null);
    assert_eq!(r.json["suivante"], false);
    let montants: Vec<f64> = r.json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["montant_ttc"].as_f64().unwrap())
        .collect();
    assert_eq!(montants, vec![60.0, 61.0]);
    // Mails et fournisseurs : même forme de page.
    let r = app
        .requete("GET", "/api/mails?page=2&tri=objet", None, Some(&m.admin))
        .await;
    assert_eq!(r.json["total"], Value::Null);
    assert_eq!(r.json["elements"].as_array().unwrap().len(), 2);
    let r = app
        .requete("GET", "/api/mails?tri=objet&sens=asc", None, Some(&m.admin))
        .await;
    assert_eq!(r.json["elements"][0]["objet"], "Pièce F00");
    assert_eq!(r.json["suivante"], true);
    let r = app
        .requete("GET", "/api/fournisseurs?tri=inconnu", None, Some(&m.admin))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    let r = app
        .requete("GET", "/api/fournisseurs?tri=nom", None, Some(&m.admin))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["suivante"], false);
}

#[tokio::test]
async fn accueil_par_urgence() {
    let m = monde().await;
    let app = &m.app;
    // Une facture sans alerte, une avec alerte (montants faux), une à payer dans 3 jours
    // (reçue la plus récente), un BL sans facture depuis 10 jours et un depuis 2 jours.
    let simple = doc(&m, "facture", "Simple", 120.0, 5).await;
    let alerte = doc(&m, "facture", "Alerte", 120.0, 6).await;
    sqlx::query("update document set montant_ttc = 999 where id = $1")
        .bind(alerte)
        .execute(app.pool())
        .await
        .unwrap();
    gmflow_core::depot::evaluer(app.pool(), alerte)
        .await
        .unwrap();
    let urgente = doc(&m, "facture", "Urgente", 240.0, 1).await;
    sqlx::query("update document set date_echeance = current_date + 3 where id = $1")
        .bind(urgente)
        .execute(app.pool())
        .await
        .unwrap();
    let vieux_bl = doc(&m, "bl", "Vieux BL", 50.0, 10).await;
    let bl_recent = doc(&m, "bl", "BL récent", 50.0, 2).await;
    // Une facture sans alerte mais en écart d'articles avec son BL : pas « validable d'un clic ».
    let ecart = doc(&m, "facture", "Écart", 130.0, 4).await;
    let bl_ecart = doc(&m, "bl", "BL écart", 50.0, 3).await;
    sqlx::query(
        "insert into rapprochement (facture_id, bl_id, methode, ecarts)
         values ($1, $2, 'manuel', '[{\"reference\": \"A1\", \"qte_bl\": 1, \"qte_facture\": 2}]')",
    )
    .bind(ecart)
    .bind(bl_ecart)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("update document set fournisseur_id = null where type = 'bl'")
        .execute(app.pool())
        .await
        .unwrap();

    let r = app
        .requete("GET", "/api/a-traiter", None, Some(&m.admin))
        .await;
    let file: Vec<(i64, i64)> = r.json["a_valider"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["id"].as_i64().unwrap(), e["rang"].as_i64().unwrap()))
        .collect();
    // Échéance proche, puis validable d'un clic, puis à corriger (les BL sans fournisseur
    // identifié ont une alerte, la facture en écart BL), les plus anciens d'abord dans chaque rang.
    assert_eq!(
        file,
        vec![
            (urgente, 1),
            (simple, 2),
            (vieux_bl, 3),
            (alerte, 3),
            (ecart, 3),
            (bl_ecart, 3),
            (bl_recent, 3)
        ]
    );
    assert_eq!(r.json["suivant"], urgente);
    assert!(r.json["a_valider"][0]["date_echeance"].is_string());

    // Valider la première rend la suivante dans le même ordre.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{simple}/verifier"),
            Some(json!({ "type": "facture", "societe_id": m.sa })),
            Some(&m.admin),
        )
        .await;
    assert!(r.code.is_success(), "{:?}", r.json);
    let place = gmflow_app::routes::a_traiter::place_dans_file(&app.etat, urgente)
        .await
        .unwrap();
    assert_eq!(place.map(|p| p.0), Some(1));

    let r = app
        .requete("GET", "/api/tableau-de-bord", None, Some(&m.admin))
        .await;
    let t = &r.json["travail"];
    assert_eq!(t["factures_ttc"], 120.0 + 999.0 + 240.0 + 130.0, "{t}");
    assert_eq!(t["echeances"], 1);
    assert_eq!(t["bl_sans_facture"], 1, "{t}");

    // Les mêmes listes, filtrées, derrière chaque chiffre.
    let r = app
        .requete(
            "GET",
            "/api/documents?type=bl&sans_facture=7",
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(ids(&r.json), vec![vieux_bl]);
    let r = app
        .requete(
            "GET",
            "/api/documents?type=facture&echeance=7&verifie=non",
            None,
            Some(&m.admin),
        )
        .await;
    assert_eq!(ids(&r.json), vec![urgente]);
}

#[tokio::test]
async fn fiche_dit_qui_a_corrige_quoi() {
    let m = monde().await;
    let app = &m.app;
    let id = doc(&m, "facture", "Durand", 120.0, 1).await;
    let r = app
        .requete("GET", &format!("/api/documents/{id}"), None, Some(&m.admin))
        .await;
    assert_eq!(r.json["historique"], json!([]));
    let corps = |numero: &str, ttc: f64| {
        json!({ "type": "facture", "societe_id": m.sa, "numero": numero,
                "montant_ht": 100.0, "montant_tva": 20.0, "montant_ttc": ttc })
    };
    let r = app
        .requete(
            "PUT",
            &format!("/api/documents/{id}"),
            Some(corps("F-1", 120.0)),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT, "{:?}", r.json);
    let r = app
        .requete(
            "PUT",
            &format!("/api/documents/{id}"),
            Some(corps("F-2", 120.0)),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::NO_CONTENT);
    let r = app
        .requete("GET", &format!("/api/documents/{id}"), None, Some(&m.admin))
        .await;
    let h = r.json["historique"].as_array().unwrap();
    let numeros: Vec<(String, String, String)> = h
        .iter()
        .filter(|c| c["champ"] == "numero")
        .map(|c| {
            let t = |k: &str| c[k].as_str().unwrap().to_string();
            (t("avant"), t("apres"), t("par"))
        })
        .collect();
    let lu = format!("N{}", piece_de(app, id).await);
    let t = |a: &str, b: &str| (a.to_string(), b.to_string(), "admin".to_string());
    assert_eq!(numeros, vec![t(&lu, "F-1"), t("F-1", "F-2")]);
    assert!(h.iter().all(|c| c["quand"].is_string()));
    // L'ordre : de la plus ancienne à la plus récente.
    assert_eq!(h.last().unwrap()["apres"], "F-2");
}

async fn piece_de(app: &Appli, document: i64) -> i64 {
    sqlx::query_scalar("select piece_jointe_id from document where id = $1")
        .bind(document)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn refus_de_reglage_absent_avec_code() {
    let m = monde().await;
    let app = &m.app;
    let id = doc(&m, "facture", "Durand", 120.0, 1).await;
    // Sans adresse d'expédition : le front propose d'aller aux réglages de l'expédition.
    let r = app
        .requete(
            "POST",
            &format!("/api/documents/{id}/deposer"),
            Some(json!({})),
            Some(&m.admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert_eq!(r.json["code"], "expedition");
    let r = app
        .requete("GET", "/api/documents/999999", None, Some(&m.admin))
        .await;
    assert_eq!(r.json["code"], "absent");
}

/// C19 : la recherche des listes se sert des index trigrammes (preuve par EXPLAIN). Les tables
/// de test sont minuscules : sans `enable_seqscan = off`, PostgreSQL les lirait en entier de
/// toute façon ; ici on vérifie que le plan *peut* passer par les index, avec la même forme de
/// filtre que `documents::lister` et `mails::lister`.
#[tokio::test]
async fn recherche_par_index_trigramme() {
    let m = monde().await;
    // La recherche trouve toujours par fournisseur, numéro et objet du mail.
    let alpha = doc(&m, "facture", "Alpha Négoce", 10.0, 1).await;
    let beta = doc(&m, "facture", "Beta", 20.0, 2).await;
    sqlx::query("update mail set objet = 'Relance urgente' where id = (select mail_id from document where id = $1)")
        .bind(beta)
        .execute(m.app.pool())
        .await
        .unwrap();
    for (q, attendu) in [
        ("alpha".to_string(), vec![alpha]),
        ("relance".to_string(), vec![beta]),
        (format!("N{beta}"), vec![beta]),
    ] {
        let r = m
            .app
            .requete(
                "GET",
                &format!("/api/documents?q={q}"),
                None,
                Some(&m.admin),
            )
            .await;
        assert_eq!(r.code, StatusCode::OK);
        assert_eq!(ids(&r.json), attendu, "q={q}");
    }
    let mut tx = m.app.pool().begin().await.unwrap();
    sqlx::query("set local enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan = |lignes: Vec<String>| lignes.join("\n");
    let documents: Vec<String> = sqlx::query_scalar(
        "explain select d.id from document d join mail m on m.id = d.mail_id
         where ($1::text is null
                or d.id in (select x.id from document x where x.fournisseur_nom ilike $1 or x.numero ilike $1
                            union all
                            select x.id from document x join mail mx on mx.id = x.mail_id where mx.objet ilike $1))",
    )
    .bind("%durand%")
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let documents = plan(documents);
    for index in [
        "document_fournisseur_nom_trgm",
        "document_numero_trgm",
        "mail_objet_trgm",
    ] {
        assert!(documents.contains(index), "{index} absent :\n{documents}");
    }
    let mails: Vec<String> = sqlx::query_scalar(
        "explain select m.id from mail m
         where ($1::text is null or m.expediteur ilike $1 or m.objet ilike $1)",
    )
    .bind("%durand%")
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let mails = plan(mails);
    for index in ["mail_expediteur_trgm", "mail_objet_trgm"] {
        assert!(mails.contains(index), "{index} absent :\n{mails}");
    }
}
