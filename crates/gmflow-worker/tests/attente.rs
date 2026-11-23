//! Documents en attente : quota compté sur les appels réussis, bascule unique du quota 100 →
//! 1000, pompe sans double poussée et mails récents d'abord, refus de Claude noté puis levé.

mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, attendre, lire_tout, mail};
use gmflow_core::file::{self, PlaceEnFile};
use gmflow_core::{fixtures, parametre, reglages};
use gmflow_worker::lecture;
use serde_json::{Value, json};

async fn jeton(b: &Banc) {
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_attente")
        .await
        .unwrap();
}

fn pdf(numero: &str) -> Vec<u8> {
    fixtures::pdf_texte(&[
        &format!("FACTURE {numero} du 10 novembre 2026"),
        "Marchandises livrees le 10 novembre",
        "Total HT 50,00 TVA 10,00 TTC 60,00",
    ])
}

async fn mail_pdf(b: &Banc, societe: i64, numero: &str) -> i64 {
    mail(
        b,
        societe,
        &fixtures::eml(
            &format!("{numero}@test"),
            "Facture",
            &[("f.pdf", "application/pdf", &pdf(numero))],
            false,
        ),
    )
    .await
}

async fn document_de(b: &Banc, mail_id: i64) -> i64 {
    sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(mail_id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

async fn executer(b: &Banc, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(b.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn quota_compte_seulement_les_appels_reussis() {
    let b = Banc::new().await;
    executer(
        &b,
        "insert into appel_claude (tache, modele, duree_ms, succes, quand) values
           ('extraction_texte', 'haiku', 1, true, now()),
           ('nouvel_essai', 'sonnet', 1, true, now()),
           ('extraction_texte', 'haiku', 1, false, now()),
           ('ocr_scan', 'haiku', 1, false, now()),
           ('extraction_texte', 'haiku', 1, true, now() - interval '2 days'),
           ('tri_mail', 'haiku', 1, true, now()),
           ('tri_mail', 'haiku', 1, false, now()),
           ('brouillon_reponse', 'sonnet', 1, true, now())",
    )
    .await;
    assert_eq!(reglages::appels_lecture_du_jour(b.pool()).await.unwrap(), 2);
    assert_eq!(reglages::appels_tri_du_jour(b.pool()).await.unwrap(), 1);
    b.fermer().await;
}

#[tokio::test]
async fn quota_100_passe_a_1000_une_seule_fois() {
    // Défaut des nouvelles installations.
    assert_eq!(reglages::Reglages::default().quota_lecture_jour, 1000);
    let b = Banc::new().await;
    let mut r = reglages::Reglages {
        quota_lecture_jour: 100,
        lecture_auto: false,
        ..Default::default()
    };
    reglages::ecrire(b.pool(), &r).await.unwrap();
    assert!(reglages::passer_quota_a_1000(b.pool()).await.unwrap());
    let lu = reglages::lire(b.pool()).await.unwrap();
    assert_eq!(lu.quota_lecture_jour, 1000);
    assert!(!lu.lecture_auto, "les autres réglages sont gardés");
    // Michael remet 100 ensuite : son choix reste, même après un redémarrage.
    r.quota_lecture_jour = 100;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    assert!(!reglages::passer_quota_a_1000(b.pool()).await.unwrap());
    assert_eq!(
        reglages::lire(b.pool()).await.unwrap().quota_lecture_jour,
        100
    );
    b.fermer().await;

    // Un quota choisi autrement n'est jamais touché ; rien d'enregistré : le défaut vaut.
    let b = Banc::new().await;
    assert!(!reglages::passer_quota_a_1000(b.pool()).await.unwrap());
    assert_eq!(
        parametre::lire(b.pool(), reglages::DRAPEAU_QUOTA_1000)
            .await
            .unwrap(),
        Some(json!(true))
    );
    let r = reglages::Reglages {
        quota_lecture_jour: 250,
        ..Default::default()
    };
    reglages::ecrire(b.pool(), &r).await.unwrap();
    sqlx::query("delete from parametre where cle = $1")
        .bind(reglages::DRAPEAU_QUOTA_1000)
        .execute(b.pool())
        .await
        .unwrap();
    assert!(!reglages::passer_quota_a_1000(b.pool()).await.unwrap());
    assert_eq!(
        reglages::lire(b.pool()).await.unwrap().quota_lecture_jour,
        250
    );
    b.fermer().await;
}

#[tokio::test]
async fn pompe_sans_double_poussee_et_mails_recents_d_abord() {
    let b = Banc::new().await;
    jeton(&b).await;
    let s = b.societe("Magasin Frouard").await;
    let vieux = mail_pdf(&b, s, "F-1").await;
    let recent = mail_pdf(&b, s, "F-2").await;
    sqlx::query("update mail set date_mail = now() - interval '1 hour' where id = $1")
        .bind(recent)
        .execute(b.pool())
        .await
        .unwrap();
    assert_eq!(lecture::pompe(b.pool()).await.unwrap(), 2);
    let (d_vieux, d_recent) = (document_de(&b, vieux).await, document_de(&b, recent).await);
    let priorites: Vec<(i64, i32)> = sqlx::query_as(
        "select (convert_from(job, 'UTF8')::jsonb #>> '{LireDocument,document_id}')::bigint, priority
         from apalis.jobs where job_type = 'lecture' order by 1",
    )
    .fetch_all(b.pool())
    .await
    .unwrap();
    assert_eq!(priorites, vec![(d_vieux, 0), (d_recent, 5)]);
    assert_eq!(
        file::place_lecture(b.pool(), d_recent).await.unwrap(),
        Some(PlaceEnFile::EnAttente {
            devant: 0,
            humain: false
        })
    );
    assert_eq!(
        file::place_lecture(b.pool(), d_vieux).await.unwrap(),
        Some(PlaceEnFile::EnAttente {
            devant: 1,
            humain: false
        })
    );
    // apalis sert bien le plus prioritaire d'abord.
    executer(
        &b,
        "insert into apalis.workers (id, worker_type, storage_name) values ('essai', 'lecture', 'PgStorage')",
    )
    .await;
    let servi: i64 = sqlx::query_scalar(
        "select (convert_from(job, 'UTF8')::jsonb #>> '{LireDocument,document_id}')::bigint
         from apalis.get_jobs('essai', 'lecture', 1)",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    assert_eq!(servi, d_recent);
    assert_eq!(
        file::place_lecture(b.pool(), d_recent).await.unwrap(),
        Some(PlaceEnFile::Prise)
    );

    // Déjà en file (en attente ou prise) : pas de seconde tâche.
    executer(&b, "update document set planifie_le = null").await;
    assert_eq!(lecture::pompe(b.pool()).await.unwrap(), 0);
    assert_eq!(
        b.compter("select count(*) from apalis.jobs where job_type = 'lecture'")
            .await,
        2
    );
    // Tâches finies, ou fantômes (prises il y a plus de 30 min) : la pompe repousse.
    executer(
        &b,
        "update apalis.jobs set status = 'Done' where status = 'Pending';
         update apalis.jobs set status = 'Running', lock_at = now() - interval '1 hour' where status = 'Queued';
         update document set planifie_le = null",
    )
    .await;
    assert_eq!(lecture::pompe(b.pool()).await.unwrap(), 2);
    b.fermer().await;
}

/// Faux Claude qui refuse (limite d'usage) tant que le fichier `refus` existe, puis lit.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new() -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-refus").unwrap();
        let ok = json!({
            "type": "result", "is_error": false, "result": "",
            "structured_output": {
                "type": "facture", "societe_id": null,
                "fournisseur": { "nom": "Durand", "siret": null, "tva": null },
                "numero": "F-9", "date": "2026-11-10", "numero_commande": null,
                "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0,
                "confiance": { "type": 1, "societe": 1, "fournisseur": 1, "numero": 1, "date": 1, "montants": 1 }
            }
        });
        let script = format!(
            "#!/bin/sh\nif [ -f '{d}/refus' ]; then\n\
             echo '{{\"type\":\"result\",\"is_error\":true,\"result\":\"You have hit your limit · resets 3pm (Europe/Paris)\"}}'\nexit 1\nfi\n\
             cat <<'FIN'\n{ok}\nFIN\n",
            d = dossier.0.display()
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dossier.0.join("refus"), "").unwrap();
        Self { dossier }
    }

    fn lever(&self) {
        std::fs::remove_file(self.dossier.0.join("refus")).unwrap();
    }
}

async fn etat_claude(b: &Banc) -> Value {
    parametre::lire(b.pool(), parametre::CLAUDE_ETAT)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn refus_de_claude_note_puis_leve_au_premier_succes() {
    let faux = FauxClaude::new();
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.dossier.0.join("claude").display().to_string();
    b.ctx.config = std::sync::Arc::new(config);
    jeton(&b).await;
    let s = b.societe("Magasin Frouard").await;
    let m = mail_pdf(&b, s, "F-9").await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    let d = document_de(&b, m).await;
    let (etat, message): (String, Option<String>) =
        sqlx::query_as("select etat, message from document where id = $1")
            .bind(d)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(etat, "a_lire");
    let message = message.unwrap();
    assert!(
        message.starts_with("Claude refuse : limite d’usage, reprise ")
            && message.ends_with("15:00"),
        "{message}"
    );
    let e = etat_claude(&b).await;
    assert_eq!(e["etat"], "erreur");
    assert_eq!(e["refus"], "limite");
    assert_eq!(e["explication"], message.as_str());
    let reprise = time::OffsetDateTime::parse(
        e["reprise"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    assert!(reprise > time::OffsetDateTime::now_utc());
    assert_eq!(reglages::appels_lecture_du_jour(b.pool()).await.unwrap(), 0);
    // Tant que la reprise n'est pas passée, la pompe ne relance pas les lectures par Claude.
    executer(&b, "update document set planifie_le = null").await;
    assert_eq!(lecture::pompe(b.pool()).await.unwrap(), 0);

    // Heure de reprise passée, Claude répond : le document est lu, l'état repasse à valide.
    faux.lever();
    executer(
        &b,
        "update parametre set valeur = jsonb_set(valeur, '{reprise}', '\"2000-01-01T00:00:00Z\"')
         where cle = 'claude_etat';
         update document set planifie_le = null, message = null",
    )
    .await;
    lire_tout(&b, &[m]).await;
    let pool = b.pool().clone();
    assert!(
        attendre(Duration::from_secs(10), || {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, String>("select etat from document where id = $1")
                    .bind(d)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    == "lu"
            }
        })
        .await
    );
    w.arreter();
    let e = etat_claude(&b).await;
    assert_eq!(e["etat"], "valide", "{e}");
    assert!(
        e.get("refus").is_none() && e.get("reprise").is_none(),
        "{e}"
    );
    assert!(reglages::appels_lecture_du_jour(b.pool()).await.unwrap() >= 1);
    b.fermer().await;
}
