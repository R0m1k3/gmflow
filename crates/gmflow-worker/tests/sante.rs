//! Port interne 8081 du worker : `/health` et `/metrics` (constat C25).

mod commun;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use commun::Banc;
use gmflow_worker::sante;
use tower::ServiceExt;

async fn lire(b: &Banc, chemin: &str) -> (StatusCode, String) {
    let rep = sante::routeur(b.pool().clone())
        .oneshot(Request::builder().uri(chemin).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let code = rep.status();
    let corps = axum::body::to_bytes(rep.into_body(), usize::MAX)
        .await
        .unwrap();
    (code, String::from_utf8(corps.to_vec()).unwrap())
}

#[tokio::test]
async fn metriques_prometheus_sur_le_port_interne() {
    let b = Banc::new().await;
    let soc = b.societe("Magasin").await;
    sqlx::query(
        "insert into boite_mail (societe_id, libelle, serveur, identifiant, mot_de_passe_chiffre, active, derniere_releve, cree_le)
         values ($1, 'Factures', 'imap.test', 'x', '\\x00', true, now() - interval '2 hours', now() - interval '1 day')",
    )
    .bind(soc)
    .execute(b.pool())
    .await
    .unwrap();
    let (code, _) = lire(&b, "/health").await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    let (code, t) = lire(&b, "/metrics").await;
    assert_eq!(code, StatusCode::OK);
    assert!(t.contains("gmflow_releves_en_retard 1"), "{t}");
    assert!(t.contains("gmflow_worker_battement_age_secondes -1"), "{t}");
    assert!(t.contains("gmflow_boite_etat{boite_id=\""), "{t}");
    b.fermer().await;
}
