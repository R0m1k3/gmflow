mod commun;

use commun::{Banc, fixture};
use gmflow_core::reinitialisation;
use gmflow_core::test_utils::{self as tu};
use gmflow_worker::releve;

const A: &str = "a@test.local";

/// Après une réinitialisation, la relève repart des mails du jour (pas du dernier UID ni de la
/// période de reprise) et retélécharge ceux-là ; la pompe recrée les documents à lire.
#[tokio::test]
async fn la_releve_repart_des_mails_du_jour_apres_reinitialisation() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml", "html-seul.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    // Reçu il y a 3 jours : relevé la première fois (reprise de 3 mois), plus après la remise à zéro.
    tu::deposer_ancien(A, &d, &fixture("sans-message-id.eml"), 3).await;
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;

    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!((b.messages_lus, b.nouveaux_mails), (4, 4));
    assert_eq!(banc.compter("select count(*) from document").await, 1);
    let chemins_avant: Vec<String> =
        sqlx::query_scalar("select chemin_eml from mail order by chemin_eml")
            .fetch_all(banc.pool())
            .await
            .unwrap();

    let bilan = reinitialisation::reinitialiser(banc.pool(), None)
        .await
        .unwrap();
    assert_eq!((bilan.mails, bilan.documents), (4, 1));
    assert_eq!(bilan.boites_relancees, 1);
    assert_eq!(banc.compter("select count(*) from mail").await, 0);
    assert_eq!(banc.compter("select count(*) from boite_dossier").await, 0);
    // Les .eml restent sur disque.
    for c in &chemins_avant {
        assert!(banc.fichiers.0.join(c).exists(), "{c}");
    }

    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert!(b.bail_obtenu);
    assert_eq!((b.messages_lus, b.nouveaux_mails), (3, 3));
    assert_eq!(banc.compter("select count(*) from mail").await, 3);
    assert_eq!(banc.compter("select count(*) from mail_boite").await, 3);
    assert_eq!(banc.compter("select count(*) from piece_jointe").await, 1);
    assert_eq!(
        banc.compter("select count(*) from document where etat = 'a_lire'")
            .await,
        1
    );
    let chemins_apres: Vec<String> =
        sqlx::query_scalar("select chemin_eml from mail order by chemin_eml")
            .fetch_all(banc.pool())
            .await
            .unwrap();
    assert_eq!(chemins_apres.len(), 3);
    assert!(
        chemins_apres.iter().all(|c| chemins_avant.contains(c)),
        "mêmes fichiers, réutilisés"
    );
    let jour: Option<String> =
        sqlx::query_scalar("select reprise_depuis::text from boite_mail where id = $1")
            .bind(boite)
            .fetch_one(banc.pool())
            .await
            .unwrap();
    assert!(jour.is_some(), "jour de la réinitialisation noté");

    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}
