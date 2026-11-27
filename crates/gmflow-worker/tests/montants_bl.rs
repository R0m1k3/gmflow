//! BL qui n'imprime pas ses totaux (tableur : quantité et prix unitaire seulement) : HT, TVA
//! et TTC calculés depuis les lignes, à la lecture comme pour les BL déjà lus.

mod commun;

use commun::{Banc, WorkersTest, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::montant::{Decimal, dec};
use gmflow_core::parametre;
use serde_json::{Value, json};

/// Faux Claude qui rend une lecture de BL sans totaux (`en_plus` : lignes ajoutées aux deux
/// lignes du BL).
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new() -> Self {
        Self::avec(0)
    }

    fn avec(en_plus: usize) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-montants-bl").unwrap();
        let reponse = dossier.0.join("reponse.json");
        let script = format!("#!/bin/sh\ncat '{}'\n", reponse.display());
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ligne = |des: &str, q: f64, pu: f64, ecomob: Option<f64>| {
            json!({ "reference": null, "designation": des, "quantite": q, "unite": "UN",
                    "prix_unitaire_ht": pu, "montant_ht": null, "gencod": null, "colis": null,
                    "pcb": null, "remise_taux": null, "taux_tva": null, "deee_ht": null,
                    "ecomob_ht": ecomob, "pv_conseille": null })
        };
        let mut lignes = vec![
            ligne("PARURE 3 P. 240 x 220", 4.0, 14.9, None),
            ligne("COUSSIN 40 x 40", 6.0, 2.2, Some(0.3)),
        ];
        lignes.extend((0..en_plus).map(|i| ligne(&format!("ARTICLE {i}"), 1.0, 1.0, None)));
        let sortie = json!({
            "type": "bl", "societe_id": null,
            "fournisseur": { "nom": "U10", "siret": null, "tva": null, "groupe": null, "entite": null },
            "numero": "LL1261103672", "date": "2026-11-12", "numero_commande": null,
            "montant_ht": null, "montant_tva": null, "montant_ttc": null,
            "lignes": lignes,
            "numeros_bl": [],
            "confiance": { "type": 1, "societe": 0, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0 }
        });
        let r = json!({ "type": "result", "is_error": false, "result": "", "structured_output": sortie });
        std::fs::write(&reponse, r.to_string()).unwrap();
        Self { dossier }
    }

    fn bin(&self) -> String {
        self.dossier.0.join("claude").display().to_string()
    }
}

async fn montants(
    b: &Banc,
    doc: i64,
) -> (Option<Decimal>, Option<Decimal>, Option<Decimal>, Value) {
    sqlx::query_as(
        "select montant_ht, montant_tva, montant_ttc, confiance
         from document where id = $1",
    )
    .bind(doc)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn bl_sans_totaux_calcules_depuis_les_lignes() {
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
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_montants")
        .await
        .unwrap();
    let pdf = fixtures::pdf_texte(&[
        "U10 SAS",
        "BON DE LIVRAISON N° : LL1261103672",
        "3574386429771 1642977 PARURE 3 P. 240 x 220 4 UN 4 14.9",
        "3574386126410 1612641 COUSSIN 40 x 40 6 UN 6 2.2 0.3",
        "Livre a : Magasin Frouard, zone commerciale, 54390 Frouard",
        "Conditions generales de vente au verso - marchandise voyageant aux risques du destinataire",
    ]);
    let m = mail(
        &b,
        societe,
        &fixtures::eml(
            "ll1261103672@test",
            "BL LL1261103672",
            &[("bl.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let doc: i64 = sqlx::query_scalar("select id from document where mail_id = $1 and etat = 'lu'")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    // 4 × 14,90 + 6 × 2,20 + 6 × 0,30 = 59,60 + 13,20 + 1,80
    let (ht, tva, ttc, confiance) = montants(&b, doc).await;
    assert_eq!(
        (ht, tva, ttc),
        (Some(dec!(74.6)), Some(dec!(14.92)), Some(dec!(89.52)))
    );
    assert_eq!(confiance["montants_calcules"], json!(["ht", "tva", "ttc"]));
    // Pas d'alerte « la somme des lignes ne fait pas le total HT » (éco-participation).
    let controles: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(doc)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(controles, json!([]));
    let lignes: Vec<Option<Decimal>> = sqlx::query_scalar(
        "select montant_ht from ligne_document where document_id = $1 order by rang",
    )
    .bind(doc)
    .fetch_all(b.pool())
    .await
    .unwrap();
    assert_eq!(lignes, vec![Some(dec!(59.6)), Some(dec!(13.2))]);

    // BL lu avant ce calcul : complété au démarrage du worker, sans Claude.
    sqlx::query(
        "update document set montant_ht = null, montant_tva = null, montant_ttc = null,
                confiance = confiance - 'montants_calcules' where id = $1",
    )
    .bind(doc)
    .execute(b.pool())
    .await
    .unwrap();
    sqlx::query("update ligne_document set montant_ht = null where document_id = $1")
        .bind(doc)
        .execute(b.pool())
        .await
        .unwrap();
    assert_eq!(
        gmflow_worker::lecture::completer_bl_sans_totaux(b.pool())
            .await
            .unwrap(),
        1
    );
    let (ht, tva, ttc, confiance) = montants(&b, doc).await;
    assert_eq!(
        (ht, tva, ttc),
        (Some(dec!(74.6)), Some(dec!(14.92)), Some(dec!(89.52)))
    );
    assert_eq!(confiance["montants_calcules"], json!(["ht", "tva", "ttc"]));
    // Rien à refaire ensuite.
    assert_eq!(
        gmflow_worker::lecture::completer_bl_sans_totaux(b.pool())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn bl_coupe_ni_totaux_calcules_ni_somme_controlee() {
    // Claude rend LIGNES_MAX lignes ou plus : la lecture est sans doute coupée.
    let faux = FauxClaude::avec(gmflow_core::lecture::extraction::LIGNES_MAX);
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
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_montants")
        .await
        .unwrap();
    let pdf = fixtures::pdf_texte(&[
        "U10 SAS",
        "BON DE LIVRAISON N° : LL1261103672",
        "3574386429771 1642977 PARURE 3 P. 240 x 220 4 UN 4 14.9",
        "Livre a : Magasin Frouard, zone commerciale, 54390 Frouard",
        "Conditions generales de vente au verso - marchandise voyageant aux risques du destinataire",
    ]);
    let m = mail(
        &b,
        societe,
        &fixtures::eml(
            "ll-coupe@test",
            "BL LL1261103672",
            &[("bl.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let doc: i64 = sqlx::query_scalar("select id from document where mail_id = $1 and etat = 'lu'")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    let (ht, tva, ttc, confiance) = montants(&b, doc).await;
    assert_eq!((ht, tva, ttc), (None, None, None), "pas de total faux");
    assert_eq!(confiance["lignes_tronquees"], json!(true));
    assert!(confiance.get("montants_calcules").is_none());
    let controles: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(doc)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(controles[0]["code"], "lignes_tronquees", "{controles}");
    assert_eq!(
        gmflow_worker::lecture::completer_bl_sans_totaux(b.pool())
            .await
            .unwrap(),
        0
    );
}
