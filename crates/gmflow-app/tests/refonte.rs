//! Endpoints de la refonte de l'interface : aperçu des pièces, file « À traiter », validation
//! en un appel, fournisseurs par société et fil, recherche unique, mise en route et essais de
//! boîte, bilan du mois. Périmètre d'un responsable compris.

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
    sa: i64,
    sb: i64,
    boite_a: i64,
    boite_b: i64,
    durand: i64,
    martin: i64,
}

async fn monde() -> Monde {
    let app = Appli::new().await;
    let sa = app.societe("Frouard").await;
    let sb = app.societe("Houdemont").await;
    sqlx::query(
        "update societe set siret = '12345678900014', adresse_depot = 'depot-' || id || '@dext.test',
                            outil_depot = 'dext'",
    )
    .execute(app.pool())
    .await
    .unwrap();
    let boite_a = app.boite(sa, "Compta Frouard").await;
    let boite_b = app.boite(sb, "Compta Houdemont").await;
    app.utilisateur("admin", MDP, "admin", None).await;
    app.utilisateur("resp-a", MDP, "responsable", Some(sa))
        .await;
    let durand: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut, domaines)
         values ('Grossiste Durand', 'grossiste durand', '55210055400013', 'valide', '{fournisseur.test}')
         returning id",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    let martin: i64 = sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut)
         values ('Martin Frais', 'martin frais', '55210055400013', 'valide') returning id",
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
        durand,
        martin,
    }
}

fn suffixe() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::SeqCst)
}

/// Mail reçu il y a `jours` jours dans `boite`, avec une pièce PDF. Rend (mail, pièce).
async fn mail(app: &Appli, boite: i64, objet: &str, jours: i64) -> (i64, i64) {
    let eml = gmflow_core::fixtures::eml(
        &format!("m{}-{}@test", suffixe(), jours),
        objet,
        &[("piece.pdf", "application/pdf", b"%PDF-1.4 test")],
        false,
    );
    let mail = app.mail_vu(&eml, &[boite]).await;
    sqlx::query("update mail set date_mail = now() - make_interval(days => $2::int) where id = $1")
        .bind(mail)
        .bind(jours as i32)
        .execute(app.pool())
        .await
        .unwrap();
    (mail, app.piece_de(mail).await)
}

/// Document lu (contrôles évalués). `ttc` : HT = TTC / 1,2, TVA = le reste.
#[allow(clippy::too_many_arguments)]
async fn doc(
    m: &Monde,
    boite: i64,
    societe: Option<i64>,
    type_doc: &str,
    numero: Option<&str>,
    ttc: f64,
    jours: i64,
    fournisseur: Option<i64>,
) -> i64 {
    let app = &m.app;
    let (_, piece) = mail(app, boite, "Document", jours).await;
    let ttc = montant::depuis_f64(ttc).unwrap();
    let ht = montant::centimes(ttc / dec!(1.2));
    let tva = ttc - ht;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, fournisseur_id, numero, date_document, montant_ht, montant_tva,
             montant_ttc, confiance, texte, lu_le)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'lu', 'pdf_texte', $2, $3,
                'Durand', $4, $5, current_date - $6::int, $7::numeric(14, 2),
                $8::numeric(14, 2), $9::numeric(14, 2),
                '{\"type\": 0.95, \"societe\": 0.95}', 'TEXTE', now()
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .bind(type_doc)
    .bind(societe)
    .bind(fournisseur)
    .bind(numero)
    .bind(jours as i32)
    .bind(ht)
    .bind(tva)
    .bind(ttc)
    .fetch_one(app.pool())
    .await
    .unwrap();
    gmflow_core::depot::evaluer(app.pool(), id).await.unwrap();
    id
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

fn ids(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect()
}

async fn texte(app: &Appli, requete: &str) -> Option<String> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(requete.to_string()))
        .fetch_one(app.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn apercu_des_pieces() {
    let m = monde().await;
    let app = &m.app;
    let eml = gmflow_core::fixtures::eml(
        "apercu@test",
        "Pièces",
        &[
            ("f.pdf", "application/pdf", b"%PDF-1.4 test"),
            ("p.html", "text/html", b"<script>alert(1)</script>"),
            ("i.svg", "image/svg+xml", b"<svg onload='alert(1)'/>"),
            ("photo.png", "image/png", b"\x89PNG\r\n\x1a\n"),
            ("facture", "application/octet-stream", b"%PDF-1.7 octet"),
            (
                "faux.pdf",
                "application/pdf",
                b"<html><script>alert(1)</script>",
            ),
        ],
        false,
    );
    let mail = app.mail_vu(&eml, &[m.boite_a]).await;
    let pieces: Vec<(i64, String)> =
        sqlx::query_as("select id, nom from piece_jointe where mail_id = $1 order by rang")
            .bind(mail)
            .fetch_all(app.pool())
            .await
            .unwrap();
    let id = |nom: &str| pieces.iter().find(|p| p.1 == nom).unwrap().0;
    let admin = Some(m.c_admin.as_str());
    let entete = |r: &commun::Reponse, n: &str| {
        r.entetes
            .get(n)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    };

    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{}?inline=1", id("f.pdf")),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert!(entete(&r, "content-disposition").starts_with("inline;"));
    assert_eq!(entete(&r, "content-type"), "application/pdf");
    assert_eq!(entete(&r, "x-content-type-options"), "nosniff");
    let csp = entete(&r, "content-security-policy");
    assert!(csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'self'"));
    assert!(!csp.contains("sandbox"), "le lecteur PDF refuse sandbox");
    assert_eq!(r.octets, b"%PDF-1.4 test");

    // Sans `inline` : téléchargement, comme avant.
    let r = app
        .requete("GET", &format!("/api/pieces/{}", id("f.pdf")), None, admin)
        .await;
    assert!(entete(&r, "content-disposition").starts_with("attachment;"));

    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{}?inline=1", id("photo.png")),
            None,
            admin,
        )
        .await;
    assert!(entete(&r, "content-disposition").starts_with("inline;"));
    assert!(entete(&r, "content-security-policy").contains("sandbox"));

    // PDF envoyé en octet-stream, sans extension : reconnu à son contenu.
    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{}?inline=1", id("facture")),
            None,
            admin,
        )
        .await;
    assert!(entete(&r, "content-disposition").starts_with("inline;"));
    assert_eq!(entete(&r, "content-type"), "application/pdf");

    // HTML et SVG (même déclarés PDF) : jamais affichés dans la page.
    for nom in ["p.html", "i.svg", "faux.pdf"] {
        let r = app
            .requete(
                "GET",
                &format!("/api/pieces/{}?inline=1", id(nom)),
                None,
                admin,
            )
            .await;
        assert_eq!(r.code, StatusCode::OK);
        assert!(
            entete(&r, "content-disposition").starts_with("attachment;"),
            "{nom}"
        );
        let csp = entete(&r, "content-security-policy");
        assert!(csp.contains("sandbox") && csp.contains("default-src 'none'"));
    }

    // Pièce d'une autre société : invisible pour un responsable.
    let eml_b = gmflow_core::fixtures::eml(
        "apercu-b@test",
        "Pièce B",
        &[("b.pdf", "application/pdf", b"%PDF-1.4 b")],
        false,
    );
    let mail_b = app.mail_vu(&eml_b, &[m.boite_b]).await;
    let piece_b = app.piece_de(mail_b).await;
    let r = app
        .requete(
            "GET",
            &format!("/api/pieces/{piece_b}?inline=1"),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    m.app.fermer().await;
}

/// Monde de la file : rend les documents dans un ordre nommé.
struct File {
    f1: i64,
    f2: i64,
    bl1: i64,
    fb: i64,
    fnull: i64,
    fother: i64,
    err: i64,
    dbl: i64,
    envoi_err: i64,
    mail_a: i64,
    mail_b: i64,
}

async fn remplir(m: &Monde) -> File {
    let app = &m.app;
    let d = Some(m.durand);
    let f1 = doc(m, m.boite_a, Some(m.sa), "facture", Some("F-1"), 60.0, 5, d).await;
    let f2 = doc(m, m.boite_a, Some(m.sa), "facture", None, 120.0, 1, d).await;
    let bl1 = doc(m, m.boite_a, Some(m.sa), "bl", Some("BL-1"), 60.0, 3, d).await;
    let fb = doc(m, m.boite_b, Some(m.sb), "facture", Some("F-B"), 30.0, 6, d).await;
    let fnull = doc(m, m.boite_a, None, "facture", Some("F-N"), 36.0, 2, d).await;
    let fother = doc(m, m.boite_a, Some(m.sb), "facture", Some("F-O"), 48.0, 4, d).await;
    // Hors file : BL déjà vérifié, facture déjà déposée.
    let bl_vu = doc(m, m.boite_a, Some(m.sa), "bl", Some("BL-V"), 60.0, 8, d).await;
    sqlx::query("update document set verifie_le = now() where id = $1")
        .bind(bl_vu)
        .execute(app.pool())
        .await
        .unwrap();
    let deposee = doc(m, m.boite_a, Some(m.sa), "facture", Some("F-D"), 72.0, 9, d).await;
    sqlx::query("update document set statut_depot = 'depose' where id = $1")
        .bind(deposee)
        .execute(app.pool())
        .await
        .unwrap();
    // Bloqué : illisible, doublon de F-1, envoi en échec.
    let (_, piece) = mail(app, m.boite_a, "Illisible", 7).await;
    let err: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, message)
         select id, mail_id, decode(md5(random()::text), 'hex'), 'erreur', 'PDF illisible'
         from piece_jointe where id = $1 returning id",
    )
    .bind(piece)
    .fetch_one(app.pool())
    .await
    .unwrap();
    let dbl = doc(m, m.boite_a, Some(m.sa), "facture", Some("F-1"), 60.0, 4, d).await;
    let envoi_err = doc(
        m,
        m.boite_a,
        Some(m.sa),
        "facture",
        Some("F-E"),
        84.0,
        10,
        d,
    )
    .await;
    sqlx::query("update document set statut_depot = 'erreur' where id = $1")
        .bind(envoi_err)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into envoi_depot (document_id, societe_id, adresse, etat, message)
         values ($1, $2, 'depot@dext.test', 'erreur', 'refus du serveur SMTP')",
    )
    .bind(envoi_err)
    .bind(m.sa)
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("update boite_mail set pause_auto = true, derniere_erreur = 'refus' where id = $1")
        .bind(m.boite_b)
        .execute(app.pool())
        .await
        .unwrap();
    // À répondre : une question dans chaque société.
    let (mail_a, _) = mail(app, m.boite_a, "Question RIB", 2).await;
    let (mail_b, _) = mail(app, m.boite_b, "Question B", 3).await;
    for (id, p) in [(mail_a, "haute"), (mail_b, "normale")] {
        sqlx::query(
            "update mail set categorie = 'question', priorite = $2, resume = 'Demande le RIB',
                 statut_reponse = 'a_repondre' where id = $1",
        )
        .bind(id)
        .bind(p)
        .execute(app.pool())
        .await
        .unwrap();
    }
    File {
        f1,
        f2,
        bl1,
        fb,
        fnull,
        fother,
        err,
        dbl,
        envoi_err,
        mail_a,
        mail_b,
    }
}

#[tokio::test]
async fn file_a_traiter_et_compteurs() {
    let m = monde().await;
    let app = &m.app;
    let f = remplir(&m).await;
    assert_eq!(
        texte(
            app,
            &format!("select statut_depot from document where id = {}", f.dbl)
        )
        .await,
        Some("doublon".into())
    );

    let r = app
        .requete("GET", "/api/a-traiter", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    // Par urgence : validables d'un clic d'abord, puis à corriger (société inconnue, numéro
    // manquant), les plus anciens d'abord dans chaque rang.
    assert_eq!(
        ids(&r.json["a_valider"]),
        vec![f.fb, f.f1, f.fother, f.bl1, f.fnull, f.f2]
    );
    assert_eq!(r.json["suivant"], f.fb);
    assert_eq!(r.json["a_valider"][0]["rang"], 2);
    let premier = &r.json["a_valider"][4];
    assert_eq!(premier["rang"], 3);
    assert_eq!(premier["alerte"], "Société destinataire non reconnue");
    assert_eq!(premier["alerte_code"], "societe");
    let bl = r.json["a_valider"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == f.bl1)
        .unwrap();
    assert_eq!(bl["alerte_code"], Value::Null);
    assert_eq!(premier["fournisseur"], "Grossiste Durand");
    assert_eq!(premier["montant_ttc"], 36.0);
    let genres: Vec<&str> = r.json["bloque"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["genre"].as_str().unwrap())
        .collect();
    assert_eq!(
        genres,
        vec!["claude", "boite", "envoi_erreur", "doublon", "illisible"]
    );
    let bloque = r.json["bloque"].as_array().unwrap();
    assert_eq!(bloque[1]["boite_id"], m.boite_b);
    assert_eq!(bloque[1]["motif"], "pause_auto");
    assert_eq!(bloque[2]["document_id"], f.envoi_err);
    assert_eq!(bloque[2]["message"], "refus du serveur SMTP");
    assert_eq!(bloque[3]["document_id"], f.dbl);
    assert!(
        bloque[3]["message"]
            .as_str()
            .unwrap()
            .contains(&f.f1.to_string())
    );
    assert_eq!(bloque[4]["document_id"], f.err);
    assert_eq!(bloque[4]["message"], "PDF illisible");
    assert_eq!(ids(&r.json["a_repondre"]), vec![f.mail_a, f.mail_b]);
    assert_eq!(r.json["a_repondre"][0]["resume"], "Demande le RIB");
    // Fournisseur reconnu par le domaine de l'expéditeur, dans la file et dans la fiche.
    assert_eq!(r.json["a_repondre"][0]["fournisseur"], "Grossiste Durand");
    let fiche = app
        .requete(
            "GET",
            &format!("/api/mails/{}", f.mail_a),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(
        fiche.json["fournisseur"], "Grossiste Durand",
        "{}",
        fiche.json
    );
    assert_eq!(fiche.json["fournisseur_id"], json!(m.durand));
    assert_eq!(
        r.json["compteurs"],
        json!({
            "bloque": 5,
            "a_valider": 6,
            "a_valider_types": { "facture": 5, "avoir": 0, "bl": 1 },
            "a_repondre": 2,
            "total": 13
        })
    );
    // Le filtre « À valider » des listes (badges du menu) compte comme la file.
    for (t, n) in [("facture", 5), ("bl", 1)] {
        let l = app
            .requete(
                "GET",
                &format!("/api/documents?type={t}&verifie=non"),
                None,
                Some(&m.c_admin),
            )
            .await;
        assert_eq!(l.json["total"], n, "{t}");
    }
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, Some(&m.c_admin))
        .await;
    assert_eq!(c.json, r.json["compteurs"]);

    // Société choisie : documents de cette société (ou de ses boîtes, pas encore rangés).
    let r = app
        .requete(
            "GET",
            &format!("/api/a-traiter?societe={}", m.sb),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(ids(&r.json["a_valider"]), vec![f.fb, f.fother]);
    assert_eq!(ids(&r.json["a_repondre"]), vec![f.mail_b]);
    assert_eq!(r.json["bloque"].as_array().unwrap().len(), 2);
    let r = app
        .requete(
            "GET",
            &format!("/api/a-traiter?societe={}", m.sa),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(ids(&r.json["a_valider"]), vec![f.f1, f.bl1, f.fnull, f.f2]);

    // Responsable : sa société seulement, ni boîtes ni Claude.
    let r = app
        .requete("GET", "/api/a-traiter", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(ids(&r.json["a_valider"]), vec![f.f1, f.bl1, f.fnull, f.f2]);
    let genres: Vec<&str> = r.json["bloque"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["genre"].as_str().unwrap())
        .collect();
    assert_eq!(genres, vec!["envoi_erreur", "doublon", "illisible"]);
    assert_eq!(ids(&r.json["a_repondre"]), vec![f.mail_a]);
    assert_eq!(
        r.json["compteurs"],
        json!({ "bloque": 3, "a_valider": 4, "a_valider_types": { "facture": 3, "avoir": 0, "bl": 1 }, "a_repondre": 1, "total": 8 })
    );
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(c.json, r.json["compteurs"]);
    // Une autre société demandée par un responsable : rien.
    let c = app
        .requete(
            "GET",
            &format!("/api/a-traiter/compteurs?societe={}", m.sb),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(c.json["a_valider"], 0);
    assert_eq!(c.json["bloque"], 0);
    let r = app.requete("GET", "/api/a-traiter", None, None).await;
    assert_eq!(r.code, StatusCode::UNAUTHORIZED);

    // Claude connecté, boîte rétablie : plus de blocage côté administrateur.
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::parametre::CLAUDE_ETAT,
        &json!({ "etat": "valide", "message": "" }),
    )
    .await
    .unwrap();
    sqlx::query("update boite_mail set pause_auto = false")
        .execute(app.pool())
        .await
        .unwrap();
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, Some(&m.c_admin))
        .await;
    assert_eq!(c.json["bloque"], 3);

    // Envoi en échec relancé (validé après l'échec) : il quitte Bloqué en attendant le worker,
    // et la fiche le dit ; au bout de 30 min sans reprise, il y revient.
    sqlx::query("update document set valide_le = now() where id = $1")
        .bind(f.envoi_err)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete("GET", "/api/a-traiter", None, Some(&m.c_admin))
        .await;
    assert!(
        !r.json["bloque"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["document_id"] == f.envoi_err)
    );
    assert_eq!(r.json["compteurs"]["bloque"], 2);
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, Some(&m.c_admin))
        .await;
    assert_eq!(c.json, r.json["compteurs"]);
    let fiche = app
        .requete(
            "GET",
            &format!("/api/documents/{}", f.envoi_err),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(fiche.json["envoi_relance"], true);
    sqlx::query("update document set valide_le = now() - interval '31 minutes' where id = $1")
        .bind(f.envoi_err)
        .execute(app.pool())
        .await
        .unwrap();
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, Some(&m.c_admin))
        .await;
    assert_eq!(c.json["bloque"], 3);
    m.app.fermer().await;
}

async fn valider(m: &Monde, id: i64, corps: Value, cookie: &str) -> commun::Reponse {
    m.app
        .requete(
            "POST",
            &format!("/api/documents/{id}/valider"),
            Some(corps),
            Some(cookie),
        )
        .await
}

async fn envois(app: &Appli) -> i64 {
    app.compter(
        "select count(*) from apalis.jobs where job_type = 'envoi' and convert_from(job, 'UTF8') like '%Deposer%'",
    )
    .await
}

#[tokio::test]
async fn valider_en_un_appel() {
    let m = monde().await;
    let app = &m.app;
    let f = remplir(&m).await;
    let resp = m.c_resp_a.as_str();

    // Sans adresse d'envoi : refusé, et rien n'est gardé (pas même la vérification).
    let r = valider(&m, f.f1, json!({}), resp).await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert_eq!(r.json["code"], "expedition");
    assert_eq!(
        texte(
            app,
            &format!("select verifie_le::text from document where id = {}", f.f1)
        )
        .await,
        None
    );
    configurer_expedition(app).await;

    // Facture juste : vérifiée, validée, envoi demandé ; suivant = celui qui la suivait.
    let r = valider(&m, f.f1, json!({}), resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["envoi"], true);
    assert_eq!(r.json["statut_depot"], "pret");
    assert_eq!(r.json["suivant"], f.bl1);
    assert_eq!(envois(app).await, 1);
    assert!(
        texte(
            app,
            &format!("select valide_le::text from document where id = {}", f.f1)
        )
        .await
        .is_some()
    );

    // BL : vérification seule ; plus rien après lui, on revient en tête de file.
    let r = valider(&m, f.bl1, json!({}), resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["envoi"], false);
    assert_eq!(r.json["statut_depot"], Value::Null);
    assert_eq!(r.json["suivant"], f.fnull);
    assert_eq!(envois(app).await, 1);
    assert!(
        texte(
            app,
            &format!("select verifie_le::text from document where id = {}", f.bl1)
        )
        .await
        .is_some()
    );
    assert_eq!(
        texte(
            app,
            &format!("select valide_le::text from document where id = {}", f.bl1)
        )
        .await,
        None
    );

    // Correction en même temps que la validation : le numéro manquant est saisi.
    let correction = |numero: &str, ht: f64| {
        json!({
            "type": "facture", "societe_id": m.sa, "fournisseur_id": m.durand,
            "numero": numero, "date_document": null, "numero_commande": null,
            "montant_ht": ht, "montant_tva": 20.0, "montant_ttc": 120.0
        })
    };
    let mut c = correction("F-2", 100.0);
    c["date_document"] = json!(
        texte(
            app,
            &format!(
                "select date_document::text from document where id = {}",
                f.f2
            )
        )
        .await
        .unwrap()
    );
    let r = valider(&m, f.f2, json!({ "corrections": c }), resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["statut_depot"], "pret");
    assert_eq!(
        texte(
            app,
            &format!(
                "select corrections->'numero'->>'apres' from document where id = {}",
                f.f2
            )
        )
        .await,
        Some("F-2".into())
    );
    assert_eq!(envois(app).await, 2);

    // Doublon : refusé avec son code, la correction n'est pas gardée.
    let mut c = correction("F-1", 50.0);
    c["montant_tva"] = json!(10.0);
    c["montant_ttc"] = json!(60.0);
    c["numero_commande"] = json!("CDE-9");
    let r = valider(&m, f.dbl, json!({ "corrections": c }), resp).await;
    assert_eq!(r.code, StatusCode::CONFLICT, "{}", r.json);
    assert_eq!(r.json["code"], "doublon");
    assert_eq!(
        r.json["erreur"],
        format!("Même facture que le document {} (reçue avant)", f.f1)
    );
    assert_eq!(
        texte(
            app,
            &format!("select numero_commande from document where id = {}", f.dbl)
        )
        .await,
        None
    );
    // Forcer : administrateur seulement.
    let r = valider(&m, f.dbl, json!({ "forcer": true }), resp).await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = valider(&m, f.dbl, json!({ "forcer": true }), &m.c_admin).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'envoi' and convert_from(job, 'UTF8') like '%\"forcer\":true%'").await,
        1
    );

    // Périmètre : autre société invisible ou interdite.
    assert_eq!(
        valider(&m, f.fb, json!({}), resp).await.code,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        valider(&m, f.fother, json!({}), resp).await.code,
        StatusCode::FORBIDDEN
    );
    // Document illisible ou déjà en envoi : pas de validation.
    let r = valider(&m, f.err, json!({}), resp).await;
    assert_eq!(r.json["code"], "etat");
    sqlx::query("update document set statut_depot = 'en_envoi' where id = $1")
        .bind(f.f1)
        .execute(app.pool())
        .await
        .unwrap();
    let r = valider(&m, f.f1, json!({}), resp).await;
    assert_eq!(r.json["code"], "deja_envoye");
    // Envoi en échec : seul un administrateur renvoie.
    let r = valider(&m, f.envoi_err, json!({}), resp).await;
    assert_eq!(r.json["code"], "erreur_envoi");
    let r = valider(&m, f.envoi_err, json!({}), &m.c_admin).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    // Relancé : il quitte Bloqué (pas de second renvoi en attendant le worker).
    let file = app
        .requete("GET", "/api/a-traiter", None, Some(&m.c_admin))
        .await;
    assert!(
        !file.json["bloque"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["document_id"] == f.envoi_err)
    );
    // Entrée invalide.
    let r = valider(
        &m,
        f.fnull,
        json!({ "corrections": { "type": "inconnu" } }),
        resp,
    )
    .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    // Sans l'en-tête X-GMFlow : refusé.
    let r = app
        .requete_brute(
            "POST",
            &format!("/api/documents/{}/valider", f.fnull),
            Some(json!({})),
            Some(resp),
            false,
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    m.app.fermer().await;
}

#[tokio::test]
async fn ecarter_doublon_et_illisible() {
    let m = monde().await;
    let app = &m.app;
    let f = remplir(&m).await;
    let resp = Some(m.c_resp_a.as_str());
    let ecarter = |id: i64| format!("/api/documents/{id}/ecarter");
    let r = app.requete("POST", &ecarter(f.err), None, resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(
        texte(
            app,
            &format!("select etat from document where id = {}", f.err)
        )
        .await,
        Some("ignore".into())
    );
    let r = app.requete("POST", &ecarter(f.dbl), None, resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["statut_depot"], "doublon");
    assert_eq!(r.json["suivant"], f.fnull);
    // Tout document lu s'écarte (pas une facture) : il passe en « Écarté » et quitte la file.
    let r = app.requete("POST", &ecarter(f.f1), None, resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["statut_depot"], Value::Null);
    assert_eq!(r.json["envoi"], false);
    assert_eq!(
        texte(
            app,
            &format!("select etat from document where id = {}", f.f1)
        )
        .await,
        Some("ignore".into())
    );
    let r = app.requete("POST", &ecarter(f.bl1), None, resp).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    // Déjà écarté (plus lu) : refusé ; autre société : invisible ou interdit.
    let r = app.requete("POST", &ecarter(f.f1), None, resp).await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert_eq!(r.json["code"], "etat");
    let r = app.requete("POST", &ecarter(f.fb), None, resp).await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app.requete("POST", &ecarter(f.fother), None, resp).await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    // Envoi en échec : un administrateur seulement ; déjà envoyé : jamais.
    let r = app.requete("POST", &ecarter(f.envoi_err), None, resp).await;
    assert_eq!(r.json["code"], "erreur_envoi");
    sqlx::query("update document set statut_depot = 'depose' where id = $1")
        .bind(f.f2)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app.requete("POST", &ecarter(f.f2), None, resp).await;
    assert_eq!(r.json["code"], "deja_envoye");
    let c = app
        .requete("GET", "/api/a-traiter/compteurs", None, resp)
        .await;
    assert_eq!(c.json["bloque"], 1, "reste l'envoi en échec");
    assert_eq!(
        app.compter("select count(*) from journal where action = 'document_ecarte'")
            .await,
        2
    );
    assert_eq!(
        app.compter(
            "select count(*) from journal where action in ('doublon_ecarte', 'illisible_ecarte')"
        )
        .await,
        2
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn fournisseurs_par_societe_et_fil() {
    let m = monde().await;
    let app = &m.app;
    let f = remplir(&m).await;
    // Martin : un BL dans l'autre société seulement.
    let bl_martin = doc(
        &m,
        m.boite_b,
        Some(m.sb),
        "bl",
        Some("BL-M"),
        10.0,
        2,
        Some(m.martin),
    )
    .await;
    let admin = Some(m.c_admin.as_str());

    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs?societe={}", m.sa),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(ids(&r.json["elements"]), vec![m.durand]);
    let d = &r.json["elements"][0];
    // Frouard (et non rangés de ses boîtes) : f1, f2, fnull, dbl, envoi_err (+ déposée), BL-1 et BL-V.
    assert_eq!(d["a_valider"], 4, "{d}");
    assert_eq!(d["bl_sans_facture"], 2);
    let attendu = app
        .compter(&format!(
            "select count(*) from document where fournisseur_id = {} and type = 'facture' and etat = 'lu'
               and statut_depot is distinct from 'doublon'
               and (societe_id = {} or societe_id is null)
               and date_document >= date_trunc('month', now() at time zone 'Europe/Paris')::date",
            m.durand, m.sa
        ))
        .await;
    assert_eq!(d["factures_mois"], attendu, "doublon non compté");
    let r = app.requete("GET", "/api/fournisseurs", None, admin).await;
    assert_eq!(r.json["total"], 2);
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs?societe={}", m.sb),
            None,
            admin,
        )
        .await;
    let martin = r.json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == m.martin)
        .unwrap()
        .clone();
    assert_eq!(martin["bl_sans_facture"], 1);

    // Responsable : seulement les fournisseurs de sa société (un document dans son périmètre,
    // ou sa société par défaut), même sans `?societe=` ; compteurs de son périmètre.
    let resp = Some(m.c_resp_a.as_str());
    let r = app.requete("GET", "/api/fournisseurs", None, resp).await;
    assert_eq!(r.json["total"], 1, "{}", r.json);
    assert_eq!(ids(&r.json["elements"]), vec![m.durand]);
    let durand = &r.json["elements"][0];
    let dans_perimetre = app
        .compter(&format!(
            "select count(*) from document d where d.fournisseur_id = {}
               and (d.societe_id is null or d.societe_id = {})
               and exists (select 1 from mail_boite mb where mb.mail_id = d.mail_id and mb.boite_id = {})",
            m.durand, m.sa, m.boite_a
        ))
        .await;
    assert_eq!(durand["nb_documents"], dans_perimetre);
    let r = app
        .requete("GET", "/api/fournisseurs?q=martin", None, resp)
        .await;
    assert_eq!(r.json["total"], 0, "la recherche ne sort pas du périmètre");
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}", m.martin),
            None,
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil", m.martin),
            None,
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);

    // Modification partielle (admin) : « Toujours ranger dans Frouard », puis « Confirmer ».
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "societe_defaut_id": m.sa })),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    sqlx::query("update fournisseur set statut = 'a_confirmer', notes = 'à garder' where id = $1")
        .bind(m.martin)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "societe_defaut_id": m.sa })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["societe_defaut"], "Frouard");
    assert_eq!(r.json["statut"], "a_confirmer");
    assert_eq!(r.json["notes"], "à garder");
    assert_eq!(r.json["siret"], "55210055400013");
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "statut": "valide", "notes": null })),
            admin,
        )
        .await;
    assert_eq!(r.json["statut"], "valide");
    assert_eq!(r.json["notes"], Value::Null);
    assert_eq!(r.json["societe_defaut_id"], m.sa);
    let r = app
        .requete(
            "PATCH",
            "/api/admin/fournisseurs/999999",
            Some(json!({ "statut": "valide" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "statut": "autre" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    // Code du logiciel de gestion : gardé, cherchable, jamais sur deux fiches.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "code_gestion": " MAR01 " })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["code_gestion"], "MAR01");
    let r = app
        .requete("GET", "/api/fournisseurs?q=mar01", None, admin)
        .await;
    assert_eq!(r.json["total"], 1);
    assert_eq!(r.json["elements"][0]["code_gestion"], "MAR01");
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.durand),
            Some(json!({ "code_gestion": "mar01" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert!(r.json.to_string().contains("Martin Frais"), "{}", r.json);
    // Coordonnées de la fiche, e-mail vérifié.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(
                json!({ "telephone": " 03 83 00 00 00 ", "interlocuteur": "Mme Roy",
                         "email": "commandes@martin.fr", "adresse": "1 rue X\n54390 Frouard" }),
            ),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["telephone"], "03 83 00 00 00");
    assert_eq!(r.json["code_gestion"], "MAR01");
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "email": "pas une adresse" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    // Doublon probable : même nom collé. « Pas un doublon » le retire de la liste.
    let r = app
        .requete(
            "POST",
            "/api/admin/fournisseurs",
            Some(json!({ "nom": "martinfrais.fr" })),
            admin,
        )
        .await;
    let copie = r.json["id"].as_i64().unwrap();
    let r = app
        .requete("GET", "/api/admin/fournisseurs/doublons", None, admin)
        .await;
    assert!(
        r.json
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["a"]["id"] == m.martin && p["b"]["id"] == copie),
        "{}",
        r.json
    );
    let r = app
        .requete("GET", "/api/admin/fournisseurs/doublons", None, resp)
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete(
            "POST",
            "/api/admin/fournisseurs/doublons/distinct",
            Some(json!({ "a": copie, "b": m.martin })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let r = app
        .requete("GET", "/api/admin/fournisseurs/doublons", None, admin)
        .await;
    assert!(
        !r.json
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["b"]["id"] == copie)
    );
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/fournisseurs/{copie}/fusionner"),
            Some(json!({ "cible_id": m.martin })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    // Société par défaut = la sienne : le responsable voit la fiche (sans les documents de B).
    let r = app.requete("GET", "/api/fournisseurs", None, resp).await;
    assert_eq!(r.json["total"], 2);
    let martin = r.json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == m.martin)
        .unwrap()
        .clone();
    assert_eq!(martin["bl_sans_facture"], 0);
    assert_eq!(martin["nb_documents"], 0);
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs?societe={}", m.sb),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.json["total"], 0);

    // Fil : documents et mails par mois, encadré « À rapprocher ».
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil", m.durand),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["fournisseur"]["nom"], "Grossiste Durand");
    assert_eq!(r.json["fournisseur"]["societe_defaut"], Value::Null);
    let elements: Vec<Value> = r.json["mois"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["elements"].as_array().unwrap().clone())
        .collect();
    let docs: Vec<i64> = elements
        .iter()
        .filter(|e| e["genre"] == "document")
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    for id in [f.f1, f.bl1, f.fb, f.fother] {
        assert!(docs.contains(&id), "{id} dans le fil");
    }
    assert!(
        !docs.contains(&f.err),
        "illisible : pas un document du fournisseur"
    );
    // La question vient du domaine du fournisseur, sans document.
    let question = elements
        .iter()
        .find(|e| e["genre"] == "mail" && e["id"] == f.mail_a)
        .expect("mail du domaine du fournisseur");
    assert_eq!(question["document_ids"], json!([]));
    let dates: Vec<&str> = elements
        .iter()
        .map(|e| e["date"].as_str().unwrap())
        .collect();
    let mut triees = dates.clone();
    triees.sort_by(|a, b| b.cmp(a));
    assert_eq!(dates, triees, "du plus récent au plus ancien");
    let bl_attente = ids(&r.json["a_rapprocher"]["bl_sans_facture"]);
    assert!(bl_attente.contains(&f.bl1));
    assert!(r.json["etat"]["a_valider"].as_i64().unwrap() >= 5);

    // Responsable : rien de l'autre société.
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil", m.durand),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    let tout = r.json["mois"].to_string();
    assert!(!tout.contains(&format!("\"id\":{},", f.fb)));
    assert!(!tout.contains(&format!("\"id\":{},", f.mail_b)));
    let elements: Vec<Value> = r.json["mois"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["elements"].as_array().unwrap().clone())
        .collect();
    assert!(
        !elements
            .iter()
            .any(|e| (e["genre"] == "document" && e["id"] == f.fother)
                || (e["genre"] == "mail"
                    && e["document_ids"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(f.fother)))),
        "rangé dans l'autre société"
    );
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil", m.martin),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.json["mois"], json!([]));
    assert_eq!(r.json["a_rapprocher"]["bl_sans_facture"], json!([]));
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil", m.martin),
            None,
            admin,
        )
        .await;
    assert_eq!(
        ids(&r.json["a_rapprocher"]["bl_sans_facture"]),
        vec![bl_martin]
    );
    let r = app
        .requete("GET", "/api/fournisseurs/999999/fil", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/fil?mois=40", m.durand),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    // Groupe : Martin Frais facture pour la marque « Frais Express » ; un seul niveau.
    let r = app
        .requete(
            "POST",
            "/api/admin/fournisseurs",
            Some(
                json!({ "nom": "Frais Express", "code_gestion": "FEX01", "statut": "valide",
                         "groupe_id": m.martin, "noms_documents": ["F. EXPRESS", " "] }),
            ),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["groupe"], "Martin Frais");
    assert_eq!(r.json["noms_documents"], json!(["F. EXPRESS"]));
    let marque = r.json["id"].as_i64().unwrap();
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "groupe_id": marque })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST, "{}", r.json);
    let r = app
        .requete(
            "GET",
            &format!("/api/fournisseurs/{}/marques", m.martin),
            None,
            admin,
        )
        .await;
    assert_eq!(r.json[0]["nom"], "Frais Express");
    // Répartition : un document de Martin Frais qui nomme la marque passe chez elle.
    let doc: i64 = sqlx::query_scalar(
        "update document set fournisseur_id = $1, texte = 'Bon de livraison F. Express n° 12'
         where id = (select id from document where fournisseur_id = $1 order by id limit 1)
         returning id",
    )
    .bind(m.martin)
    .fetch_one(app.pool())
    .await
    .unwrap();
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/fournisseurs/{}/repartir", m.martin),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["deplaces"], 1, "{}", r.json);
    let rangee: i64 = sqlx::query_scalar("select fournisseur_id from document where id = $1")
        .bind(doc)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(rangee, marque);
    m.app.fermer().await;
}

#[tokio::test]
async fn recherche_unique() {
    let m = monde().await;
    let app = &m.app;
    let d = Some(m.durand);
    let cible = doc(
        &m,
        m.boite_a,
        Some(m.sa),
        "facture",
        Some("FA-778"),
        1250.40,
        1,
        d,
    )
    .await;
    sqlx::query("update document set date_document = '2025-02-03' where id = $1")
        .bind(cible)
        .execute(app.pool())
        .await
        .unwrap();
    let mail_cible: i64 = sqlx::query_scalar("select mail_id from document where id = $1")
        .bind(cible)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let autre = doc(&m, m.boite_a, Some(m.sa), "bl", Some("BL-12"), 99.0, 2, d).await;
    let chez_b = doc(
        &m,
        m.boite_b,
        Some(m.sb),
        "facture",
        Some("FB-1"),
        1250.40,
        1,
        d,
    )
    .await;
    let chercher = |q: &str, c: &str| {
        let q = q
            .replace(' ', "%20")
            .replace('/', "%2F")
            .replace(',', "%2C");
        let c = c.to_string();
        async move {
            app.requete("GET", &format!("/api/recherche?q={q}"), None, Some(&c))
                .await
        }
    };
    let admin = m.c_admin.as_str();

    let r = chercher("1 250,40", admin).await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["interpretation"]["montant"], 1250.4);
    let docs = ids(&r.json["documents"]["elements"]);
    assert!(docs.contains(&cible) && docs.contains(&chez_b) && !docs.contains(&autre));
    assert!(ids(&r.json["mails"]["elements"]).contains(&mail_cible));
    assert_eq!(r.json["documents"]["total"], 2);

    assert!(ids(&chercher("1250.4", admin).await.json["documents"]["elements"]).contains(&cible));
    for q in ["03/02/2025", "2025-02-03", "FA-778", "durand fa-778"] {
        let r = chercher(q, admin).await;
        assert_eq!(
            ids(&r.json["documents"]["elements"]),
            vec![cible],
            "{q} : {}",
            r.json
        );
    }
    for q in ["03/02", "Durand 1250,4"] {
        let r = chercher(q, admin).await;
        let docs = ids(&r.json["documents"]["elements"]);
        assert!(
            docs.contains(&cible) && !docs.contains(&autre),
            "{q} : {}",
            r.json
        );
    }
    assert_eq!(
        chercher("03/02", admin).await.json["interpretation"]["date"]["annee"],
        Value::Null
    );
    // Un nombre seul vaut un numéro ou un montant.
    let r = chercher("778", admin).await;
    assert_eq!(ids(&r.json["documents"]["elements"]), vec![cible]);
    let r = chercher("99", admin).await;
    assert_eq!(ids(&r.json["documents"]["elements"]), vec![autre]);
    let r = chercher("introuvable", admin).await;
    assert_eq!(r.json["documents"]["total"], 0);
    assert_eq!(r.json["mails"]["total"], 0);
    // Filtres de type.
    let r = app
        .requete("GET", "/api/recherche?q=durand&type=bl", None, Some(admin))
        .await;
    assert_eq!(ids(&r.json["documents"]["elements"]), vec![autre]);
    assert_eq!(r.json["mails"]["total"], 0);
    let r = app
        .requete("GET", "/api/recherche?type=mail", None, Some(admin))
        .await;
    assert_eq!(r.json["documents"]["total"], 0);
    assert_eq!(r.json["mails"]["total"], 3);
    let r = app
        .requete("GET", "/api/recherche?type=devis", None, Some(admin))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    // `document` : tous les documents, pas de mails ; l'outil de la société est donné.
    let r = app
        .requete("GET", "/api/recherche?type=document", None, Some(admin))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["documents"]["total"], 3);
    assert_eq!(r.json["mails"]["total"], 0);
    assert_eq!(r.json["documents"]["elements"][0]["outil"], "dext");
    // Requête vide : les derniers reçus.
    let r = app
        .requete("GET", "/api/recherche", None, Some(admin))
        .await;
    assert_eq!(r.json["mails"]["total"], 3);
    assert_eq!(r.json["documents"]["total"], 3);
    // Responsable : rien de l'autre société, même reçu par sa boîte.
    let r = chercher("1 250,40", &m.c_resp_a).await;
    assert_eq!(ids(&r.json["documents"]["elements"]), vec![cible]);
    let range_ailleurs = doc(
        &m,
        m.boite_a,
        Some(m.sb),
        "facture",
        Some("FO-9"),
        1250.40,
        1,
        d,
    )
    .await;
    let r = chercher("1 250,40", &m.c_resp_a).await;
    assert_eq!(ids(&r.json["documents"]["elements"]), vec![cible]);
    let r = chercher("1 250,40", admin).await;
    assert!(ids(&r.json["documents"]["elements"]).contains(&range_ailleurs));
    let r = app
        .requete(
            "GET",
            &format!("/api/recherche?societe={}", m.sb),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.json["documents"]["total"], 0);
    assert_eq!(r.json["mails"]["total"], 0);
    m.app.fermer().await;
}

#[tokio::test]
async fn mise_en_route_et_essais() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    let r = app
        .requete("GET", "/api/admin/mise-en-route", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete("GET", "/api/admin/mise-en-route", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let fait = |r: &commun::Reponse| -> Vec<(String, bool)> {
        r.json["etapes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["cle"].as_str().unwrap().to_string(),
                    e["fait"].as_bool().unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(
        fait(&r),
        vec![
            ("societes".into(), true),
            ("boites".into(), false),
            ("claude".into(), false),
            ("envoi".into(), false),
            ("utilisateurs".into(), false),
        ]
    );
    assert_eq!(r.json["terminee"], false);
    let manques = r.json["etapes"][4]["manques"].to_string();
    assert!(manques.contains("Houdemont") && !manques.contains("Frouard"));
    assert_eq!(
        r.json["etapes"][1]["resume"],
        "2 boîtes mail, aucune surveillée"
    );
    assert_eq!(
        r.json["etapes"][4]["resume"],
        "1 administrateur, 1 responsable"
    );

    sqlx::query("update boite_mail set active = true")
        .execute(app.pool())
        .await
        .unwrap();
    gmflow_core::parametre::ecrire(
        app.pool(),
        gmflow_core::parametre::CLAUDE_ETAT,
        &json!({ "etat": "valide", "message": "" }),
    )
    .await
    .unwrap();
    configurer_expedition(app).await;
    app.utilisateur("resp-b", MDP, "responsable", Some(m.sb))
        .await;
    let r = app
        .requete("GET", "/api/admin/mise-en-route", None, admin)
        .await;
    assert_eq!(r.json["terminee"], true, "{}", r.json);
    assert_eq!(r.json["etapes"][1]["resume"], "2 boîtes mail surveillées");

    // Essai de paramètres pas encore enregistrés : demande gardée pour le worker.
    let r = app
        .requete(
            "POST",
            "/api/admin/boites/tester",
            Some(json!({ "serveur": "imap.test", "identifiant": "compta@frouard.test" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST, "mot de passe obligatoire");
    let r = app
        .requete(
            "POST",
            "/api/admin/boites/tester",
            Some(json!({
                "serveur": "imap.test", "identifiant": "compta@frouard.test",
                "mot_de_passe": "secret-de-test", "smtp_serveur": "smtp.test", "smtp_securite": "starttls"
            })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{}", r.json);
    let id = r.json["id"].as_str().unwrap().to_string();
    assert_eq!(
        app.compter("select count(*) from apalis.jobs where job_type = 'boites' and convert_from(job, 'UTF8') like '%Essai%'").await,
        1
    );
    assert_eq!(
        app.compter(&format!(
            "select count(*) from parametre where cle = 'essai_boite:{id}' and valeur_chiffree is not null"
        ))
        .await,
        1
    );
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/essais/{id}"),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    assert_eq!(r.json["etat"], "en_cours");
    assert_eq!(r.json["imap"]["port"], 993);
    assert_eq!(r.json["smtp"]["port"], 587);
    assert!(!r.json.to_string().contains("secret-de-test"));
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/essais/{id}"),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete(
            "POST",
            "/api/admin/boites/tester",
            Some(json!({ "serveur": "imap.test", "identifiant": "x", "mot_de_passe": "y" })),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);

    // Boîte existante : son mot de passe est repris ; SMTP seul.
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/boites/{}/tester-smtp", m.boite_a),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT);
    assert_eq!(r.json["code"], "smtp_absent");
    sqlx::query("update boite_mail set smtp_serveur = 'smtp.test', smtp_port = 465, smtp_securite = 'tls' where id = $1")
        .bind(m.boite_a)
        .execute(app.pool())
        .await
        .unwrap();
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/boites/{}/tester-smtp", m.boite_a),
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{}", r.json);
    let id = r.json["id"].as_str().unwrap();
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/essais/{id}"),
            None,
            admin,
        )
        .await;
    assert_eq!(r.json["imap"], Value::Null);
    assert_eq!(r.json["boite_id"], m.boite_a);
    let r = app
        .requete(
            "POST",
            "/api/admin/boites/tester",
            Some(json!({ "boite_id": m.boite_a })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{}", r.json);
    let r = app
        .requete(
            "GET",
            "/api/admin/boites/essais/00000000-0000-0000-0000-000000000000",
            None,
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    m.app.fermer().await;
}

/// Création d'une boîte qui reprend un essai réussi (assistant : pas de second essai), et
/// fiche d'une boîte.
#[tokio::test]
async fn boite_creee_depuis_un_essai() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    let parametres = json!({
        "serveur": "imap.test", "identifiant": "compta@frouard.test", "mot_de_passe": "secret-de-test"
    });
    let r = app
        .requete("POST", "/api/admin/boites/tester", Some(parametres), admin)
        .await;
    assert_eq!(r.code, StatusCode::ACCEPTED, "{}", r.json);
    let essai = r.json["id"].as_str().unwrap().to_string();
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/essais/{essai}"),
            None,
            admin,
        )
        .await;
    assert!(r.json.get("mdp_empreinte").is_none(), "{}", r.json);
    let creer = |essai: Option<&str>, mdp: &str, activer: bool| {
        json!({
            "societe_id": m.sa, "libelle": "Compta 2", "serveur": "imap.test",
            "identifiant": "compta@frouard.test", "mot_de_passe": mdp,
            "dossiers": ["INBOX"], "essai_id": essai, "activer": activer
        })
    };
    // Essai pas terminé : refusé.
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(Some(&essai), "secret-de-test", true)),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT, "{}", r.json);
    assert_eq!(r.json["code"], "essai");
    // Le worker a terminé l'essai (simulé ici) : connexion réussie, dossiers trouvés.
    sqlx::query(
        "update parametre set valeur = valeur || jsonb_build_object(
             'etat', 'termine', 'termine_le', '2026-11-22T10:00:00Z',
             'resultat_imap', jsonb_build_object(
                 'ok', true, 'message', 'Connexion réussie : 2 dossiers trouvés.', 'detail', null,
                 'dossiers', jsonb_build_array(
                     jsonb_build_object('nom', 'INBOX', 'affiche', 'Boîte de réception', 'envoyes', false, 'non_selectionnable', false),
                     jsonb_build_object('nom', 'Envoy&AOk-s', 'affiche', 'Envoyés', 'envoyes', true, 'non_selectionnable', false)),
                 'reception', 'INBOX', 'envoyes', 'Envoy&AOk-s'))
         where cle = $1",
    )
    .bind(format!("essai_boite:{essai}"))
    .execute(app.pool())
    .await
    .unwrap();
    // Autre mot de passe, ou activer sans essai : refusé.
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(Some(&essai), "autre-secret", true)),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::CONFLICT, "{}", r.json);
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(None, "secret-de-test", true)),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST, "{}", r.json);
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(Some(&essai), "secret-de-test", true)),
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    // Même paramètres : la boîte est créée, testée et surveillée en un appel.
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(Some(&essai), "secret-de-test", true)),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let id = r.json["id"].as_i64().unwrap();
    assert_eq!(r.json["active"], true);
    assert_eq!(r.json["dernier_test"]["ok"], true);
    assert_eq!(
        r.json["dernier_test"]["dossiers"],
        json!(["INBOX", "Envoy&AOk-s"])
    );
    assert_eq!(r.json["dernier_test"]["envoyes"], "Envoy&AOk-s");
    assert_eq!(
        r.json["dernier_test"]["dossiers_detail"][1]["affiche"],
        "Envoyés"
    );
    assert_eq!(
        app.compter(&format!(
            "select count(*) from journal where action = 'activation' and objet_id = '{id}'"
        ))
        .await,
        1
    );
    // Surveillée tout de suite : la première relève est déjà dans la file.
    assert_eq!(
        app.compter(&format!(
            "select count(*) from apalis.jobs where job_type = 'boites' and convert_from(job, 'UTF8') like '%Releve%' and convert_from(job, 'UTF8') like '%\"boite_id\":{id}%'"
        ))
        .await,
        1
    );
    // Sans essai : créée en pause, comme avant.
    let r = app
        .requete(
            "POST",
            "/api/admin/boites",
            Some(creer(None, "secret-de-test", false)),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["active"], false);
    assert_eq!(r.json["dernier_test"], Value::Null);

    // Fiche d'une boîte.
    let r = app
        .requete("GET", &format!("/api/admin/boites/{id}"), None, admin)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["identifiant"], "compta@frouard.test");
    assert!(r.json.get("mot_de_passe_chiffre").is_none());
    let r = app
        .requete(
            "GET",
            &format!("/api/admin/boites/{id}"),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    let r = app
        .requete("GET", "/api/admin/boites/999999", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    m.app.fermer().await;
}

#[tokio::test]
async fn bilan_du_mois() {
    let m = monde().await;
    let app = &m.app;
    let d = Some(m.durand);
    doc(
        &m,
        m.boite_a,
        Some(m.sa),
        "facture",
        Some("F-1"),
        60.0,
        0,
        d,
    )
    .await;
    doc(&m, m.boite_a, Some(m.sa), "bl", Some("BL-1"), 60.0, 0, d).await;
    doc(
        &m,
        m.boite_b,
        Some(m.sb),
        "facture",
        Some("F-B"),
        60.0,
        0,
        d,
    )
    .await;
    doc(
        &m,
        m.boite_b,
        Some(m.sb),
        "bl",
        Some("BL-B"),
        60.0,
        0,
        Some(m.martin),
    )
    .await;
    let r = app
        .requete("GET", "/api/bilan", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["societes"].as_array().unwrap().len(), 2);
    assert_eq!(r.json["bl_en_attente"].as_array().unwrap().len(), 2);
    assert!(r.json["qualite"]["lecture"]["par_etat"].is_array());
    assert!(r.json["qualite"]["tri"].is_array());
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan?societe={}", m.sb),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.json["societes"].as_array().unwrap().len(), 1);
    assert_eq!(r.json["bl_en_attente"][0]["fournisseur"], "Martin Frais");
    assert_eq!(r.json["bl_sans_lignes"], 1);
    let r = app
        .requete("GET", "/api/bilan", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(r.json["societes"].as_array().unwrap().len(), 1);
    assert_eq!(r.json["societes"][0]["societe_id"], m.sa);
    assert_eq!(r.json["bl_en_attente"].as_array().unwrap().len(), 1);
    assert_eq!(r.json["bl_sans_lignes"], 1);

    // Reçus par la boîte de Frouard : l'un rangé chez Houdemont (invisible pour le responsable
    // de Frouard), l'autre pas encore rangé (compté pour Frouard, même avec `?societe=`).
    doc(
        &m,
        m.boite_a,
        Some(m.sb),
        "facture",
        Some("F-O"),
        48.0,
        0,
        d,
    )
    .await;
    doc(&m, m.boite_a, None, "facture", Some("F-N"), 36.0, 0, d).await;
    doc(&m, m.boite_a, None, "bl", Some("BL-N"), 12.0, 0, d).await;
    let recues = |r: &commun::Reponse, societe: Value| -> i64 {
        r.json["societes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["societe_id"] == societe)
            .map(|l| l["recues"].as_i64().unwrap())
            .unwrap_or(-1)
    };
    let r = app
        .requete("GET", "/api/bilan", None, Some(&m.c_resp_a))
        .await;
    assert_eq!(
        r.json["societes"].as_array().unwrap().len(),
        1,
        "{}",
        r.json
    );
    assert_eq!(recues(&r, json!(m.sa)), 2, "F-1 et le non rangé");
    assert_eq!(r.json["bl_sans_lignes"], 2);
    let bl: i64 = r.json["bl_en_attente"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["bl"].as_i64().unwrap())
        .sum();
    assert_eq!(bl, 2, "BL-1 et le BL pas encore rangé");
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan?societe={}", m.sa),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(
        r.json["societes"].as_array().unwrap().len(),
        1,
        "{}",
        r.json
    );
    assert_eq!(recues(&r, json!(m.sa)), 2);
    assert_eq!(r.json["bl_sans_lignes"], 2);
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan?societe={}", m.sb),
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(recues(&r, json!(m.sb)), 2, "F-B et F-O");
    let r = app
        .requete("GET", "/api/bilan", None, Some(&m.c_admin))
        .await;
    assert_eq!(recues(&r, Value::Null), 1, "ligne des non rangés");
    // Un responsable qui demande une autre société : rien.
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan?societe={}", m.sb),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.json["societes"], json!([]));
    assert_eq!(r.json["bl_en_attente"], json!([]));
    assert_eq!(r.json["bl_sans_lignes"], 0);
    let r = app
        .requete("GET", "/api/bilan?mois=2026-13", None, Some(&m.c_admin))
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    m.app.fermer().await;
}

/// Fiche document (réception, boîte, alertes dans le vocabulaire de l'écran) et fiche mail
/// (boîtes avec adresse et société, résumé des documents des pièces).
#[tokio::test]
async fn fiches_document_et_mail() {
    let m = monde().await;
    let app = &m.app;
    sqlx::query("update boite_mail set identifiant = 'compta@frouard.test', smtp_serveur = 'smtp.test' where id = $1")
        .bind(m.boite_a)
        .execute(app.pool())
        .await
        .unwrap();
    let id = doc(
        &m,
        m.boite_a,
        Some(m.sa),
        "facture",
        Some("F-9"),
        1250.4,
        1,
        Some(m.durand),
    )
    .await;
    // HT + TVA ne font plus le TTC.
    sqlx::query("update document set montant_tva = 200 where id = $1")
        .bind(id)
        .execute(app.pool())
        .await
        .unwrap();
    gmflow_core::depot::evaluer(app.pool(), id).await.unwrap();
    let resp = Some(m.c_resp_a.as_str());
    let r = app
        .requete("GET", &format!("/api/documents/{id}"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert!(r.json["recu_le"].is_string());
    assert_eq!(r.json["envoi_relance"], false);
    assert_eq!(r.json["boites"][0]["adresse"], "compta@frouard.test");
    assert_eq!(r.json["boites"][0]["societe"], "Frouard");
    let c = &r.json["controles"][0];
    assert_eq!(c["code"], "montants", "{}", r.json["controles"]);
    assert_eq!(
        c["message"],
        "HT + TVA donnent 1\u{a0}242,00\u{a0}€, le TTC lu est 1\u{a0}250,40\u{a0}€ \
         (éco-participation ou port à ajouter au HT et à la TVA ?)"
    );
    assert_eq!(c["champs"], json!(["montant_ttc"]));
    assert!(c["proposition"].is_null(), "{c}");

    let mail_id = r.json["mail_id"].as_i64().unwrap();
    let r = app
        .requete("GET", &format!("/api/mails/{mail_id}"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let b = &r.json["boites"][0];
    assert_eq!(b["adresse"], "compta@frouard.test");
    assert_eq!(b["societe_id"], m.sa);
    assert_eq!(b["societe"], "Frouard");
    assert_eq!(b["smtp"], true);
    let p = &r.json["pieces"][0];
    assert_eq!(p["document_id"], id);
    assert_eq!(p["document_statut_depot"], "a_verifier");
    assert_eq!(p["document_numero"], "F-9");
    assert_eq!(p["document_montant_ttc"], 1250.4);
    assert_eq!(p["document_fournisseur"], "Grossiste Durand");
    assert_eq!(p["document_fournisseur_id"], m.durand);
    assert_eq!(p["document_outil"], "dext");
    assert_eq!(p["document_verifie"], false);
    assert_eq!(p["document_envoye_le"], Value::Null);

    // Les phrases d'avant (« structure… ») sont réécrites une fois au démarrage.
    sqlx::query(
        "update document set controles = '[{\"code\": \"societe\", \"message\": \"structure destinataire non reconnue\"}]'
         where id = $1",
    )
    .bind(id)
    .execute(app.pool())
    .await
    .unwrap();
    assert!(
        gmflow_core::depot::mettre_a_jour_controles(app.pool())
            .await
            .unwrap()
            >= 1
    );
    assert_eq!(
        texte(
            app,
            &format!("select controles->0->>'code' from document where id = {id}")
        )
        .await,
        Some("montants".into())
    );
    assert_eq!(
        gmflow_core::depot::mettre_a_jour_controles(app.pool())
            .await
            .unwrap(),
        0,
        "une seule fois"
    );
    m.app.fermer().await;
}

#[tokio::test]
async fn mail_hors_societe_et_apercu_tableur() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    let resp = Some(m.c_resp_a.as_str());
    let eml = gmflow_core::fixtures::eml(
        "hors@test",
        "Pas pour Frouard",
        &[
            ("bl.csv", "text/csv", "Réf;Qté\n12;3\n".as_bytes()),
            ("facture.pdf", "application/pdf", b"%PDF-1.4 test"),
        ],
        false,
    );
    let mail = app.mail_vu(&eml, &[m.boite_a]).await;
    let csv: i64 =
        sqlx::query_scalar("select id from piece_jointe where mail_id = $1 and nom = 'bl.csv'")
            .bind(mail)
            .fetch_one(app.pool())
            .await
            .unwrap();
    let pdf: i64 = sqlx::query_scalar(
        "select id from piece_jointe where mail_id = $1 and nom = 'facture.pdf'",
    )
    .bind(mail)
    .fetch_one(app.pool())
    .await
    .unwrap();
    // Aperçu tableur : une feuille, deux lignes ; un PDF n'est pas un tableur.
    let r = app
        .requete("GET", &format!("/api/pieces/{csv}/tableau"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json[0]["lignes"][1], json!(["12", "3"]));
    let r = app
        .requete("GET", &format!("/api/pieces/{pdf}/tableau"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);

    // Une facture lue de ce mail, pas encore partie.
    let doc: i64 = sqlx::query_scalar(
        "insert into document (mail_id, piece_jointe_id, sha256, etat, type, societe_id, statut_depot)
         select $1, $2, sha256, 'lu', 'facture', $3, 'pret' from piece_jointe where id = $2
         returning id",
    )
    .bind(mail)
    .bind(pdf)
    .bind(m.sa)
    .fetch_one(app.pool())
    .await
    .unwrap();

    // Le responsable de Frouard le masque : il disparaît, la facture est écartée.
    let r = app
        .requete(
            "POST",
            &format!("/api/mails/{mail}/hors-societe"),
            Some(json!({})),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["documents"], 1);
    assert_eq!(r.json["societes"], json!([m.sa]));
    for c in [resp, admin] {
        let r = app
            .requete("GET", &format!("/api/mails/{mail}"), None, c)
            .await;
        assert_eq!(r.code, StatusCode::NOT_FOUND);
        let r = app.requete("GET", "/api/mails?q=Pas%20pour", None, c).await;
        assert_eq!(r.json["total"], 0, "{}", r.json);
    }
    let (etat, statut): (String, Option<String>) =
        sqlx::query_as("select etat, statut_depot from document where id = $1")
            .bind(doc)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!((etat.as_str(), statut), ("ignore", None));
    // Société d'une autre boîte : refusé.
    let r = app
        .requete(
            "POST",
            &format!("/api/mails/{mail}/hors-societe"),
            Some(json!({ "societe_id": m.sb })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);

    // Annuler : le mail revient, la facture reprend son état.
    let r = app
        .requete(
            "DELETE",
            &format!("/api/mails/{mail}/hors-societe"),
            Some(json!({})),
            resp,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["documents"], 1);
    let r = app
        .requete("GET", &format!("/api/mails/{mail}"), None, resp)
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let etat: String = sqlx::query_scalar("select etat from document where id = $1")
        .bind(doc)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(etat, "lu");
    m.app.fermer().await;
}

/// Fiche avec un ancien SIRET invalide : un PATCH d'un autre champ passe sans le revalider ;
/// un champ refusé est nommé dans l'erreur ; un responsable ne modifie pas.
#[tokio::test]
async fn fiche_fournisseur_modification_partielle() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    sqlx::query(
        "update fournisseur set siret = '12345678900000', tva = 'FR00XX', email = 'pas une adresse'
         where id = $1",
    )
    .bind(m.martin)
    .execute(app.pool())
    .await
    .unwrap();
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "code_gestion": "PH-0042" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["code_gestion"], "PH-0042");
    assert_eq!(r.json["siret"], "12345678900000", "valeur stockée intacte");
    assert_eq!(r.json["email"], "pas une adresse");
    // Le champ envoyé reste vérifié, et l'erreur dit lequel.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "siret": "123", "notes": "x" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "siret", "{}", r.json);
    assert!(r.json["erreur"].as_str().unwrap().contains("SIRET"));
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "email": "pas une adresse non plus" })),
            admin,
        )
        .await;
    assert_eq!(r.json["champ"], "email", "{}", r.json);
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.durand),
            Some(json!({ "code_gestion": "ph-0042" })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["champ"], "code_gestion", "{}", r.json);
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "nom": "  " })),
            admin,
        )
        .await;
    assert_eq!(r.json["champ"], "nom", "{}", r.json);
    // Corriger le SIRET : accepté.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "siret": "552 100 554 00013", "email": null })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["siret"], "55210055400013");
    assert_eq!(r.json["email"], Value::Null);
    assert_eq!(r.json["tva"], "FR00XX");
    // Responsable : lecture seule.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "code_gestion": "AUTRE" })),
            Some(m.c_resp_a.as_str()),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    m.app.fermer().await;
}

/// Durand et Martin partagent un SIRET mais n'ont rien de commun dans le nom : « même groupe
/// probable », à regrouper (PATCH de `groupe_id`), puis répartition des documents.
#[tokio::test]
async fn doublons_meme_groupe_et_regroupement() {
    let m = monde().await;
    let app = &m.app;
    let admin = Some(m.c_admin.as_str());
    let r = app
        .requete("GET", "/api/admin/fournisseurs/doublons", None, admin)
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let paire = r
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["a"]["id"] == m.durand && p["b"]["id"] == m.martin)
        .unwrap_or_else(|| panic!("{}", r.json))
        .clone();
    assert_eq!(paire["meme_groupe"], true);
    assert_eq!(paire["raison"], "même groupe probable");
    assert_eq!(paire["a"]["nb_marques"], 0);
    // Regrouper : Martin dans le groupe Durand ; la paire n'est plus proposée.
    let r = app
        .requete(
            "PATCH",
            &format!("/api/admin/fournisseurs/{}", m.martin),
            Some(json!({ "groupe_id": m.durand })),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    let r = app
        .requete("GET", "/api/admin/fournisseurs/doublons", None, admin)
        .await;
    assert!(
        !r.json
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["b"]["id"] == m.martin),
        "{}",
        r.json
    );
    let r = app
        .requete(
            "POST",
            &format!("/api/admin/fournisseurs/{}/repartir", m.durand),
            Some(json!({})),
            admin,
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{}", r.json);
    assert!(r.json["deplaces"].is_number());
    m.app.fermer().await;
}

#[tokio::test]
async fn corps_html_du_mail() {
    let m = monde().await;
    let app = &m.app;
    let html = app
        .mail_vu(&commun::fixture("html-cid.eml"), &[m.boite_b])
        .await;
    let texte = app
        .mail_vu(&commun::fixture("simple.eml"), &[m.boite_a])
        .await;
    let admin = Some(m.c_admin.as_str());

    let r = app
        .requete("GET", &format!("/api/mails/{html}"), None, admin)
        .await;
    assert_eq!(r.json["html"], true);
    let r = app
        .requete("GET", &format!("/api/mails/{texte}"), None, admin)
        .await;
    assert_eq!(r.json["html"], false);

    let r = app
        .requete("GET", &format!("/api/mails/{html}/html"), None, admin)
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let entete = |n: &str| r.entetes.get(n).unwrap().to_str().unwrap().to_string();
    assert!(entete("content-type").starts_with("text/html"));
    let csp = entete("content-security-policy");
    assert!(
        csp.contains("default-src 'none'") && csp.contains("sandbox"),
        "{csp}"
    );
    assert!(
        !csp.contains("script-src") && !csp.contains("allow-scripts"),
        "{csp}"
    );
    assert!(String::from_utf8_lossy(&r.octets).contains("data:image/png;base64,"));

    // Mail texte seul : pas de page HTML ; mail d'une autre société : invisible.
    let r = app
        .requete("GET", &format!("/api/mails/{texte}/html"), None, admin)
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
    let r = app
        .requete(
            "GET",
            &format!("/api/mails/{html}/html"),
            None,
            Some(m.c_resp_a.as_str()),
        )
        .await;
    assert_eq!(r.code, StatusCode::NOT_FOUND);
}
