//! Essais de boîte demandés par l'assistant d'ajout : IMAP sur le serveur de test (dossiers,
//! réception, envoyés), mot de passe refusé dit en clair, SMTP sans envoi ; le mot de passe
//! est effacé de la demande une fois l'essai fait.

mod commun;

use commun::Banc;
use gmflow_core::crypto::Chiffreur;
use gmflow_core::essai_boite::{self, Essai, ParamImap, ParamSmtp};
use gmflow_core::parametre;
use gmflow_core::test_utils::{self as tu, CLE_TEST, MDP_IMAP};

const A: &str = "a@test.local";

async fn demander(
    banc: &Banc,
    identifiant: &str,
    mdp: &str,
    imap: Option<ParamImap>,
    smtp: Option<ParamSmtp>,
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    let cle = essai_boite::cle(id);
    let essai = Essai {
        etat: "en_cours".into(),
        cree_le: String::new(),
        boite_id: None,
        identifiant: identifiant.into(),
        imap,
        smtp,
        resultat_imap: None,
        resultat_smtp: None,
        termine_le: None,
    };
    parametre::ecrire(banc.pool(), &cle, &serde_json::to_value(&essai).unwrap())
        .await
        .unwrap();
    parametre::ecrire_secret(
        banc.pool(),
        &cle,
        &Chiffreur::new(&CLE_TEST).chiffrer(mdp.as_bytes()),
    )
    .await
    .unwrap();
    id
}

async fn resultat(banc: &Banc, id: uuid::Uuid) -> Essai {
    gmflow_worker::test_boite::essayer(&banc.ctx, id)
        .await
        .unwrap();
    let cle = essai_boite::cle(id);
    assert!(
        parametre::lire_secret(banc.pool(), &cle)
            .await
            .unwrap()
            .is_none(),
        "mot de passe effacé"
    );
    serde_json::from_value(parametre::lire(banc.pool(), &cle).await.unwrap().unwrap()).unwrap()
}

fn imap() -> Option<ParamImap> {
    let (serveur, port) = tu::imap_test();
    Some(ParamImap { serveur, port })
}

#[tokio::test]
async fn essai_imap_et_smtp() {
    let banc = Banc::new().await;
    let envoyes = format!("{}Envoy&AOk-s", tu::dossier_unique());
    tu::creer_dossier(A, &envoyes).await;

    let id = demander(&banc, A, MDP_IMAP, imap(), None).await;
    let e = resultat(&banc, id).await;
    assert_eq!(e.etat, "termine");
    assert!(e.termine_le.is_some());
    let r = e.resultat_imap.unwrap();
    assert!(r.ok, "{}", r.message);
    assert_eq!(r.reception.as_deref(), Some("INBOX"));
    let d = r.dossiers.iter().find(|d| d.nom == envoyes).unwrap();
    assert!(d.affiche.ends_with("Envoyés"));
    assert!(r.envoyes.is_some(), "dossier des envoyés repéré");
    assert!(e.resultat_smtp.is_none());

    // Mot de passe refusé : dit en clair, détail brut gardé.
    let id = demander(&banc, A, "faux", imap(), None).await;
    let r = resultat(&banc, id).await.resultat_imap.unwrap();
    assert!(!r.ok);
    assert!(
        r.message.starts_with("Mot de passe refusé"),
        "{}",
        r.message
    );
    assert!(r.detail.is_some());
    assert!(r.dossiers.is_empty());

    // Serveur injoignable.
    let id = demander(
        &banc,
        A,
        MDP_IMAP,
        Some(ParamImap {
            serveur: "127.0.0.1".into(),
            port: 1,
        }),
        None,
    )
    .await;
    let r = resultat(&banc, id).await.resultat_imap.unwrap();
    assert!(!r.ok);
    assert!(r.message.contains("refuse la connexion"), "{}", r.message);

    // SMTP : connexion et authentification, rien d'envoyé.
    let smtp = tu::SmtpTest::demarrer().await;
    let id = demander(
        &banc,
        "compta@frouard.test",
        "mdp-smtp",
        None,
        Some(ParamSmtp {
            serveur: "127.0.0.1".into(),
            port: smtp.port,
            securite: "clair".into(),
        }),
    )
    .await;
    let e = resultat(&banc, id).await;
    assert!(e.resultat_imap.is_none());
    let r = e.resultat_smtp.unwrap();
    assert!(r.ok, "{:?}", r);
    assert_eq!(smtp.connexions(), vec!["compta@frouard.test".to_string()]);
    assert!(smtp.recus().is_empty());

    // Essai déjà fait ou inconnu : rien.
    gmflow_worker::test_boite::essayer(&banc.ctx, id)
        .await
        .unwrap();
    gmflow_worker::test_boite::essayer(&banc.ctx, uuid::Uuid::new_v4())
        .await
        .unwrap();
    tu::supprimer_dossier(A, &envoyes).await;
    banc.fermer().await;
}
