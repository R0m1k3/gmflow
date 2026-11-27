//! Lot 4 de bout en bout : lignes des BL lues par Claude (faux Claude) et des factures
//! électroniques lues par le code, rapprochement automatique et écarts.

mod commun;

use std::collections::HashMap;
use std::time::Duration;

use commun::{Banc, WorkersTest, attendre, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::parametre;
use serde_json::{Value, json};

const SIRET_FOURNISSEUR: &str = "55210055400013";

/// Faux Claude : réponse choisie par le nom du fichier cité dans le prompt.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(reponses: &[(&str, Value)]) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-rap").unwrap();
        let mut cas = String::new();
        for (i, (fichier, v)) in reponses.iter().enumerate() {
            let rep = json!({ "type": "result", "is_error": false, "result": "", "structured_output": v });
            let chemin = dossier.0.join(format!("rep{i}.json"));
            std::fs::write(&chemin, rep.to_string()).unwrap();
            cas.push_str(&format!(
                "*'Fichier : {fichier}'*) cat '{}'; exit 0;;\n",
                chemin.display()
            ));
        }
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in\n{cas}esac; done\necho '{{\"is_error\": true, \"result\": \"inconnu\"}}'\n"
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn bin(&self) -> String {
        self.dossier.0.join("claude").display().to_string()
    }
}

/// Réponse de lecture d'un BL.
fn bl(
    numero: &str,
    commande: Option<&str>,
    ht: Option<f64>,
    lignes: &[(&str, f64, Option<f64>)],
) -> Value {
    json!({
        "type": "bl", "societe_id": null,
        "fournisseur": { "nom": "Grossiste Durand & Fils", "siret": SIRET_FOURNISSEUR, "tva": null },
        "numero": numero, "date": "2026-11-10", "numero_commande": commande,
        "montant_ht": ht, "montant_tva": null, "montant_ttc": null,
        "lignes": lignes.iter().map(|(r, q, m)| json!({
            "reference": r, "designation": format!("Article {r}"), "quantite": q, "unite": null,
            "prix_unitaire_ht": null, "montant_ht": m
        })).collect::<Vec<_>>(),
        "numeros_bl": [],
        "confiance": { "type": 1, "societe": 0.5, "fournisseur": 1, "numero": 1, "date": 1, "montants": 1 }
    })
}

async fn banc(faux: &FauxClaude) -> Banc {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_rap")
        .await
        .unwrap();
    b
}

async fn societe(b: &Banc) -> i64 {
    sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

async fn fournisseur(b: &Banc) -> i64 {
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut, format_bl)
         values ('Grossiste Durand & Fils', $1, $2, 'valide', 'colonnes réf. / qté') returning id",
    )
    .bind(gmflow_core::fournisseurs::normaliser(
        "Grossiste Durand & Fils",
    ))
    .bind(SIRET_FOURNISSEUR)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

/// Mail avec un BL en PDF texte (lu par le faux Claude).
async fn mail_bl(b: &Banc, s: i64, fichier: &str) -> i64 {
    let pdf = fixtures::pdf_texte(&[
        &format!("BON DE LIVRAISON {fichier}"),
        "Grossiste Durand et Fils, livraison au magasin de Frouard",
        &format!("Client SIRET {SIRET_A}"),
        "Articles livrés selon commande, voir le détail des quantités ci-dessous",
    ]);
    mail(
        b,
        s,
        &fixtures::eml(
            &format!("{fichier}@test"),
            &format!("Livraison {fichier}"),
            &[(fichier, "application/pdf", &pdf)],
            false,
        ),
    )
    .await
}

/// Mail avec une facture Factur-X à lignes (lue par le code).
async fn mail_facture(
    b: &Banc,
    s: i64,
    numero: &str,
    commande: &str,
    bl: Option<&str>,
    lignes: &[fixtures::LigneTest<'_>],
) -> i64 {
    let xml = fixtures::xml_cii_lignes(
        numero,
        SIRET_A,
        &fixtures::aujourdhui_aaaammjj(),
        commande,
        bl,
        lignes,
    );
    let pdf = fixtures::pdf_facturx(&xml, &[&format!("Facture {numero}")]);
    mail(
        b,
        s,
        &fixtures::eml(
            &format!("{numero}@test"),
            &format!("Facture {numero}"),
            &[(&format!("{numero}.pdf"), "application/pdf", &pdf)],
            false,
        ),
    )
    .await
}

async fn doc(b: &Banc, mail_id: i64) -> i64 {
    sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(mail_id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

#[derive(Debug, sqlx::FromRow)]
struct Lien {
    facture_id: i64,
    bl_id: i64,
    methode: String,
    ecart_montant: Option<f64>,
    ecarts: Value,
}

async fn liens(b: &Banc) -> Vec<Lien> {
    sqlx::query_as(
        "select facture_id, bl_id, methode, ecart_montant::float8 as ecart_montant, ecarts
         from rapprochement where not rejete order by facture_id, bl_id",
    )
    .fetch_all(b.pool())
    .await
    .unwrap()
}

/// Attend que `n` liens existent et que plus aucun document ne soit en cours.
async fn attendre_liens(b: &Banc, n: usize) -> Vec<Lien> {
    let pool = b.pool().clone();
    attendre(Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            let (l, en_cours): (i64, i64) = sqlx::query_as(
                "select (select count(*) from rapprochement where not rejete),
                        (select count(*) from document where etat in ('a_lire', 'en_cours'))",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            l as usize >= n && en_cours == 0
        }
    })
    .await;
    // Laisse finir un éventuel calcul en cours sur le dernier document lu.
    tokio::time::sleep(Duration::from_millis(500)).await;
    liens(b).await
}

#[tokio::test]
async fn bl_et_facture_rapproches_avec_ecarts_de_quantites() {
    let faux = FauxClaude::new(&[
        (
            "BL-501.pdf",
            bl(
                "BL-501",
                Some("CMD-77"),
                None,
                &[("REF-1", 20.0, None), ("REF-2", 4.0, None)],
            ),
        ),
        (
            "BL-700.pdf",
            bl(
                "BL-700",
                Some("CMD-99"),
                Some(12.0),
                &[("REF-9", 3.0, Some(12.0))],
            ),
        ),
    ]);
    let b = banc(&faux).await;
    let s = societe(&b).await;
    fournisseur(&b).await;
    let m_bl = mail_bl(&b, s, "BL-501.pdf").await;
    let m_seul = mail_bl(&b, s, "BL-700.pdf").await;
    let m_fa = mail_facture(
        &b,
        s,
        "FA-31",
        "CMD-77",
        Some("BL-501"),
        &[("REF-1", "Seau", 20.0, 2.5), ("REF-2", "Balai", 5.0, 4.0)],
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m_bl, m_seul, m_fa]).await;
    let l = attendre_liens(&b, 1).await;
    let (d_bl, d_seul, d_fa) = (
        doc(&b, m_bl).await,
        doc(&b, m_seul).await,
        doc(&b, m_fa).await,
    );

    // Lignes enregistrées : BL par Claude, facture par le XML.
    let n: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>(
        "select document_id, count(*) from ligne_document group by document_id",
    )
    .fetch_all(b.pool())
    .await
    .unwrap()
    .into_iter()
    .collect();
    assert_eq!(n.get(&d_bl), Some(&2));
    assert_eq!(n.get(&d_fa), Some(&2));
    let (numeros, lues): (Vec<String>, bool) =
        sqlx::query_as("select numeros_bl, lignes_lues from document where id = $1")
            .bind(d_fa)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(numeros, ["BL-501"]);
    assert!(lues);

    // Un seul lien : la facture cite le BL ; le BL de l'autre commande reste seul.
    assert_eq!(l.len(), 1, "{l:?}");
    assert_eq!((l[0].facture_id, l[0].bl_id), (d_fa, d_bl));
    assert_eq!(l[0].methode, "numero_bl");
    // BL sans montant : pas d'écart de montant calculable ; un balai manque.
    assert_eq!(l[0].ecart_montant, None);
    let e = l[0].ecarts.as_array().unwrap();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0]["reference"], "REF-2");
    assert_eq!(e[0]["qte_bl"], 4.0);
    assert_eq!(e[0]["qte_facture"], 5.0);
    assert!(l.iter().all(|x| x.bl_id != d_seul));

    // Le contrôle des lignes d'un BL passe quand la somme fait le total.
    let c: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(d_seul)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(c, json!([]));

    // BL lu en partie seulement (document trop long) : plus de comparaison des lignes (le balai
    // « manquant » pourrait être dans la partie coupée), une alerte sur le BL à la place.
    sqlx::query(
        "update document set confiance = confiance || '{\"lignes_tronquees\": true}' where id = $1",
    )
    .bind(d_bl)
    .execute(b.pool())
    .await
    .unwrap();
    gmflow_core::rapprochement::recalculer(b.pool(), d_bl)
        .await
        .unwrap();
    let l = liens(&b).await;
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].ecarts, json!([]));
    let c: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(d_bl)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(c[0]["code"], "lignes_tronquees");
    w.arreter();
    b.fermer().await;
}

#[tokio::test]
async fn rapprochement_par_commande_sans_ecart_et_lien_defait_non_recree() {
    let faux = FauxClaude::new(&[
        (
            "BL-601.pdf",
            bl(
                "BL-601",
                Some("cmd 88"),
                Some(50.0),
                &[("REF-1", 20.0, Some(50.0))],
            ),
        ),
        (
            "BL-602.pdf",
            bl(
                "BL-602",
                Some("CMD-88"),
                Some(25.0),
                &[("REF-2", 5.0, Some(30.0))],
            ),
        ),
    ]);
    let b = banc(&faux).await;
    let s = societe(&b).await;
    let f = fournisseur(&b).await;
    let m_fa = mail_facture(
        &b,
        s,
        "FA-40",
        "CMD-88",
        None,
        &[("REF-1", "Seau", 20.0, 2.5), ("REF-2", "Balai", 5.0, 5.0)],
    )
    .await;
    let m1 = mail_bl(&b, s, "BL-601.pdf").await;
    let m2 = mail_bl(&b, s, "BL-602.pdf").await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m_fa, m1, m2]).await;
    let l = attendre_liens(&b, 2).await;
    let (d_fa, d1, d2) = (doc(&b, m_fa).await, doc(&b, m1).await, doc(&b, m2).await);
    assert_eq!(l.len(), 2, "{l:?}");
    assert!(
        l.iter()
            .all(|x| x.facture_id == d_fa && x.methode == "commande")
    );
    // HT facture 75 = 50 + 25 (totaux des BL) ; quantités identiques, mais le balai est à
    // 6 € sur les lignes du BL (30 € pour 5) et à 5 € sur la facture : prix changé.
    assert!(l.iter().all(|x| x.ecart_montant == Some(0.0)), "{l:?}");
    let e = &l[0].ecarts;
    assert_eq!(e.as_array().map(Vec::len), Some(1), "{e}");
    assert_eq!(e[0]["genre"], "prix");
    assert_eq!(e[0]["reference"], "REF-2");
    assert_eq!(
        (e[0]["prix_facture"].as_f64(), e[0]["prix_bl"].as_f64()),
        (Some(5.0), Some(6.0))
    );
    assert_eq!(e[0]["montant_ht"].as_f64(), Some(-5.0));
    // BL-602 : lignes 30 € pour un total de 25 € -> contrôle non bloquant.
    let c: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(d2)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(c[0]["code"], "lignes", "{c}");

    // Un humain défait le lien automatique de BL-602 : il n'est pas recréé au recalcul, et
    // l'écart de la facture change (75 - 50).
    let id: i64 = sqlx::query_scalar("select id from rapprochement where bl_id = $1")
        .bind(d2)
        .fetch_one(b.pool())
        .await
        .unwrap();
    gmflow_core::rapprochement::delier(b.pool(), id)
        .await
        .unwrap();
    gmflow_core::rapprochement::recalculer_fournisseur(b.pool(), f)
        .await
        .unwrap();
    let l = liens(&b).await;
    assert_eq!(l.len(), 1, "{l:?}");
    assert_eq!(l[0].bl_id, d1);
    assert_eq!(l[0].ecart_montant, Some(25.0));
    // Lien manuel : remis en place, et gardé par le recalcul.
    let u: i64 = sqlx::query_scalar(
        "insert into utilisateur (identifiant, hash_mdp, role) values ('u', 'x', 'admin') returning id",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    gmflow_core::rapprochement::lier(b.pool(), d_fa, d2, u)
        .await
        .unwrap();
    gmflow_core::rapprochement::recalculer_fournisseur(b.pool(), f)
        .await
        .unwrap();
    let l = liens(&b).await;
    assert_eq!(l.len(), 2, "{l:?}");
    assert_eq!(
        l.iter().find(|x| x.bl_id == d2).map(|x| x.methode.as_str()),
        Some("manuel")
    );
    assert!(l.iter().all(|x| x.ecart_montant == Some(0.0)), "{l:?}");
    // Un BL ne se relie pas à la place d'une facture.
    assert!(
        gmflow_core::rapprochement::lier(b.pool(), d1, d2, u)
            .await
            .is_err()
    );
    w.arreter();
    b.fermer().await;
}
