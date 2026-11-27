//! Tableurs : lignes d'articles lues dans les cellules par le code, Claude pour l'en-tête du
//! document seulement ; correspondance des colonnes demandée une fois puis gardée.

mod commun;

use commun::{Banc, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::montant::{Decimal, dec};
use gmflow_core::parametre;
use gmflow_worker::lecture;
use serde_json::{Value, json};

/// Faux Claude : journalise chaque appel, rend `colonnes` à la question des colonnes et
/// `lecture` sinon.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(lecture: &str, colonnes: &str) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-tab").unwrap();
        std::fs::write(dossier.0.join("lecture.json"), lecture).unwrap();
        std::fs::write(dossier.0.join("colonnes.json"), colonnes).unwrap();
        let d = dossier.0.display();
        let script = format!(
            "#!/bin/sh\n{{ printf '%s\\n' \"$*\"; echo ===; }} >> '{d}/journal'\n\
             case \"$*\" in *'cellule par le numéro de sa colonne'*) cat '{d}/colonnes.json' ;; \
             *) cat '{d}/lecture.json' ;; esac\n"
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn appels(&self) -> Vec<String> {
        std::fs::read_to_string(self.dossier.0.join("journal"))
            .unwrap_or_default()
            .split("===\n")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

fn reponse(sortie: &Value) -> String {
    json!({ "type": "result", "is_error": false, "result": "", "structured_output": sortie })
        .to_string()
}

/// En-tête d'un BL lu par Claude : aucune ligne (le code les lit), totaux absents.
fn bl_sans_lignes() -> String {
    reponse(&json!({
        "type": "bl", "societe_id": null,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": "FR96552100554" },
        "numero": "BL-77", "date": "2026-11-12", "numero_commande": null,
        "montant_ht": null, "montant_tva": null, "montant_ttc": null,
        "lignes": [],
        "confiance": { "type": 0.95, "societe": 0.5, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0.9 }
    }))
}

async fn banc(faux: &FauxClaude) -> Banc {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.dossier.0.join("claude").display().to_string();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_tab")
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

/// Lit le seul document du mail (pompe puis lecture directe) ; rend son id.
async fn lire_le_document(b: &Banc, m: i64) -> i64 {
    lecture::pompe(b.pool()).await.unwrap();
    let id: i64 = sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    lecture::lire(&b.ctx, id, false, false, false)
        .await
        .unwrap();
    id
}

fn xlsx(lignes: &[Vec<String>]) -> Vec<u8> {
    let l: Vec<Vec<&str>> = lignes
        .iter()
        .map(|l| l.iter().map(String::as_str).collect())
        .collect();
    let l: Vec<&[&str]> = l.iter().map(Vec::as_slice).collect();
    fixtures::xlsx(&l)
}

fn v(cellules: &[&str]) -> Vec<String> {
    cellules.iter().map(|c| c.to_string()).collect()
}

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    etat: String,
    numero: Option<String>,
    montant_ht: Option<Decimal>,
    confiance: Value,
    message: Option<String>,
}

async fn doc(b: &Banc, id: i64) -> Doc {
    sqlx::query_as(
        "select etat, numero, montant_ht, confiance, message
         from document where id = $1",
    )
    .bind(id)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

async fn nb_lignes(b: &Banc, id: i64) -> i64 {
    b.compter(&format!(
        "select count(*) from ligne_document where document_id = {id}"
    ))
    .await
}

#[tokio::test]
async fn six_cents_lignes_lues_dans_les_cellules_sans_coupure() {
    let faux = FauxClaude::new(&bl_sans_lignes(), "{}");
    let b = banc(&faux).await;
    let a = societe(&b).await;
    // En-tête décalé (logo, adresse, n° de BL au-dessus), 600 lignes, ligne de total exclue.
    let mut l = vec![
        v(&["DURAND SAS"]),
        v(&["Bon de livraison n° BL-77 du 12/11/2026"]),
        v(&[]),
        v(&[
            "Réf.",
            "Désignation",
            "EAN",
            "Qté livrée",
            "P.U. HT",
            "Montant HT",
        ]),
    ];
    for i in 0..600 {
        l.push(v(&[
            &format!("R{i:03}"),
            &format!("Article numéro {i}"),
            "3760123456789",
            "2",
            "1.25",
            "2.5",
        ]));
    }
    l.push(v(&["", "Total HT", "", "1200", "", "1500"]));
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "tab1@test",
            "BL",
            &[(
                "bl.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &xlsx(&l),
            )],
            false,
        ),
    )
    .await;
    let id = lire_le_document(&b, m).await;
    let d = doc(&b, id).await;
    assert_eq!(
        (d.etat.as_str(), d.numero.as_deref()),
        ("lu", Some("BL-77"))
    );
    assert_eq!(nb_lignes(&b, id).await, 600);
    assert_eq!(d.confiance["lignes_source"], "cellules");
    assert_eq!(d.confiance["colonnes_source"], "entete");
    assert!(d.confiance.get("lignes_tronquees").is_none());
    // Totaux calculés depuis les lignes (BL sans totaux) : 600 × 2,50.
    assert_eq!(d.montant_ht, Some(dec!(1500)));
    assert_eq!(d.message, None);
    let ligne: (
        Option<String>,
        Option<Decimal>,
        Option<Decimal>,
        Option<String>,
    ) = sqlx::query_as(
        "select reference, quantite, prix_unitaire_ht, gencod from ligne_document
         where document_id = $1 and rang = 599",
    )
    .bind(id)
    .fetch_one(b.pool())
    .await
    .unwrap();
    assert_eq!(
        ligne,
        (
            Some("R599".into()),
            Some(dec!(2)),
            Some(dec!(1.25)),
            Some("3760123456789".into())
        )
    );
    let appels = faux.appels();
    assert_eq!(appels.len(), 1, "un seul appel : l'en-tête du document");
    // Extrait court : en-tête et totaux, pas les lignes du milieu.
    assert!(appels[0].contains("BL-77") && appels[0].contains("Total HT"));
    assert!(
        !appels[0].contains("Article numéro 300"),
        "lignes recopiées dans le prompt"
    );
    assert!(appels[0].contains("lignes = []"));
    b.fermer().await;
}

#[tokio::test]
async fn correspondance_demandee_une_fois_puis_reutilisee() {
    let colonnes = reponse(&json!({
        "ligne_entete": 1,
        "colonnes": {
            "reference": 0, "gencod": null, "designation": 1, "quantite": 2, "unite": null,
            "colis": null, "pcb": null, "prix_unitaire_ht": 3, "montant_ht": null,
            "remise_taux": null, "taux_tva": null, "deee_ht": null, "ecomob_ht": null,
            "pv_conseille": null
        }
    }));
    let faux = FauxClaude::new(&bl_sans_lignes(), &colonnes);
    let b = banc(&faux).await;
    let a = societe(&b).await;
    // Intitulés maison, non reconnus par le code.
    let gabarit = |n: usize| {
        let mut l = vec![v(&["Envoi DURAND"]), v(&["Art", "Lib", "Nb", "Val"])];
        for i in 0..n {
            l.push(v(&[&format!("X{i}"), &format!("Vase {i}"), "3", "4.5"]));
        }
        xlsx(&l)
    };
    let m1 = mail(
        &b,
        a,
        &fixtures::eml("tab2@test", "BL", &[("bl.xlsx", "", &gabarit(5))], false),
    )
    .await;
    let id1 = lire_le_document(&b, m1).await;
    let d1 = doc(&b, id1).await;
    assert_eq!(d1.etat, "lu");
    assert_eq!(nb_lignes(&b, id1).await, 5);
    assert_eq!(d1.confiance["colonnes_source"], "claude");
    assert_eq!(faux.appels().len(), 2, "colonnes puis en-tête");
    // Gardée sur la fiche fournisseur, sous l'empreinte des intitulés.
    let garde: Value = sqlx::query_scalar(
        "select f.colonnes_tableur from fournisseur f join document d on d.fournisseur_id = f.id
         where d.id = $1",
    )
    .bind(id1)
    .fetch_one(b.pool())
    .await
    .unwrap();
    let cles: Vec<&String> = garde.as_object().unwrap().keys().collect();
    assert_eq!(cles.len(), 1, "{garde}");
    assert_eq!(garde[cles[0]]["quantite"], 2);

    // Même gabarit, autre BL : aucune question sur les colonnes.
    let m2 = mail(
        &b,
        a,
        &fixtures::eml("tab3@test", "BL", &[("bl2.xlsx", "", &gabarit(8))], false),
    )
    .await;
    let id2 = lire_le_document(&b, m2).await;
    assert_eq!(nb_lignes(&b, id2).await, 8);
    assert_eq!(doc(&b, id2).await.confiance["colonnes_source"], "apprise");
    let appels = faux.appels();
    assert_eq!(appels.len(), 3, "seulement l'en-tête du second BL");
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'colonnes_tableur'")
            .await,
        1
    );
    // Mesure (L18) : l'appel est rattaché au premier BL, dont il a lu l'en-tête.
    assert_eq!(
        b.compter(&format!(
            "select count(*) from appel_claude where tache = 'colonnes_tableur' and document_id = {id1}"
        ))
        .await,
        1
    );
    b.fermer().await;
}

#[tokio::test]
async fn entete_introuvable_la_voie_texte_reprend() {
    // Claude ne trouve pas de tableau : lecture du texte comme avant, lignes de Claude.
    let colonnes = reponse(&json!({
        "ligne_entete": null,
        "colonnes": {
            "reference": null, "gencod": null, "designation": null, "quantite": null,
            "unite": null, "colis": null, "pcb": null, "prix_unitaire_ht": null,
            "montant_ht": null, "remise_taux": null, "taux_tva": null, "deee_ht": null,
            "ecomob_ht": null, "pv_conseille": null
        }
    }));
    let mut lecture: Value = serde_json::from_str(&bl_sans_lignes()).unwrap();
    lecture["structured_output"]["lignes"] = json!([{
        "reference": "Z1", "designation": "Lampe", "quantite": 1, "unite": null,
        "prix_unitaire_ht": 10, "montant_ht": 10, "gencod": null, "colis": null, "pcb": null,
        "remise_taux": null, "taux_tva": null, "deee_ht": null, "ecomob_ht": null,
        "pv_conseille": null
    }]);
    let faux = FauxClaude::new(&lecture.to_string(), &colonnes);
    let b = banc(&faux).await;
    let a = societe(&b).await;
    let l = vec![
        v(&["Notre envoi", "du 12/11"]),
        v(&["Z1 Lampe", "un", "à dix euros", "x"]),
        v(&["voir pièce", "jointe", "merci", "y"]),
    ];
    let m = mail(
        &b,
        a,
        &fixtures::eml("tab4@test", "BL", &[("bl.xlsx", "", &xlsx(&l))], false),
    )
    .await;
    let id = lire_le_document(&b, m).await;
    let d = doc(&b, id).await;
    assert_eq!(d.etat, "lu");
    assert!(
        d.confiance.get("lignes_source").is_none(),
        "{}",
        d.confiance
    );
    assert_eq!(nb_lignes(&b, id).await, 1);
    let appels = faux.appels();
    assert_eq!(appels.len(), 2);
    assert!(!appels[1].contains("lignes = []"));
    b.fermer().await;
}
