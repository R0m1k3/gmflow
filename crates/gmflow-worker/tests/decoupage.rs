//! PDF de plusieurs pièces : découpé en parties lues chacune à part ; une facture de plusieurs
//! pages ne l'est jamais ; une relecture ne recrée rien.

mod commun;

use commun::{Banc, mail};
use gmflow_core::fixtures::{self, PageTest, SIRET_A};
use gmflow_core::lecture::decoupage;
use gmflow_core::{parametre, stockage};
use gmflow_worker::lecture;

/// Faux Claude : rend une facture dont le numéro est le premier « FACTURE No X » du prompt, à
/// défaut un BL de numéro « BL No X ». (Texte ASCII : les PDF de test sont en WinAnsi.)
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new() -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-dec").unwrap();
        let d = dossier.0.display();
        let script = format!(
            r#"#!/bin/sh
echo appel >> '{d}/journal'
num=$(printf '%s' "$*" | grep -o 'FACTURE No [A-Z0-9-]*' | head -1 | cut -d' ' -f3)
type=facture
if [ -z "$num" ]; then num=$(printf '%s' "$*" | grep -o 'BL No [A-Z0-9-]*' | head -1 | cut -d' ' -f3); type=bl; fi
cat <<FIN
{{"type":"result","is_error":false,"result":"","structured_output":{{"type":"$type","societe_id":null,
"fournisseur":{{"nom":"Durand","siret":null,"tva":null}},"numero":"$num","date":"2026-11-12",
"numero_commande":null,"montant_ht":50.0,"montant_tva":10.0,"montant_ttc":60.0,
"lignes":[{{"reference":"A1","designation":"Vase","quantite":1,"unite":null,"prix_unitaire_ht":50,
"montant_ht":50,"gencod":null,"colis":null,"pcb":null,"remise_taux":null,"taux_tva":null,
"deee_ht":null,"ecomob_ht":null,"pv_conseille":null}}],
"confiance":{{"type":1,"societe":0.5,"fournisseur":1,"numero":1,"date":1,"montants":1}}}}}}
FIN
"#
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn appels(&self) -> usize {
        std::fs::read_to_string(self.dossier.0.join("journal"))
            .unwrap_or_default()
            .lines()
            .count()
    }
}

async fn banc(faux: &FauxClaude) -> (Banc, i64) {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.dossier.0.join("claude").display().to_string();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_dec")
        .await
        .unwrap();
    let a: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    (b, a)
}

/// Lit tous les documents à lire du mail (directement, sans workers), jusqu'à ce qu'il n'y en
/// ait plus (les parties créées par le découpage sont lues au tour suivant).
async fn lire_le_mail(b: &Banc, m: i64) {
    lecture::pompe(b.pool()).await.unwrap();
    for _ in 0..5 {
        let ids: Vec<i64> = sqlx::query_scalar(
            "select id from document where mail_id = $1 and etat = 'a_lire' order by id",
        )
        .bind(m)
        .fetch_all(b.pool())
        .await
        .unwrap();
        if ids.is_empty() {
            return;
        }
        for id in ids {
            lecture::lire(&b.ctx, id, false, false, false)
                .await
                .unwrap();
        }
    }
}

fn bl(numero: &str, page: &str) -> PageTest {
    PageTest::texte(&[
        "DURAND SAS - 12 rue des Lilas, Nancy",
        &format!("BL No {numero} du 12/11/2026"),
        "Client : Magasin Frouard",
        "A1 Vase en verre 1 x 50,00",
        page,
    ])
}

/// Numéro, pages et nom d'une partie.
type Partie<'a> = (Option<&'a str>, Option<i32>, Option<i32>, &'a str);

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    etat: String,
    numero: Option<String>,
    message: Option<String>,
    nom: String,
    page_debut: Option<i32>,
    page_fin: Option<i32>,
    piece_id: i64,
}

async fn docs(b: &Banc, m: i64) -> Vec<Doc> {
    sqlx::query_as(
        "select d.etat, d.numero, d.message, p.nom, p.page_debut, p.page_fin, p.id as piece_id
         from document d join piece_jointe p on p.id = d.piece_jointe_id
         where d.mail_id = $1 order by p.page_debut nulls first, p.id",
    )
    .bind(m)
    .fetch_all(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn trois_bl_dans_un_pdf_donnent_trois_documents() {
    let faux = FauxClaude::new();
    let (b, a) = banc(&faux).await;
    let pdf = fixtures::pdf_pages(vec![
        bl("BL-101", "Page 1/1"),
        bl("BL-102", "Page 1/2"),
        bl("BL-102", "Page 2/2"),
        bl("BL-103", "Page 1/1"),
    ]);
    let brut = fixtures::eml(
        "dec@test",
        "Vos BL",
        &[("bls.pdf", "application/pdf", &pdf)],
        false,
    );
    let m = mail(&b, a, &brut).await;
    lire_le_mail(&b, m).await;
    let d = docs(&b, m).await;
    assert_eq!(d.len(), 4, "{d:?}");
    // Le fichier d'origine : ignoré, il renvoie à ses parties.
    assert_eq!(d[0].etat, "ignore");
    assert!(
        d[0].message.as_deref().unwrap().contains("3 pièces"),
        "{:?}",
        d[0].message
    );
    let parties: Vec<Partie> = d[1..]
        .iter()
        .map(|d| {
            (
                d.numero.as_deref(),
                d.page_debut,
                d.page_fin,
                d.nom.as_str(),
            )
        })
        .collect();
    assert_eq!(
        parties,
        [
            (Some("BL-101"), Some(1), Some(1), "bls (p. 1).pdf"),
            (Some("BL-102"), Some(2), Some(3), "bls (p. 2-3).pdf"),
            (Some("BL-103"), Some(4), Some(4), "bls (p. 4).pdf"),
        ]
    );
    assert!(d[1..].iter().all(|d| d.etat == "lu"));
    // Un appel par partie, aucun pour le découpage (décidé par le code).
    assert_eq!(faux.appels(), 3);
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'decoupage_pdf'")
            .await,
        0
    );
    // Le contenu d'une partie (aperçu, dépôt) : ses pages seulement.
    let rang: i32 = sqlx::query_scalar("select rang from piece_jointe where id = $1")
        .bind(d[0].piece_id)
        .fetch_one(b.pool())
        .await
        .unwrap();
    let chemin: String = sqlx::query_scalar("select chemin_eml from mail where id = $1")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    let eml = std::fs::read(stockage::chemin_absolu(&b.ctx.config.fichiers, &chemin)).unwrap();
    let partie = decoupage::contenu_piece(&eml, rang, Some((2, 3))).unwrap();
    assert_eq!(gmflow_core::lecture::pdf::pages(&partie), 2);
    let texte = gmflow_core::lecture::pdf::texte(&partie).unwrap();
    assert!(
        texte.contains("BL-102") && !texte.contains("BL-101"),
        "{texte}"
    );
    let sha: Vec<u8> = sqlx::query_scalar("select sha256 from piece_jointe where id = $1")
        .bind(d[2].piece_id)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(sha, gmflow_core::eml::sha256(&partie), "contenu stable");
    // Le mail compte toujours une seule pièce jointe.
    assert_eq!(
        b.compter(&format!(
            "select count(*) from piece_jointe where mail_id = {m} and source_id is null"
        ))
        .await,
        1
    );

    // Relecture du fichier d'origine : mêmes parties, aucun doublon.
    sqlx::query("update document set etat = 'a_lire' where piece_jointe_id = $1")
        .bind(d[0].piece_id)
        .execute(b.pool())
        .await
        .unwrap();
    lire_le_mail(&b, m).await;
    assert_eq!(docs(&b, m).await.len(), 4);
    assert_eq!(faux.appels(), 3);
    b.fermer().await;
}

#[tokio::test]
async fn facture_de_quatre_pages_jamais_decoupee() {
    let faux = FauxClaude::new();
    let (b, a) = banc(&faux).await;
    let page = |k: u32| {
        PageTest::texte(&[
            "DURAND SAS - 12 rue des Lilas, Nancy",
            "FACTURE No F-2026-9 du 12/11/2026",
            "Client : Magasin Frouard",
            "BL No 4512 du 10/11 : 3 articles livres",
            &format!("Page {k}/4"),
        ])
    };
    let pdf = fixtures::pdf_pages(vec![page(1), page(2), page(3), page(4)]);
    let brut = fixtures::eml(
        "dec2@test",
        "Facture",
        &[("f.pdf", "application/pdf", &pdf)],
        false,
    );
    let m = mail(&b, a, &brut).await;
    lire_le_mail(&b, m).await;
    let d = docs(&b, m).await;
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].page_debut, None);
    // Décidé par le code : aucun appel de découpage.
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'decoupage_pdf'")
            .await,
        0
    );
    b.fermer().await;
}

#[tokio::test]
async fn doute_demande_a_claude_et_reponse_illisible_pas_de_decoupage() {
    // Page 2 : un BL cité, sans pagination : le code doute et demande à Claude, dont la réponse
    // (ici sans « pieces ») ne tient pas : le fichier est lu en entier.
    let faux = FauxClaude::new();
    let (b, a) = banc(&faux).await;
    let pdf = fixtures::pdf_pages(vec![
        PageTest::texte(&[
            "FACTURE No F-2026-10 du 12/11/2026",
            "Client : Magasin Frouard",
        ]),
        PageTest::texte(&[
            "BL No 4513 du 11/11 : 2 articles",
            "Total HT 50,00 TVA 10,00 TTC 60,00",
        ]),
    ]);
    let brut = fixtures::eml(
        "dec3@test",
        "Facture",
        &[("f.pdf", "application/pdf", &pdf)],
        false,
    );
    let m = mail(&b, a, &brut).await;
    lire_le_mail(&b, m).await;
    assert_eq!(docs(&b, m).await.len(), 1);
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'decoupage_pdf'")
            .await,
        1
    );
    b.fermer().await;
}
