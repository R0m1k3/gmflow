use gmflow_core::imap::{self, ErreurImap, Transport};
use gmflow_core::test_utils;

#[tokio::test]
async fn login_et_liste() {
    let dossier = test_utils::dossier_unique();
    test_utils::creer_dossier("a@test.local", &dossier).await;
    let mut s = test_utils::session("a@test.local").await;
    let noms = imap::dossiers(&mut s).await.unwrap();
    assert!(noms.iter().any(|n| n == "INBOX"), "{noms:?}");
    assert!(noms.contains(&dossier));
    s.logout().await.ok();
    test_utils::supprimer_dossier("a@test.local", &dossier).await;
}

#[tokio::test]
async fn mauvais_mot_de_passe() {
    let (h, p) = test_utils::imap_test();
    let r = imap::connecter(&h, p, "a@test.local", "faux", Transport::ClairTest).await;
    assert!(matches!(r, Err(ErreurImap::RefusAuth(_))), "{r:?}");
}
