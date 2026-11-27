//! « Annuler » une validation (audit U1) : jamais d'envoi après une annulation acceptée, jamais
//! deux envois, refus quand le worker a déjà pris l'envoi ; la course entre l'annulation et le
//! worker n'a qu'un gagnant.

mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, mail};
use gmflow_core::envoi_prevu::{self, Issue};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::test_utils::SmtpTest;
use gmflow_core::{expedition, parametre};
use gmflow_worker::envoi;

async fn monde() -> (Banc, SmtpTest, i64) {
    let b = Banc::new().await;
    let smtp = SmtpTest::demarrer().await;
    parametre::ecrire(
        b.pool(),
        expedition::CLE,
        &serde_json::to_value(smtp.expedition()).unwrap(),
    )
    .await
    .unwrap();
    let societe: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret, outil_depot, adresse_depot, depot_auto)
         values ('Magasin Frouard', $1, 'dext', 'depot@dext.test', false) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    (b, smtp, societe)
}

/// Facture lue, prête (sans alerte), jamais validée.
async fn facture(b: &Banc, societe: i64, numero: &str) -> i64 {
    let pdf = fixtures::pdf_texte(&[&format!("Facture {numero}")]);
    let m = mail(
        b,
        societe,
        &fixtures::eml(
            &format!("{numero}@test"),
            &format!("Facture {numero}"),
            &[(&format!("{numero}.pdf"), "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, date_document, montant_ht, montant_tva, montant_ttc,
             confiance, texte, lu_le, statut_depot)
         select p.id, p.mail_id, p.sha256, 'lu', 'pdf_texte', 'facture', $2, 'Durand', $3,
                current_date, 50, 10, 60, '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now(), 'pret'
         from piece_jointe p where p.mail_id = $1 returning id",
    )
    .bind(m)
    .bind(societe)
    .bind(numero)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

/// Ce que fait « Valider » : état d'avant noté, validation, envoi prévu dans `delai_s`.
async fn valider(b: &Banc, id: i64, delai_s: i64) -> i64 {
    let mut tx = b.pool().begin().await.unwrap();
    let avant = envoi_prevu::photo_avant(&mut tx, id).await.unwrap();
    sqlx::query("update document set verifie_le = now(), valide_le = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let p = envoi_prevu::prevoir(
        &mut tx,
        id,
        uuid::Uuid::new_v4(),
        None,
        false,
        &avant,
        delai_s,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    p.id
}

async fn etat_doc(b: &Banc, id: i64) -> (Option<String>, bool, bool) {
    sqlx::query_as(
        "select statut_depot, valide_le is not null, verifie_le is not null from document where id = $1",
    )
    .bind(id)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn annulee_avant_le_delai_rien_ne_part() {
    let (b, smtp, societe) = monde().await;
    let d = facture(&b, societe, "FA-100").await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    let p = valider(&b, d, 2).await;
    envoi_prevu::pousser(b.pool(), p).await.unwrap();
    // L'onglet peut se fermer : la tâche est déjà en file et attend l'heure prévue.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        envoi_prevu::annuler(b.pool(), p, None).await.unwrap(),
        Issue::Annulee
    );
    // Exactement l'état d'avant : non validée, non vérifiée, prête comme avant.
    assert_eq!(etat_doc(&b, d).await, (Some("pret".into()), false, false));
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(smtp.recus().is_empty(), "rien ne part après l'annulation");
    assert_eq!(
        b.compter("select count(*) from envoi_depot").await,
        0,
        "aucun envoi noté"
    );
    assert_eq!(
        b.compter("select count(*) from journal where action = 'validation_annulee'")
            .await,
        1
    );
    // Une seconde annulation n'a plus rien à faire ; la tâche rejouée ne part pas non plus.
    assert_eq!(
        envoi_prevu::annuler(b.pool(), p, None).await.unwrap(),
        Issue::RienAAnnuler
    );
    envoi::deposer_prevu(&b.ctx, p, d, false, None)
        .await
        .unwrap();
    // Société passée en envoi automatique : une validation annulée ne part pas toute seule.
    sqlx::query("update societe set depot_auto = true where id = $1")
        .bind(societe)
        .execute(b.pool())
        .await
        .unwrap();
    envoi::deposer(&b.ctx, d, false, None).await.unwrap();
    assert!(smtp.recus().is_empty());
    w.arreter();
    b.fermer().await;
}

#[tokio::test]
async fn part_apres_le_delai_puis_annulation_refusee() {
    let (b, smtp, societe) = monde().await;
    let d = facture(&b, societe, "FA-101").await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    let debut = std::time::Instant::now();
    let p = valider(&b, d, 1).await;
    envoi_prevu::pousser(b.pool(), p).await.unwrap();
    // Un dépôt poussé par ailleurs (relance, dépôt automatique) ne passe pas devant le délai.
    gmflow_core::file::pousser_envoi(
        b.pool(),
        gmflow_core::file::TacheEnvoi::Deposer {
            document_id: d,
            forcer: false,
            auteur_id: None,
        },
    )
    .await
    .unwrap();
    let pool = b.pool().clone();
    let parti = commun::attendre(Duration::from_secs(20), || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "select statut_depot from document where id = $1",
            )
            .bind(d)
            .fetch_one(&pool)
            .await
            .unwrap()
            .as_deref()
                == Some("depose")
        }
    })
    .await;
    assert!(parti, "envoi fait après le délai");
    assert!(
        debut.elapsed() >= Duration::from_millis(900),
        "pas avant le délai"
    );
    assert_eq!(smtp.recus().len(), 1);
    assert_eq!(
        envoi_prevu::annuler(b.pool(), p, None).await.unwrap(),
        Issue::DejaPartie
    );
    assert_eq!(etat_doc(&b, d).await.0.as_deref(), Some("depose"));
    // Tâche rejouée : jamais deux envois.
    envoi::deposer_prevu(&b.ctx, p, d, false, None)
        .await
        .unwrap();
    envoi::deposer(&b.ctx, d, false, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(smtp.recus().len(), 1);
    w.arreter();
    b.fermer().await;
}

#[tokio::test]
async fn course_annulation_contre_worker_un_seul_gagnant() {
    let (b, smtp, societe) = monde().await;
    let mut ids = Vec::new();
    for i in 0..12 {
        ids.push(facture(&b, societe, &format!("FA-2{i:02}")).await);
    }
    let mut parties = 0;
    for (i, d) in ids.iter().copied().enumerate() {
        let p = valider(&b, d, 0).await;
        let (ctx, pool) = (b.ctx.clone(), b.pool().clone());
        // Alterner qui démarre en premier : les deux ordres sont essayés.
        let depot = async move { envoi::deposer_prevu(&ctx, p, d, false, None).await };
        let annul = async move { envoi_prevu::annuler(&pool, p, None).await };
        let (rd, ra) = if i % 2 == 0 {
            tokio::join!(tokio::spawn(depot), tokio::spawn(annul))
        } else {
            let (y, x) = tokio::join!(tokio::spawn(annul), tokio::spawn(depot));
            (x, y)
        };
        rd.unwrap().unwrap();
        let issue = ra.unwrap().unwrap();
        let (statut, valide, _) = etat_doc(&b, d).await;
        match issue {
            Issue::Annulee => {
                assert_eq!(statut.as_deref(), Some("pret"), "pièce {d}");
                assert!(!valide);
            }
            Issue::DejaPartie => {
                parties += 1;
                assert_eq!(statut.as_deref(), Some("depose"), "pièce {d}");
            }
            Issue::RienAAnnuler => panic!("pièce {d} : ni annulée ni partie"),
        }
        let envois: i64 =
            sqlx::query_scalar("select count(*) from envoi_depot where document_id = $1")
                .bind(d)
                .fetch_one(b.pool())
                .await
                .unwrap();
        assert_eq!(envois, i64::from(issue == Issue::DejaPartie), "pièce {d}");
    }
    assert_eq!(smtp.recus().len(), parties);
    b.fermer().await;
}

/// Partie d'un PDF découpé (audit L19) validée avec délai : l'envoi pris par le worker joint
/// seulement ses pages, extraites du fichier d'origine ; une seconde tâche n'envoie rien de plus.
#[tokio::test]
async fn partie_d_un_pdf_decoupe_envoie_ses_seules_pages() {
    let (b, smtp, societe) = monde().await;
    let pdf = fixtures::pdf_pages(vec![
        fixtures::PageTest::texte(&["Facture FP-1", "page 1"]),
        fixtures::PageTest::texte(&["Facture FP-2", "page 2"]),
        fixtures::PageTest::texte(&["Facture FP-2", "page 3"]),
    ]);
    let m = mail(
        &b,
        societe,
        &fixtures::eml(
            "decoupe@test",
            "Factures",
            &[("lot.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    // Origine ignorée, partie p. 2-3 lue et prête (comme après `decoupage::decouper`).
    let d: i64 = sqlx::query_scalar(
        "with o as (select id, mail_id, rang, sha256 from piece_jointe where mail_id = $1),
              p as (insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256,
                                              source_id, page_debut, page_fin)
                    select mail_id, -(rang * 100 + 2), 'lot (p. 2-3).pdf', 'application/pdf', 1,
                           decode(md5('partie'), 'hex'), id, 2, 3 from o
                    returning id, mail_id, sha256)
         insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, date_document, montant_ht, montant_tva, montant_ttc,
             confiance, texte, lu_le, statut_depot)
         select p.id, p.mail_id, p.sha256, 'lu', 'pdf_texte', 'facture', $2, 'Durand', 'FP-2',
                current_date, 50, 10, 60, '{\"type\": 0.95, \"societe\": 0.95}', 'FACTURE', now(), 'pret'
         from p returning id",
    )
    .bind(m)
    .bind(societe)
    .fetch_one(b.pool())
    .await
    .unwrap();
    let p = valider(&b, d, 0).await;
    envoi::deposer_prevu(&b.ctx, p, d, false, None)
        .await
        .unwrap();
    assert_eq!(etat_doc(&b, d).await.0.as_deref(), Some("depose"));
    let recus = smtp.recus();
    assert_eq!(recus.len(), 1);
    let joint = (0..4)
        .find_map(|r| gmflow_core::eml::extraire_piece(recus[0].as_bytes(), r).ok())
        .expect("pièce jointe envoyée");
    assert_eq!(
        gmflow_core::lecture::pdf::pages(&joint),
        2,
        "pages 2 et 3 seulement"
    );
    assert!(recus[0].contains("lot (p. 2-3).pdf"));
    // Rejouée : rien de plus.
    envoi::deposer_prevu(&b.ctx, p, d, false, None)
        .await
        .unwrap();
    assert_eq!(smtp.recus().len(), 1);
    b.fermer().await;
}
