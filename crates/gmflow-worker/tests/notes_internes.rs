//! Notes internes (audit U23) : jamais envoyées à l'extérieur. Ni le mail de dépôt Dext/Yooz
//! d'une pièce annotée, ni la demande de brouillon à Claude, ni la réponse envoyée au
//! fournisseur ne contiennent ses notes.

mod commun;

use commun::{Banc, mail};
use gmflow_core::test_utils::SmtpTest;
use gmflow_core::{expedition, fixtures, parametre};
use gmflow_worker::{envoi, redaction, reponse};

const NOTE: &str = "NOTE-INTERNE-Durand-ne-pas-payer";

#[tokio::test]
async fn le_depot_n_emporte_pas_les_notes_internes() {
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
        "insert into societe (raison_sociale, outil_depot, adresse_depot)
         values ('Magasin Frouard', 'dext', 'depot@dext.test') returning id",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    let m = mail(
        &b,
        societe,
        &fixtures::eml(
            "note@test",
            "Facture FA-90",
            &[("FA-90.pdf", "application/pdf", b"%PDF-1.4 test")],
            false,
        ),
    )
    .await;
    let id: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, date_document, montant_ttc, statut_depot, valide_le,
             verifie_le, texte, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'facture', $2, 'Durand', 'FA-90',
                current_date, 60, 'pret', now(), now(), 'FACTURE', now()
         from piece_jointe where mail_id = $1 returning id",
    )
    .bind(m)
    .bind(societe)
    .fetch_one(b.pool())
    .await
    .unwrap();
    sqlx::query(
        "insert into note_interne (document_id, auteur_nom, texte) values ($1, 'resp-a', $2)",
    )
    .bind(id)
    .bind(NOTE)
    .execute(b.pool())
    .await
    .unwrap();

    envoi::deposer(&b.ctx, id, false, None).await.unwrap();
    let recus = smtp.recus();
    assert_eq!(recus.len(), 1, "un mail de dépôt");
    assert!(
        !recus[0].contains(NOTE) && !recus[0].contains("NOTE-INTERNE"),
        "la note est partie chez Dext"
    );
    assert_eq!(
        b.compter("select count(*) from envoi_depot where etat = 'envoye'")
            .await,
        1
    );
    // La note reste sur la pièce.
    assert_eq!(b.compter("select count(*) from note_interne").await, 1);
}

#[tokio::test]
async fn la_reponse_au_fournisseur_n_emporte_pas_les_notes_internes() {
    // Faux Claude qui garde ses arguments (le prompt) et rend un brouillon.
    let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-notes").unwrap();
    let journal = dossier.0.join("journal");
    let bin = dossier.0.join("claude");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n\
             echo '{{\"type\":\"result\",\"is_error\":false,\"result\":\"\",\"structured_output\":{{\"texte\":\"Bonjour, bien recu.\"}}}}'\n",
            journal.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = bin.display().to_string();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-jeton_notes")
        .await
        .unwrap();
    let smtp = SmtpTest::demarrer().await;
    let societe = b.societe("Magasin Frouard").await;
    let boite = b.boite_ok(societe, "a@test.local", &["INBOX"]).await;
    sqlx::query(
        "update boite_mail set smtp_serveur = '127.0.0.1', smtp_port = $2, smtp_securite = 'clair',
            dossier_envoyes = null where id = $1",
    )
    .bind(boite)
    .bind(i32::from(smtp.port))
    .execute(b.pool())
    .await
    .unwrap();
    let m = mail(
        &b,
        societe,
        &fixtures::eml(
            "durand@test",
            "Facture FA-91, une question",
            &[("FA-91.pdf", "application/pdf", b"%PDF-1.4 test")],
            false,
        ),
    )
    .await;
    sqlx::query("update mail_boite set boite_id = $2 where mail_id = $1")
        .bind(m)
        .bind(boite)
        .execute(b.pool())
        .await
        .unwrap();
    let doc: i64 = sqlx::query_scalar(
        "insert into document (piece_jointe_id, mail_id, sha256, etat, voie, type, societe_id,
             fournisseur_nom, numero, montant_ttc, texte, lu_le)
         select id, mail_id, sha256, 'lu', 'pdf_texte', 'facture', $2, 'Durand', 'FA-91', 60,
                'FACTURE', now()
         from piece_jointe where mail_id = $1 returning id",
    )
    .bind(m)
    .bind(societe)
    .fetch_one(b.pool())
    .await
    .unwrap();
    sqlx::query(
        "insert into note_interne (document_id, auteur_nom, texte) values ($1, 'resp-a', $2)",
    )
    .bind(doc)
    .bind(NOTE)
    .execute(b.pool())
    .await
    .unwrap();

    let br: i64 = sqlx::query_scalar(
        "insert into brouillon_reponse (mail_id, boite_id, statut) values ($1, $2, 'generation')
         returning id",
    )
    .bind(m)
    .bind(boite)
    .fetch_one(b.pool())
    .await
    .unwrap();
    redaction::generer(&b.ctx, br).await.unwrap();
    let prompt = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(prompt.contains("FA-91, une question"), "prompt : {prompt}");
    assert!(
        !prompt.contains("NOTE-INTERNE"),
        "la note est partie chez Claude"
    );
    let (statut, texte): (String, String) =
        sqlx::query_as("select statut, texte from brouillon_reponse where id = $1")
            .bind(br)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(statut, "propose", "{texte}");
    assert!(!texte.contains("NOTE-INTERNE"));

    // Clic « Envoyer » : la réponse part, sans la note.
    sqlx::query("update brouillon_reponse set statut = 'envoi' where id = $1")
        .bind(br)
        .execute(b.pool())
        .await
        .unwrap();
    reponse::envoyer(&b.ctx, br, None).await.unwrap();
    let recus = smtp.recus();
    assert_eq!(recus.len(), 1, "une réponse envoyée");
    assert!(
        !recus[0].contains("NOTE-INTERNE"),
        "la note est partie chez le fournisseur"
    );
    b.fermer().await;
}
