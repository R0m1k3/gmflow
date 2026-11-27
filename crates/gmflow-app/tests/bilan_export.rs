//! Export Excel du bilan du mois (audit U16) : contenu du classeur relu avec calamine, comparé
//! aux données semées et à `GET /api/bilan`, droits par société.

mod commun;

use std::io::Cursor;

use axum::http::StatusCode;
use calamine::{Data, Range, Reader, Xlsx};
use commun::Appli;
use gmflow_core::export_bilan::{self, LIGNE_ENTETES, ONGLET_BL, ONGLET_DETAIL, ONGLET_SYNTHESE};
use gmflow_core::montant::{self, dec};

const MDP: &str = "un mot de passe solide";

struct Monde {
    app: Appli,
    c_admin: String,
    c_resp_a: String,
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
    Monde {
        c_admin: app.connecter("admin", MDP).await,
        c_resp_a: app.connecter("resp-a", MDP).await,
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

fn feuilles(octets: &[u8]) -> (Range<Data>, Range<Data>, Range<Data>) {
    let mut x: Xlsx<_> = Xlsx::new(Cursor::new(octets.to_vec())).expect("xlsx lisible");
    assert_eq!(x.sheet_names(), [ONGLET_SYNTHESE, ONGLET_DETAIL, ONGLET_BL]);
    (
        x.worksheet_range(ONGLET_SYNTHESE).unwrap(),
        x.worksheet_range(ONGLET_DETAIL).unwrap(),
        x.worksheet_range(ONGLET_BL).unwrap(),
    )
}

fn texte(r: &Range<Data>, l: u32, c: u32) -> String {
    match r.get_value((l, c)) {
        Some(Data::String(s)) => s.clone(),
        Some(Data::Empty) | None => String::new(),
        v => panic!("texte attendu en ({l}, {c}) : {v:?}"),
    }
}

fn nombre(r: &Range<Data>, l: u32, c: u32) -> f64 {
    match r.get_value((l, c)) {
        Some(Data::Float(x)) => *x,
        v => panic!("nombre attendu en ({l}, {c}) : {v:?}"),
    }
}

fn date(r: &Range<Data>, l: u32, c: u32) -> String {
    match r.get_value((l, c)) {
        Some(Data::DateTime(d)) => export_bilan::depuis_serie_excel(d.as_f64()).to_string(),
        v => panic!("date attendue en ({l}, {c}) : {v:?}"),
    }
}

/// Lignes de données du détail : (société, numéro, type, TTC, état).
fn lignes_detail(d: &Range<Data>) -> Vec<(String, String, String, f64, String)> {
    let mut v = Vec::new();
    let mut l = LIGNE_ENTETES + 1;
    while !texte(d, l, 1).is_empty() {
        v.push((
            texte(d, l, 1),
            texte(d, l, 4),
            texte(d, l, 3),
            nombre(d, l, 7),
            texte(d, l, 9),
        ));
        l += 1;
    }
    v
}

#[tokio::test]
async fn export_excel_du_bilan_memes_chiffres_et_memes_droits() {
    let m = monde().await;
    let app = &m.app;
    let envoyee = piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "facture",
            numero: "FA-1",
            societe: Some(m.sa),
            statut: Some("depose"),
            date: "2026-11-03",
            ttc: 120.0,
            valide: true,
        },
    )
    .await;
    sqlx::query(
        "insert into envoi_depot (document_id, societe_id, adresse, outil, quand, etat)
         values ($1, $2, 'depot@dext.test', 'dext', '2026-11-04 08:30:00+00', 'envoye')",
    )
    .bind(envoyee)
    .bind(m.sa)
    .execute(app.pool())
    .await
    .unwrap();
    piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "avoir",
            numero: "AV-2",
            societe: Some(m.sa),
            statut: Some("depose"),
            date: "2026-11-05",
            ttc: 24.0,
            valide: true,
        },
    )
    .await;
    piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "facture",
            numero: "FA-3",
            societe: Some(m.sa),
            statut: Some("a_verifier"),
            date: "2026-11-20",
            ttc: 60.0,
            valide: false,
        },
    )
    .await;
    // Pas encore rangée, reçue par la boîte de Frouard.
    piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "facture",
            numero: "FA-4",
            societe: None,
            statut: Some("a_verifier"),
            date: "2026-11-21",
            ttc: 30.0,
            valide: false,
        },
    )
    .await;
    piece(
        &m,
        m.boite_b,
        Piece {
            type_piece: "facture",
            numero: "FB-1",
            societe: Some(m.sb),
            statut: Some("pret"),
            date: "2026-11-12",
            ttc: 240.0,
            valide: true,
        },
    )
    .await;
    // Autre mois : hors du bilan.
    piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "facture",
            numero: "FA-OCT",
            societe: Some(m.sa),
            statut: Some("depose"),
            date: "2026-10-31",
            ttc: 99.0,
            valide: true,
        },
    )
    .await;
    // BL du mois sans facture.
    piece(
        &m,
        m.boite_a,
        Piece {
            type_piece: "bl",
            numero: "BL-1",
            societe: Some(m.sa),
            statut: None,
            date: "2026-11-08",
            ttc: 50.0,
            valide: false,
        },
    )
    .await;

    // Administrateur, toutes sociétés.
    let r = app
        .requete(
            "GET",
            "/api/bilan/export?mois=2026-11",
            None,
            Some(&m.c_admin),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        r.entetes["content-type"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let dispo = r.entetes["content-disposition"].to_str().unwrap();
    assert!(dispo.contains("bilan-toutes-2026-11.xlsx"), "{dispo}");
    let (s, d, b) = feuilles(&r.octets);
    let page = app
        .requete("GET", "/api/bilan?mois=2026-11", None, Some(&m.c_admin))
        .await
        .json;
    // Synthèse = lignes de la page, dans le même ordre, puis le total.
    let lignes = page["societes"].as_array().unwrap();
    assert_eq!(lignes.len(), 3, "{page}");
    for (i, l) in lignes.iter().enumerate() {
        let n = LIGNE_ENTETES + 1 + i as u32;
        assert_eq!(
            texte(&s, n, 0),
            l["societe"].as_str().unwrap_or("Pas encore rangée")
        );
        assert_eq!(nombre(&s, n, 1), l["recues"].as_f64().unwrap());
        assert_eq!(nombre(&s, n, 2), l["deposees"].as_f64().unwrap());
        assert_eq!(
            nombre(&s, n, 3),
            l["pretes"].as_f64().unwrap() + l["a_verifier"].as_f64().unwrap()
        );
        assert_eq!(nombre(&s, n, 6), l["montant_depose"].as_f64().unwrap());
    }
    let total = LIGNE_ENTETES + 4;
    assert_eq!(texte(&s, total, 0), "Total");
    assert_eq!(nombre(&s, total, 1), 5.0);
    assert_eq!(nombre(&s, total, 2), 2.0);
    assert_eq!(nombre(&s, total, 3), 3.0);
    // 120 envoyés − 24 d'avoir.
    assert_eq!(nombre(&s, total, 6), 96.0);

    let det = lignes_detail(&d);
    assert_eq!(det.len(), 5, "{det:?}");
    let fa1 = det.iter().find(|x| x.1 == "FA-1").unwrap();
    assert_eq!(fa1.0, "La Foir'Fouille Frouard");
    assert_eq!((fa1.2.as_str(), fa1.3), ("Facture", 120.0));
    assert_eq!(fa1.4, "Envoyée à Dext");
    let av = det.iter().find(|x| x.1 == "AV-2").unwrap();
    assert_eq!((av.2.as_str(), av.3), ("Avoir", -24.0));
    assert_eq!(
        det.iter().find(|x| x.1 == "FB-1").unwrap().4,
        "Validée",
        "validée, pas encore partie"
    );
    assert_eq!(
        det.iter().find(|x| x.1 == "FA-4").unwrap().0,
        "Pas encore rangée"
    );
    assert!(!det.iter().any(|x| x.1 == "FA-OCT" || x.1 == "BL-1"));
    // Ligne de FA-1 : vraies dates, montants et date d'envoi (heure de Paris).
    let l = LIGNE_ENTETES + 1 + det.iter().position(|x| x.1 == "FA-1").unwrap() as u32;
    assert_eq!(date(&d, l, 0), "2026-11-03 0:00:00.0");
    assert_eq!(nombre(&d, l, 5), 100.0);
    assert_eq!(nombre(&d, l, 6), 20.0);
    assert_eq!(date(&d, l, 8), "2026-12-03 0:00:00.0");
    assert_eq!(date(&d, l, 10), "2026-11-04 9:30:00.0");
    // BL en attente de facture.
    assert_eq!(texte(&b, LIGNE_ENTETES + 1, 1), "Durand");
    assert_eq!(nombre(&b, LIGNE_ENTETES + 1, 2), 1.0);

    // Administrateur, une société choisie : seulement elle.
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan/export?mois=2026-11&societe={}", m.sb),
            None,
            Some(&m.c_admin),
        )
        .await;
    let dispo = r.entetes["content-disposition"].to_str().unwrap();
    assert!(
        dispo.contains("bilan-la-foir-fouille-houdemont-2026-11.xlsx"),
        "{dispo}"
    );
    let (_, d, _) = feuilles(&r.octets);
    let det = lignes_detail(&d);
    assert_eq!(det.len(), 1);
    assert_eq!(det[0].1, "FB-1");

    // Responsable de Frouard : sa société seulement (avec ce qui n'est pas encore rangé).
    let r = app
        .requete(
            "GET",
            "/api/bilan/export?mois=2026-11",
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::OK);
    let dispo = r.entetes["content-disposition"].to_str().unwrap();
    assert!(
        dispo.contains("bilan-la-foir-fouille-frouard-2026-11.xlsx"),
        "{dispo}"
    );
    let (s, d, _) = feuilles(&r.octets);
    let mut numeros: Vec<String> = lignes_detail(&d).into_iter().map(|x| x.1).collect();
    numeros.sort();
    assert_eq!(numeros, ["AV-2", "FA-1", "FA-3", "FA-4"]);
    assert_eq!(texte(&s, LIGNE_ENTETES + 1, 0), "La Foir'Fouille Frouard");
    assert_eq!(texte(&s, LIGNE_ENTETES + 2, 0), "Total");
    // Une autre société que la sienne : refusé.
    let r = app
        .requete(
            "GET",
            &format!("/api/bilan/export?mois=2026-11&societe={}", m.sb),
            None,
            Some(&m.c_resp_a),
        )
        .await;
    assert_eq!(r.code, StatusCode::FORBIDDEN);
    // Mois mal formé, pas connecté.
    assert_eq!(
        app.requete(
            "GET",
            "/api/bilan/export?mois=2026-13",
            None,
            Some(&m.c_admin)
        )
        .await
        .code,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.requete("GET", "/api/bilan/export", None, None)
            .await
            .code,
        StatusCode::UNAUTHORIZED
    );
    m.app.fermer().await;
}
