//! Groupes détectés tout seuls : un BL à en-tête U10 qui porte « Dép. : L3C » crée la fiche
//! du groupe U10 et celle de la marque L3C (« à confirmer »), et le document va à L3C.

mod commun;

use commun::{Banc, WorkersTest, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::{fournisseurs, parametre};
use serde_json::{Value, json};

/// TVA du groupe U10 dans ces tests (clé juste : SIREN 552 100 554).
const TVA_U10: &str = "FR96552100554";

/// Faux Claude qui rend le contenu d'un fichier, changé entre deux lectures.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new() -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-groupes").unwrap();
        let reponse = dossier.0.join("reponse.json");
        let script = format!("#!/bin/sh\ncat '{}'\n", reponse.display());
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn bin(&self) -> String {
        self.dossier.0.join("claude").display().to_string()
    }

    /// Lecture d'un BL : `fournisseur` est l'objet fournisseur de la sortie structurée.
    fn lira(&self, numero: &str, fournisseur: Value) {
        let sortie = json!({
            "type": "bl", "societe_id": null, "fournisseur": fournisseur,
            "numero": numero, "date": "2026-11-12", "numero_commande": null,
            "montant_ht": null, "montant_tva": null, "montant_ttc": null,
            "lignes": [{ "reference": "R1", "designation": "Vase", "quantite": 2.0, "unite": null,
                         "prix_unitaire_ht": null, "montant_ht": null }],
            "numeros_bl": [],
            "confiance": { "type": 1, "societe": 0, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0 }
        });
        let r = json!({ "type": "result", "is_error": false, "result": "", "structured_output": sortie });
        std::fs::write(self.dossier.0.join("reponse.json"), r.to_string()).unwrap();
    }
}

/// Lit un BL (PDF texte) reçu par mail et rend l'id du document.
async fn lire_bl(b: &Banc, societe: i64, numero: &str, lignes: &[&str]) -> i64 {
    // Assez de texte pour être lu comme un PDF texte (pas comme un scan).
    let mut l = lignes.to_vec();
    l.extend([
        "Livre a : Magasin Frouard, zone commerciale, 54390 Frouard",
        "Conditions generales de vente au verso - marchandise voyageant aux risques du destinataire",
    ]);
    let pdf = fixtures::pdf_texte(&l);
    let m = mail(
        b,
        societe,
        &fixtures::eml(
            &format!("{numero}@test"),
            &format!("BL {numero}"),
            &[("bl.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(b, &[m]).await;
    w.arreter();
    let (id, voie): (i64, Option<String>) =
        sqlx::query_as("select id, voie from document where mail_id = $1 and etat = 'lu'")
            .bind(m)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(voie.as_deref(), Some("pdf_texte"));
    id
}

#[derive(Debug, sqlx::FromRow, PartialEq)]
struct Fiche {
    id: i64,
    nom: String,
    statut: String,
    groupe_id: Option<i64>,
    noms_documents: Vec<String>,
    tva: Option<String>,
}

async fn fiche(b: &Banc, nom: &str) -> Fiche {
    let f: Option<Fiche> = sqlx::query_as(
        "select id, nom, statut, groupe_id, noms_documents, tva from fournisseur
         where nom = $1 and fusionne_dans is null",
    )
    .bind(nom)
    .fetch_optional(b.pool())
    .await
    .unwrap();
    match f {
        Some(f) => f,
        None => {
            let toutes: Vec<(i64, String, Option<i64>)> =
                sqlx::query_as("select id, nom, groupe_id from fournisseur order by id")
                    .fetch_all(b.pool())
                    .await
                    .unwrap();
            panic!("pas de fiche {nom} : {toutes:?}")
        }
    }
}

async fn fournisseur_du(b: &Banc, doc: i64) -> i64 {
    sqlx::query_scalar("select fournisseur_id from document where id = $1")
        .bind(doc)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn groupe_et_marques_crees_a_la_lecture() {
    let faux = FauxClaude::new();
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    let societe: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_groupes")
        .await
        .unwrap();

    // 1. Le BL réel : logo U10, « Dép. : L3C / EXPEDIE PAR : U10 THIZY ».
    faux.lira(
        "BL-1",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10, "groupe": "U10", "entite": "L3C" }),
    );
    let d1 = lire_bl(
        &b,
        societe,
        "BL-1",
        &[
            "U10",
            "Dep. : L3C / EXPEDIE PAR : U10 THIZY, 124 Av. des Granges 69240 THIZY LES BOURGS",
            "N EORI : FR451341465",
            "BON DE LIVRAISON BL-1",
            "R1 Vase 2",
        ],
    )
    .await;
    let u10 = fiche(&b, "U10").await;
    let l3c = fiche(&b, "L3C").await;
    assert_eq!(u10.groupe_id, None);
    assert_eq!(u10.statut, "a_confirmer");
    assert_eq!(
        u10.tva.as_deref(),
        Some(TVA_U10),
        "identifiants appris au groupe"
    );
    assert_eq!(l3c.groupe_id, Some(u10.id));
    assert_eq!(l3c.statut, "a_confirmer");
    assert_eq!(l3c.tva, None);
    assert_eq!(fournisseur_du(&b, d1).await, l3c.id);
    let (g, e): (Option<String>, Option<String>) =
        sqlx::query_as("select fournisseur_groupe, fournisseur_entite from document where id = $1")
            .bind(d1)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!((g.as_deref(), e.as_deref()), (Some("U10"), Some("L3C")));

    // 2. Un BL Homea du même groupe : nouvelle marque, toujours un seul U10.
    faux.lira(
        "BL-2",
        json!({ "nom": "U10 THIZY", "siret": null, "tva": TVA_U10, "groupe": "U10", "entite": "Homea" }),
    );
    let d2 = lire_bl(
        &b,
        societe,
        "BL-2",
        &[
            "U10",
            "Dep. : HOMEA",
            "BON DE LIVRAISON BL-2",
            "R2 Coussin 4",
        ],
    )
    .await;
    let homea = fiche(&b, "Homea").await;
    assert_eq!(homea.groupe_id, Some(u10.id));
    assert_eq!(fournisseur_du(&b, d2).await, homea.id);
    assert_eq!(
        b.compter("select count(*) from fournisseur where nom ilike 'u10%'")
            .await,
        1
    );

    // 3. « L 3 C » : la même marque, retenue comme autre écriture.
    faux.lira(
        "BL-3",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10, "groupe": "U 10", "entite": "L 3 C" }),
    );
    let d3 = lire_bl(
        &b,
        societe,
        "BL-3",
        &["U10", "BON DE LIVRAISON BL-3", "R3 Bol 6"],
    )
    .await;
    assert_eq!(fournisseur_du(&b, d3).await, l3c.id);
    assert_eq!(b.compter("select count(*) from fournisseur").await, 3);
    assert_eq!(fiche(&b, "L3C").await.noms_documents, vec!["L 3 C"]);

    // 4. Réponse sans groupe ni entité (ancien format) et aucune marque reconnue : le BL
    //    reste sur U10 (Claude ne tranche pas).
    faux.lira(
        "BL-4",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10 }),
    );
    let d4 = lire_bl(
        &b,
        societe,
        "BL-4",
        &[
            "U10",
            "Gamme DECOR10 hiver",
            "BON DE LIVRAISON BL-4",
            "R4 Guirlande 3",
        ],
    )
    .await;
    assert_eq!(fournisseur_du(&b, d4).await, u10.id);

    // 5. Règle sans Claude : « Dép. : Decor10 » sur un document du groupe crée la marque, et le
    //    BL 4 resté sur le groupe la rejoint (répartition automatique).
    faux.lira(
        "BL-5",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10, "groupe": null, "entite": null }),
    );
    let d5 = lire_bl(
        &b,
        societe,
        "BL-5",
        &[
            "U10",
            "Dep. : Decor10 / EXPEDIE PAR : U10 THIZY",
            "BON DE LIVRAISON BL-5",
            "R5 Sapin 1",
        ],
    )
    .await;
    let decor = fiche(&b, "Decor10").await;
    assert_eq!(decor.groupe_id, Some(u10.id));
    assert_eq!(decor.statut, "a_confirmer");
    assert_eq!(fournisseur_du(&b, d5).await, decor.id);
    assert_eq!(fournisseur_du(&b, d4).await, decor.id, "rattrapage");

    // 6. Deux fiches hors groupe au même SIREN, noms sans rapport : même groupe probable,
    //    jamais une fusion proposée ; les marques déjà groupées ne sont pas proposées.
    let mut ids = Vec::new();
    for nom in ["Kaemingk", "Edelman"] {
        ids.push(
            sqlx::query_scalar::<_, i64>(
                "insert into fournisseur (nom, nom_normalise, siret) values ($1, $2, '44306184100047') returning id",
            )
            .bind(nom)
            .bind(fournisseurs::normaliser(nom))
            .fetch_one(b.pool())
            .await
            .unwrap(),
        );
    }
    let d = fournisseurs::doublons_probables(b.pool()).await.unwrap();
    let paire = d
        .iter()
        .find(|p| p.a == ids[0] && p.b == ids[1])
        .unwrap_or_else(|| panic!("{d:?}"));
    assert!(paire.meme_groupe);
    assert_eq!(paire.raison, "même groupe probable");
    assert!(
        !d.iter().any(|p| [l3c.id, homea.id, decor.id].contains(&p.a)
            || [l3c.id, homea.id, decor.id].contains(&p.b)),
        "{d:?}"
    );
    b.fermer().await;
}

/// Facture U10 : Claude ne lit ni groupe ni entité, U10 n'a encore aucune marque ; le cadre
/// « Dép. : HOMEA Maison / EXPEDIE PAR : U10 Strasbourg » suffit à créer le groupe et la marque.
#[tokio::test]
async fn marque_du_cadre_sans_entite_lue() {
    let faux = FauxClaude::new();
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    let societe: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_cadre")
        .await
        .unwrap();

    faux.lira(
        "BL-H1",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10, "groupe": null, "entite": null }),
    );
    let d1 = lire_bl(
        &b,
        societe,
        "BL-H1",
        &[
            "U10",
            "Dep. : HOMEA Maison",
            "EXPEDIE PAR : U10 Strasbourg - 15 Rue du Tanin - CS 30032 - 67380 LINGOLSHEIM",
            "BON DE LIVRAISON BL-H1",
            "R1 Vase 2",
        ],
    )
    .await;
    let u10 = fiche(&b, "U10").await;
    let homea = fiche(&b, "HOMEA Maison").await;
    assert_eq!(u10.groupe_id, None);
    assert_eq!(homea.groupe_id, Some(u10.id));
    assert_eq!(fournisseur_du(&b, d1).await, homea.id);

    // « Dép. : HOMEA » ensuite : la même marque.
    faux.lira(
        "BL-H2",
        json!({ "nom": "U10", "siret": null, "tva": TVA_U10, "groupe": null, "entite": null }),
    );
    let d2 = lire_bl(
        &b,
        societe,
        "BL-H2",
        &[
            "U10",
            "Dep. : HOMEA",
            "BON DE LIVRAISON BL-H2",
            "R2 Coussin 4",
        ],
    )
    .await;
    assert_eq!(fournisseur_du(&b, d2).await, homea.id);
    assert_eq!(b.compter("select count(*) from fournisseur").await, 2);
}
