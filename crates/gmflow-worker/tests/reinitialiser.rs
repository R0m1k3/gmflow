mod commun;

use commun::{Banc, fixture};
use gmflow_core::reinitialisation;
use gmflow_core::test_utils::{self as tu};
use gmflow_worker::releve;

const A: &str = "a@test.local";

/// Après une réinitialisation, la relève repart de la période de reprise (pas du dernier UID)
/// et retélécharge tous les mails ; la pompe recrée les documents à lire.
#[tokio::test]
async fn la_releve_retelecharge_tout_apres_reinitialisation() {
    let banc = Banc::new().await;
    let d = tu::dossier_unique();
    tu::creer_dossier(A, &d).await;
    for f in ["simple.eml", "pdf.eml", "html-seul.eml"] {
        tu::deposer(A, &d, &fixture(f)).await;
    }
    let soc = banc.societe("Frouard").await;
    let boite = banc.boite_ok(soc, A, &[&d]).await;

    let b = releve::relever(&banc.ctx, boite).await.unwrap();
    assert_eq!((b.messages_lus, b.nouveaux_mails), (3, 3));
    assert_eq!(banc.compter("select count(*) from document").await, 1);
    let chemins_avant: Vec<String> =
        sqlx::query_scalar("select chemin_eml from mail order by chemin_eml")
            .fetch_all(banc.pool())
            .await
            .unwrap();

    let bilan = reinitialisation::reinitialiser(banc.pool(), None)
        .await
        .unwrap();
    assert_eq!((bilan.mails, bilan.documents), (3, 1));
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
    assert_eq!(chemins_avant, chemins_apres, "mêmes fichiers, réutilisés");

    tu::supprimer_dossier(A, &d).await;
    banc.fermer().await;
}
