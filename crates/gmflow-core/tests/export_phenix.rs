//! Export des BL vers BO Phénix : quantités, prix, lots, fichiers ; recherche des articles
//! au magasin avec une fausse API.

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx};
use gmflow_core::articles_magasin::{self, ArticleMagasin, Composant, Source};
use gmflow_core::export_phenix::{
    self, COLONNES_MATRICE, COLONNES_TRAME, Cellule, LigneBl, Rangee,
};
use serde_json::{Value, json};

fn val<'a>(r: &'a Rangee, col: &str) -> Option<&'a Cellule> {
    r.iter().find(|(c, _)| *c == col).map(|(_, v)| v)
}

fn num(r: &Rangee, col: &str) -> Option<f64> {
    match val(r, col) {
        Some(Cellule::Nombre(x)) => Some(*x),
        _ => None,
    }
}

fn txt<'a>(r: &'a Rangee, col: &str) -> Option<&'a str> {
    match val(r, col) {
        Some(Cellule::Texte(t)) => Some(t),
        _ => None,
    }
}

fn composant(codein: &str, quantite: f64, pa: f64, pv: f64) -> Composant {
    Composant {
        no_id: format!("n{codein}"),
        codein: Some(codein.into()),
        libelle: Some(format!("ARTICLE {codein}")),
        gencod: Some(format!("326231308{codein}")),
        reference: Some(format!("REF-{codein}")),
        quantite,
        pa: Some(pa),
        pv: Some(pv),
        eco_ht: Some(0.04),
        famille: Some("310902".into()),
        ..Default::default()
    }
}

/// Ligne de lot : 1 lot = 1 S + 1 M + 2 L.
fn ligne_lot(prix: Option<f64>) -> LigneBl {
    LigneBl {
        rang: 0,
        reference: Some("03/3484".into()),
        designation: Some("BOITE PAULOWNIA LOT DE 3".into()),
        colis: Some(2.0),
        pcb: Some(6.0),
        prix_unitaire_ht: prix,
        ecomob_ht: Some(0.19),
        magasin: Some(ArticleMagasin {
            par: "gencod".into(),
            no_id: "132319".into(),
            codein: Some("440088".into()),
            libelle: Some("BOITE PAULOWNIA TRADI LOT DE 3".into()),
            gencod: None,
            composants: vec![
                composant("440110", 1.0, 2.0, 4.99),
                composant("440111", 1.0, 3.0, 6.99),
                composant("440112", 2.0, 2.0, 5.99),
            ],
        }),
        ..Default::default()
    }
}

#[test]
fn quantite_en_unites_colis_fois_pcb() {
    let l = LigneBl {
        quantite: Some(3.0),
        colis: Some(3.0),
        pcb: Some(12.0),
        ..Default::default()
    };
    assert_eq!(export_phenix::unites(&l), Some(36.0));
    let l = LigneBl {
        quantite: Some(5.0),
        ..Default::default()
    };
    assert_eq!(export_phenix::unites(&l), Some(5.0));
}

#[test]
fn prix_par_colis_ramene_a_l_unite() {
    // 3 colis de 12 à 24 € le colis, remise 10 % : montant 64,80 = 24 × 3 × 0,9.
    let l = LigneBl {
        colis: Some(3.0),
        pcb: Some(12.0),
        prix_unitaire_ht: Some(24.0),
        remise_taux: Some(10.0),
        montant_ht: Some(64.8),
        ..Default::default()
    };
    assert_eq!(export_phenix::prix_achat(&l), (Some(2.0), Some(10.0)));
    // Même BL avec un prix unitaire : 36 × 2 × 0,9 = 64,80.
    let l = LigneBl {
        prix_unitaire_ht: Some(2.0),
        ..l
    };
    assert_eq!(export_phenix::prix_achat(&l), (Some(2.0), Some(10.0)));
    // Sans prix unitaire : montant net / quantité, sans remise en plus.
    let l = LigneBl {
        quantite: Some(4.0),
        montant_ht: Some(10.0),
        remise_taux: Some(5.0),
        ..Default::default()
    };
    assert_eq!(export_phenix::prix_achat(&l), (Some(2.5), None));
}

#[test]
fn prix_des_composants_reequilibres() {
    let c = ligne_lot(None).magasin.unwrap().composants;
    // Prix du magasin : 2 + 3 + 2 × 2 = 9 ; le BL dit 9 → inchangés.
    assert_eq!(
        export_phenix::prix_composants(Some(9.0), &c),
        vec![Some(2.0), Some(3.0), Some(2.0)]
    );
    // Le BL dit 18 : tout double, le total redonne le prix du lot.
    let p = export_phenix::prix_composants(Some(18.0), &c);
    assert_eq!(p, vec![Some(4.0), Some(6.0), Some(4.0)]);
    let total: f64 = p.iter().zip(&c).map(|(p, c)| p.unwrap() * c.quantite).sum();
    assert!((total - 18.0).abs() < 1e-9);
    // Sans prix d'achat au magasin : au prorata des prix de vente.
    let sans_pa: Vec<Composant> = c
        .iter()
        .map(|x| Composant {
            pa: None,
            ..x.clone()
        })
        .collect();
    let p = export_phenix::prix_composants(Some(23.96), &sans_pa);
    let total: f64 = p
        .iter()
        .zip(&sans_pa)
        .map(|(p, c)| p.unwrap() * c.quantite)
        .sum();
    assert!((total - 23.96).abs() < 0.001, "{total}");
    // Sans prix du lot : ceux du magasin.
    assert_eq!(
        export_phenix::prix_composants(None, &c),
        vec![Some(2.0), Some(3.0), Some(2.0)]
    );
}

#[test]
fn matrice_remplace_le_lot_par_ses_composants() {
    let simple = LigneBl {
        rang: 1,
        reference: Some("AB123".into()),
        gencod: Some("3532156987412".into()),
        quantite: Some(4.0),
        prix_unitaire_ht: Some(5.2),
        remise_taux: Some(5.0),
        ..Default::default()
    };
    let e = export_phenix::matrice(&[ligne_lot(Some(18.0)), simple]);
    assert_eq!(e.rangees.len(), 4);
    let l = &e.rangees[2];
    assert_eq!(txt(l, "CODEIN"), Some("440112"));
    assert_eq!(txt(l, "REFERENCE"), Some("REF-440112"));
    // 2 colis × 6 lots × 2 par lot.
    assert_eq!(num(l, "QTECDE"), Some(24.0));
    assert_eq!(num(l, "PA"), Some(4.0));
    let s = &e.rangees[3];
    assert_eq!(txt(s, "GENCOD"), Some("3532156987412"));
    assert_eq!(txt(s, "CODEIN"), None);
    assert_eq!(num(s, "QTECDE"), Some(4.0));
    assert_eq!(num(s, "PA"), Some(5.2));
    assert_eq!(num(s, "REMISE "), Some(5.0));
    assert!(e.alertes.is_empty(), "{:?}", e.alertes);
}

#[test]
fn trame_lot_de_vente_et_nouveau_produit() {
    let nouveau = LigneBl {
        rang: 1,
        reference: Some("XZ9".into()),
        designation: Some("LAMPE LED".into()),
        gencod: Some("3700000000017".into()),
        quantite: Some(6.0),
        pcb: Some(6.0),
        prix_unitaire_ht: Some(7.5),
        taux_tva: Some(5.5),
        deee_ht: Some(0.1),
        pv_conseille: Some(19.99),
        ..Default::default()
    };
    let e = export_phenix::trame(&[ligne_lot(Some(18.0)), nouveau], Some("R0041"));
    assert_eq!(e.rangees.len(), 5);
    let mere = &e.rangees[0];
    assert_eq!(num(mere, "NUMERO LIGNE"), Some(1.0));
    assert_eq!(txt(mere, "Codein"), Some("440088"));
    assert_eq!(txt(mere, "Fournisseur"), Some("R0041"));
    assert_eq!(num(mere, "NUM LOT VENTE"), None);
    let c = &e.rangees[3];
    assert_eq!(num(c, "NUMERO LIGNE"), Some(4.0));
    assert_eq!(num(c, "NUM LOT VENTE"), Some(1.0));
    assert_eq!(num(c, "QTE LOT VENTE"), Some(2.0));
    assert_eq!(num(c, "PrixAchat"), Some(4.0));
    assert_eq!(num(c, "Prix de Vente"), Some(5.99));
    assert_eq!(txt(c, "CodeFamille"), Some("310902"));
    // Le BL donne de l'Ecomob pour le lot : celui du magasin pour chaque composant.
    assert_eq!(num(c, "Ecomob HT"), Some(0.04));
    assert_eq!(num(c, "TauxTva"), Some(20.0));
    let n = &e.rangees[4];
    assert_eq!(num(n, "NUMERO LIGNE"), Some(5.0));
    assert_eq!(txt(n, "Codein"), None);
    assert_eq!(num(n, "PrixAchat"), Some(7.5));
    assert_eq!(num(n, "Prix de Vente"), None);
    assert_eq!(num(n, "TauxTva"), Some(5.5));
    assert_eq!(num(n, "DEEE HT"), Some(0.1));
    assert_eq!(num(n, "PV_Conseille"), Some(19.99));
    assert_eq!(num(n, "Colisage"), Some(6.0));
    assert!(
        e.alertes
            .iter()
            .any(|a| a.rang == Some(1) && a.message.contains("nouveau produit"))
    );
    // Sans code BO Phénix : alerte générale.
    let e = export_phenix::trame(&[ligne_lot(Some(18.0))], None);
    assert!(e.alertes.iter().any(|a| a.rang.is_none()));
}

fn lire(octets: Vec<u8>) -> (Vec<String>, Vec<Vec<Data>>) {
    let mut x: Xlsx<_> = Xlsx::new(Cursor::new(octets)).unwrap();
    let noms = x.sheet_names().to_vec();
    let r = x.worksheet_range(&noms[0]).unwrap();
    (noms, r.rows().map(|l| l.to_vec()).collect())
}

#[test]
fn fichiers_aux_en_tetes_du_modele() {
    let e = export_phenix::matrice(&[ligne_lot(Some(18.0))]);
    let (onglets, lignes) = lire(export_phenix::xlsx_matrice(&e).unwrap());
    assert_eq!(onglets, vec!["Feuil1"]);
    let entetes: Vec<String> = lignes[0].iter().map(|c| c.to_string()).collect();
    assert_eq!(entetes, COLONNES_MATRICE);
    assert_eq!(entetes[5], "REMISE ");
    assert_eq!(lignes.len(), 4);
    assert_eq!(lignes[1][1], Data::String("326231308440110".into()));
    assert_eq!(lignes[1][3], Data::Float(12.0));

    let e = export_phenix::trame(&[ligne_lot(Some(18.0))], Some("R0041"));
    let (onglets, lignes) = lire(export_phenix::xlsx_trame(&e).unwrap());
    assert_eq!(onglets[0], "TRAME MODELE");
    assert_eq!(lignes[0].len(), COLONNES_TRAME.len());
    let col = |n: &str| COLONNES_TRAME.iter().position(|c| *c == n).unwrap();
    assert_eq!(lignes[2][col("NUM LOT VENTE")], Data::Float(1.0));
    assert_eq!(lignes[2][col("Fournisseur")], Data::String("R0041".into()));
}

// ---------- Recherche au magasin ----------

/// Fausse API : réponse par (chemin, paramètres triés) ; journal des appels.
struct Fausse {
    reponses: HashMap<String, Value>,
    appels: std::sync::Mutex<Vec<String>>,
}

fn cle(chemin: &str, params: &[(&str, &str)]) -> String {
    let mut p: Vec<String> = params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    p.sort();
    format!("{chemin}?{}", p.join("&"))
}

impl Source for Fausse {
    async fn obtenir(&self, chemin: &str, params: &[(&str, &str)]) -> Result<Value, String> {
        let k = cle(chemin, params);
        self.appels.lock().unwrap().push(k.clone());
        Ok(self
            .reponses
            .get(&k)
            .cloned()
            .unwrap_or(json!({ "articles": [], "lots": [] })))
    }
}

fn fausse() -> Fausse {
    let mut r = HashMap::new();
    // Par gencod : préférentiel exact.
    r.insert(
        cle("/articles", &[("ean", "3262313080538"), ("limit", "20")]),
        json!({ "articles": [{ "no_id": "132341", "codein": "440110", "libelle1": "BOITE S", "gtin": "3262313080538" }] }),
    );
    // Le lot : son gencod n'est pas le GTIN préférentiel, il est sur sa fiche.
    r.insert(
        cle("/articles", &[("ean", "3700000034840"), ("limit", "20")]),
        json!({ "articles": [
            { "no_id": "132319", "codein": "440088", "libelle1": "BOITE PAULOWNIA TRADI LOT DE 3", "gtin": "3700000099999" }
        ] }),
    );
    r.insert(
        cle("/articles/132319", &[]),
        json!({ "gtins": [{ "gtin": "3700000099999", "preferentiel": 1 }, { "gtin": "3700000034840", "preferentiel": 0 }] }),
    );
    r.insert(
        cle("/appro/article/132319", &[]),
        json!({ "lots": [
            { "art_lot_noid": "132341", "codein": "440110", "libelle1": "BOITE S", "quantite": "1.000", "pourc_marge": null },
            { "art_lot_noid": "132343", "codein": "440112", "libelle1": "BOITE L", "quantite": "2.000", "pourc_marge": "40" }
        ] }),
    );
    for (id, codein, pa, eco) in [
        ("132341", "440110", "2.1100", "0.0400"),
        ("132343", "440112", "3.4000", null_eco()),
    ] {
        r.insert(
            cle(&format!("/articles/{id}/referentiel"), &[]),
            json!({
                "article": { "codein": codein, "libelle1": format!("BOITE {codein}"), "lib_ticket": "BOITE",
                             "eco_ht": eco, "pv_conseille": "4.9900", "nom_code": "310902" },
                "gtins": [{ "gtin": format!("326{codein}"), "preferentiel": 1 }],
                "prix": { "achat": pa, "vente_par_site": [{ "site": "000", "pv": "4.9900" }] },
                "fournisseurs": [
                    { "codefou": "X0001", "ref_fou": "AUTRE", "preference": 1 },
                    { "codefou": "R0041", "ref_fou": format!("03/3484-{codein}"), "preference": 2 }
                ]
            }),
        );
    }
    Fausse {
        reponses: r,
        appels: Default::default(),
    }
}

fn null_eco() -> &'static str {
    "0"
}

#[tokio::test]
async fn recherche_par_gencod_avec_detail_du_lot() {
    let s = fausse();
    let codes = vec!["R0041".to_string()];
    let mut cache = HashMap::new();

    let a = articles_magasin::chercher(&s, Some("3262 3130 80538"), &codes, &mut cache)
        .await
        .unwrap()
        .expect("trouvé par gencod");
    assert_eq!(a.par, "gencod");
    assert_eq!(a.codein.as_deref(), Some("440110"));
    assert!(a.composants.is_empty());

    let a = articles_magasin::chercher(&s, Some("3700000034840"), &codes, &mut cache)
        .await
        .unwrap()
        .expect("lot trouvé par un gencod non préférentiel");
    assert_eq!(a.par, "gencod");
    assert_eq!(a.gencod.as_deref(), Some("3700000034840"));
    assert_eq!(a.codein.as_deref(), Some("440088"));
    assert_eq!(a.composants.len(), 2);
    let l = &a.composants[1];
    assert_eq!(l.codein.as_deref(), Some("440112"));
    assert_eq!(l.quantite, 2.0);
    assert_eq!(l.pa, Some(3.4));
    assert_eq!(l.pv, Some(4.99));
    assert_eq!(l.eco_ht, None);
    assert_eq!(l.pourc_marge, Some(40.0));
    assert_eq!(l.gencod.as_deref(), Some("326440112"));
    // Référence chez notre fournisseur, pas celle du fournisseur préféré.
    assert_eq!(l.reference.as_deref(), Some("03/3484-440112"));
    assert_eq!(a.composants[0].eco_ht, Some(0.04));

    // Inconnu : rien.
    let a = articles_magasin::chercher(&s, Some("1234567890123"), &codes, &mut cache)
        .await
        .unwrap();
    assert!(a.is_none());
}

#[test]
fn codes_bo_multiples() {
    assert_eq!(
        articles_magasin::codes(Some("J009, J012")),
        vec!["J009", "J012"]
    );
    assert!(articles_magasin::codes(None).is_empty());
}

/// Contre le vrai serveur : `GMFLOW_TEST_APIFLOW=https://api.ffnancy.fr` et
/// `GMFLOW_TEST_APIFLOW_CLE`, puis `cargo test -- --ignored vrai_serveur`.
#[tokio::test]
#[ignore]
async fn vrai_serveur() {
    let (Ok(adresse), Ok(cle)) = (
        std::env::var("GMFLOW_TEST_APIFLOW"),
        std::env::var("GMFLOW_TEST_APIFLOW_CLE"),
    ) else {
        return;
    };
    let s = articles_magasin::Http::new(&adresse, Some(cle));
    let codes = vec!["R0041".to_string()];
    let mut cache = HashMap::new();
    for g in ["3262313080538"] {
        let a = articles_magasin::chercher(&s, Some(g), &codes, &mut cache)
            .await
            .unwrap();
        println!("{g} => {}", serde_json::to_string_pretty(&a).unwrap());
    }
}

#[test]
fn hors_lot_ni_codein_ni_donnees_du_magasin() {
    // Article retrouvé au magasin, mais le BL n'imprime pas son gencod : rien du magasin.
    let l = LigneBl {
        rang: 0,
        reference: Some("AB123".into()),
        designation: Some("SEAU".into()),
        quantite: Some(4.0),
        prix_unitaire_ht: Some(5.2),
        magasin: Some(ArticleMagasin {
            par: "gencod".into(),
            no_id: "1".into(),
            codein: Some("565000".into()),
            libelle: Some("SEAU 10 L".into()),
            gencod: Some("3532156987412".into()),
            composants: Vec::new(),
        }),
        ..Default::default()
    };
    let m = export_phenix::matrice(std::slice::from_ref(&l));
    let r = &m.rangees[0];
    assert_eq!(txt(r, "CODEIN"), None);
    assert_eq!(txt(r, "GENCOD"), None);
    assert_eq!(txt(r, "REFERENCE"), Some("AB123"));
    assert!(m.alertes.is_empty(), "{:?}", m.alertes);
    let t = export_phenix::trame(&[l], Some("R0041"));
    let r = &t.rangees[0];
    assert_eq!(txt(r, "Codein"), None);
    assert_eq!(txt(r, "Gencod"), None);
    assert_eq!(txt(r, "LIBELLE"), Some("SEAU"));
}
