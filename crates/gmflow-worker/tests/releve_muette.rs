//! Serveur IMAP qui accepte la connexion puis ne répond plus : la relève échoue après le délai
//! (erreur notée, bail rendu) au lieu de garder la tâche pour toujours. Binaire à part : le délai
//! raccourci (`GMFLOW_TEST_DELAI_IMAP`) ne touche pas les autres tests de relève.
mod commun;

use std::time::{Duration, Instant};

use commun::Banc;
use gmflow_core::crypto::Chiffreur;
use gmflow_core::test_utils::CLE_TEST;
use gmflow_worker::releve;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Accueille, accepte LOGIN, puis se tait (la connexion reste ouverte).
async fn serveur_muet() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((flux, _)) = l.accept().await {
            tokio::spawn(async move {
                let (lecture, mut ecriture) = flux.into_split();
                ecriture.write_all(b"* OK pret\r\n").await.unwrap();
                let mut lignes = BufReader::new(lecture).lines();
                while let Ok(Some(l)) = lignes.next_line().await {
                    let mut mots = l.split_whitespace();
                    let (Some(tag), Some(cmd)) = (mots.next(), mots.next()) else {
                        continue;
                    };
                    if cmd.eq_ignore_ascii_case("LOGIN") {
                        let _ = ecriture
                            .write_all(format!("{tag} OK LOGIN fait\r\n").as_bytes())
                            .await;
                    }
                    // EXAMINE, LOGOUT… : aucune réponse.
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn serveur_muet_la_releve_echoue_au_lieu_de_bloquer() {
    // SAFETY : seul test de ce binaire, posé avant tout appel IMAP.
    unsafe { std::env::set_var("GMFLOW_TEST_DELAI_IMAP", "2") };
    let b = Banc::new().await;
    let soc = b.societe("Frouard").await;
    let port = serveur_muet().await;
    let boite: i64 = sqlx::query_scalar(
        "insert into boite_mail (societe_id, libelle, serveur, port, identifiant, mot_de_passe_chiffre, dossiers, active)
         values ($1, 'Muette', '127.0.0.1', $2, 'a@test.local', $3, '{INBOX,Autre}', true) returning id",
    )
    .bind(soc)
    .bind(i32::from(port))
    .bind(Chiffreur::new(&CLE_TEST).chiffrer(b"mdp"))
    .fetch_one(b.pool())
    .await
    .unwrap();

    let debut = Instant::now();
    let r = tokio::time::timeout(Duration::from_secs(30), releve::relever(&b.ctx, boite))
        .await
        .expect("la relève doit rendre la main");
    let e = r.unwrap_err().to_string();
    assert!(e.contains("ne répond plus (EXAMINE"), "{e}");
    // Session perdue : ni le second dossier ni le LOGOUT ne sont tentés (un seul délai).
    assert!(
        debut.elapsed() < Duration::from_secs(20),
        "{:?}",
        debut.elapsed()
    );
    let (erreur, bail): (Option<String>, Option<time::OffsetDateTime>) =
        sqlx::query_as("select derniere_erreur, releve_jusqu_a from boite_mail where id = $1")
            .bind(boite)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert!(erreur.unwrap().contains("ne répond plus"));
    assert_eq!(bail, None, "bail rendu");
    b.fermer().await;
}
