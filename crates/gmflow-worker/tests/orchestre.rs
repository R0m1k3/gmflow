//! Orchestre : relance d'une file sans doubler une tâche, file relancée après une panne de la
//! base, tour de surveillance (déblocage, raison écrite, état), relances demandées par l'app,
//! santé qui exige le battement de chaque file.

mod commun;

use std::time::Duration;

use apalis::prelude::{BoxDynError, TaskId};
use commun::{Banc, WorkersTest, attendre, mail};
use gmflow_core::{file, fixtures, parametre, reglages, traitements};
use gmflow_worker::relance::{self, Reprise, Suivi};
use gmflow_worker::{lecture, orchestre, sante};
use serde_json::Value;

async fn executer(b: &Banc, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(b.pool())
        .await
        .unwrap();
}

async fn texte(b: &Banc, sql: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(b.pool())
        .await
        .unwrap()
}

async fn statut(b: &Banc, id: &str) -> String {
    texte(
        b,
        &format!("select status from apalis.jobs where id = '{id}'"),
    )
    .await
    .unwrap()
}

/// Un document `a_lire` venu d'un mail avec un PDF.
async fn document(b: &Banc, societe: i64, numero: &str) -> i64 {
    let brut = fixtures::eml(
        &format!("{numero}@test"),
        "Facture",
        &[(
            "f.pdf",
            "application/pdf",
            &fixtures::pdf_texte(&[&format!("FACTURE {numero}")]),
        )],
        false,
    );
    let m = mail(b, societe, &brut).await;
    lecture::pompe(b.pool()).await.unwrap();
    sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

/// Une tâche de lecture « prise » par `worker` dans l'état `status`. Rend son id apalis.
async fn tache_prise(b: &Banc, document_id: i64, worker: &str, status: &str) -> String {
    file::pousser_lecture(b.pool(), document_id, true)
        .await
        .unwrap();
    let id = texte(
        b,
        &format!(
            "select id from apalis.jobs where job_type = 'lecture'
             and convert_from(job, 'UTF8')::jsonb #>> '{{LireDocument,document_id}}' = '{document_id}'
             order by run_at desc limit 1"
        ),
    )
    .await
    .unwrap();
    executer(
        b,
        &format!(
            "update apalis.jobs set status = '{status}', lock_by = '{worker}', lock_at = now()
             where id = '{id}'"
        ),
    )
    .await;
    id
}

#[tokio::test]
async fn reprise_sans_doubler_les_taches() {
    let b = Banc::new().await;
    let s = b.societe("Frodis").await;
    executer(
        &b,
        "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
         values ('gmflow-lecture', 'lecture', 'PgStorage', '', now() - interval '1 minute'),
                ('gmflow-boites', 'boites', 'PgStorage', '', now())",
    )
    .await;
    let mut d = vec![];
    for n in 0..5 {
        d.push(document(&b, s, &format!("F{n}")).await);
    }
    // Finie pendant la panne (accusé perdu), en cours, échouée, jamais commencée, interrompue.
    let finie = tache_prise(&b, d[0], "gmflow-lecture", "Running").await;
    let en_cours = tache_prise(&b, d[1], "gmflow-lecture", "Running").await;
    let echouee = tache_prise(&b, d[2], "gmflow-lecture", "Running").await;
    let jamais = tache_prise(&b, d[3], "gmflow-lecture", "Queued").await;
    let interrompue = tache_prise(&b, d[4], "gmflow-lecture", "Running").await;

    let suivi = std::sync::Arc::new(Suivi::default());
    let ok = async { Ok::<(), BoxDynError>(()) };
    suivi
        .executer(file::FILE_LECTURE, &TaskId::from_string(finie.clone()), ok)
        .await
        .unwrap();
    let ko = async { Err::<(), BoxDynError>("illisible".into()) };
    assert!(
        suivi
            .executer(
                file::FILE_LECTURE,
                &TaskId::from_string(echouee.clone()),
                ko
            )
            .await
            .is_err()
    );
    // Une tâche abandonnée en route (future lâchée) est notée interrompue.
    {
        let id = TaskId::from_string(interrompue.clone());
        let lachee = suivi.executer(
            file::FILE_LECTURE,
            &id,
            std::future::pending::<Result<(), BoxDynError>>(),
        );
        let _ = tokio::time::timeout(Duration::from_millis(10), lachee).await;
    }
    // La tâche en cours finit 300 ms après le début de la reprise : la reprise l'attend.
    let s2 = suivi.clone();
    let id2 = TaskId::from_string(en_cours.clone());
    let (pret_tx, pret_rx) = tokio::sync::oneshot::channel();
    let longue = tokio::spawn(async move {
        s2.executer(file::FILE_LECTURE, &id2, async {
            let _ = pret_tx.send(());
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(())
        })
        .await
    });
    pret_rx.await.unwrap();
    assert_eq!(suivi.en_cours(file::FILE_LECTURE), 1);

    let debut = std::time::Instant::now();
    let r = relance::reprendre(b.pool(), &suivi, file::FILE_LECTURE, 1, Duration::ZERO)
        .await
        .unwrap();
    assert!(
        debut.elapsed() >= Duration::from_millis(200),
        "la reprise attend la fin de la tâche en cours"
    );
    longue.await.unwrap().unwrap();
    assert_eq!(
        r,
        Reprise {
            terminees: 3,
            remises: 2
        }
    );
    assert_eq!(statut(&b, &finie).await, "Done");
    assert_eq!(statut(&b, &en_cours).await, "Done");
    assert_eq!(statut(&b, &echouee).await, "Failed");
    assert_eq!(statut(&b, &jamais).await, "Pending");
    assert_eq!(statut(&b, &interrompue).await, "Pending");
    assert_eq!(
        b.compter(&format!(
            "select count(*) from apalis.jobs where id in ('{jamais}', '{interrompue}')
             and lock_by is null and lock_at is null"
        ))
        .await,
        2
    );
    // Jamais commencée : pas d'essai compté ; interrompue : un essai de plus.
    assert_eq!(
        b.compter(&format!(
            "select attempts::bigint from apalis.jobs where id = '{jamais}'"
        ))
        .await,
        0
    );
    assert_eq!(
        b.compter(&format!(
            "select attempts::bigint from apalis.jobs where id = '{interrompue}'"
        ))
        .await,
        1
    );
    assert_eq!(
        b.compter(
            "select count(*) from journal where action = 'file_relancee' and objet_id = 'lecture'"
        )
        .await,
        1
    );
    // Les autres files et la première incarnation ne sont pas touchées.
    let r0 = relance::reprendre(b.pool(), &suivi, file::FILE_LECTURE, 0, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(r0, Reprise::default());
    b.fermer().await;
}

#[tokio::test]
async fn file_relancee_apres_une_panne() {
    let b = Banc::new().await;
    let mut ctx = b.ctx.clone();
    ctx.relance_base = Duration::from_millis(100);
    let w = WorkersTest::demarrer(ctx);
    let pool = b.pool().clone();
    assert!(
        attendre(Duration::from_secs(60), || async {
            traitements::battements(&pool).await.unwrap().len() == 6
        })
        .await,
        "les six files s'inscrivent"
    );
    // Panne : la relève des tâches échoue sur toutes les files.
    executer(
        &b,
        "alter function apalis.get_jobs rename to get_jobs_coupee",
    )
    .await;
    // Une tâche poussée réveille la file, qui échoue en la cherchant.
    file::pousser_tri(b.pool(), 999_999).await.unwrap();
    assert!(
        attendre(Duration::from_secs(60), || async {
            b.compter(
                "select count(*) from journal where action = 'file_relancee' and objet_id = 'tri'",
            )
            .await
                > 0
        })
        .await,
        "la file de tri est relancée"
    );
    let erreur = texte(
        &b,
        "select details ->> 'erreur' from journal where action = 'file_relancee'
         and objet_id = 'tri' order by id limit 1",
    )
    .await
    .unwrap_or_default();
    assert!(erreur.contains("get_jobs"), "{erreur}");
    // Fin de la panne : la file relancée reprend son travail sous un nouveau nom.
    executer(
        &b,
        "alter function apalis.get_jobs_coupee rename to get_jobs",
    )
    .await;
    assert!(
        attendre(Duration::from_secs(60), || async {
            b.compter(
                "select count(*) from apalis.jobs where job_type = 'tri' and status = 'Done'
                 and lock_by like 'gmflow-tri-r%'",
            )
            .await
                == 1
        })
        .await,
        "la tâche passe après la relance"
    );
    w.arreter();
    b.fermer().await;
}

#[tokio::test]
async fn tour_debloque_et_ecrit_les_raisons() {
    let b = Banc::new().await;
    let s = b.societe("Frodis").await;
    // Lecture automatique, pas de jeton Claude : les documents par Claude attendent.
    reglages::ecrire(b.pool(), &reglages::Reglages::default())
        .await
        .unwrap();
    let vieux = document(&b, s, "V1").await;
    let recent = document(&b, s, "R1").await;
    let bloque = document(&b, s, "B1").await;
    executer(
        &b,
        &format!(
            "update document set cree_le = now() - interval '1 hour', voie = 'pdf_texte',
                planifie_le = now() - interval '1 hour', message = null where id = {vieux};
             update document set voie = 'pdf_texte', message = null where id = {recent};
             update document set etat = 'en_cours', pris_le = now() - interval '40 minutes'
              where id = {bloque};"
        ),
    )
    .await;
    let m = b.societe("Autre").await;
    let mail_bloque = mail(&b, m, &fixtures::eml("tri@test", "Question", &[], false)).await;
    executer(
        &b,
        &format!(
            "update mail set tri_etat = 'en_cours', tri_pris_le = now() - interval '40 minutes'
             where id = {mail_bloque}"
        ),
    )
    .await;

    let mut memoire = orchestre::Memoire::default();
    let etat = orchestre::tour(b.pool(), &mut memoire).await.unwrap();

    assert_eq!(
        texte(
            &b,
            &format!("select etat from document where id = {bloque}")
        )
        .await,
        Some("a_lire".into())
    );
    assert_eq!(
        texte(
            &b,
            &format!("select tri_etat from mail where id = {mail_bloque}")
        )
        .await,
        Some("a_trier".into())
    );
    assert_eq!(
        b.compter(
            "select count(*) from journal where action in ('lecture_debloquee', 'tri_debloque')"
        )
        .await,
        2
    );
    let message = texte(
        &b,
        &format!("select message from document where id = {vieux}"),
    )
    .await
    .unwrap_or_default();
    assert!(
        message.starts_with("Claude n’est pas connecté"),
        "{message}"
    );
    assert_eq!(
        texte(
            &b,
            &format!("select message from document where id = {recent}")
        )
        .await,
        None,
        "moins de 15 minutes : rien d'écrit"
    );
    assert_eq!(etat["lecture"]["raisons_notees"], 1);
    assert!(etat["claude"]["lecture"].as_str().is_some());
    assert_eq!(etat["files"]["lecture"]["vivante"], false);
    let lu = parametre::lire(b.pool(), traitements::ORCHESTRE_ETAT)
        .await
        .unwrap()
        .unwrap();
    assert!(lu["vu_le"].as_str().is_some());
    // Files jamais vues : un événement chacune au premier tour, puis plus rien.
    let arretees = b
        .compter("select count(*) from journal where action = 'file_arretee'")
        .await;
    assert_eq!(arretees, 6);
    orchestre::tour(b.pool(), &mut memoire).await.unwrap();
    assert_eq!(
        b.compter("select count(*) from journal where action = 'file_arretee'")
            .await,
        6
    );
    assert_eq!(
        b.compter("select count(*) from journal where action = 'lecture_debloquee'")
            .await,
        1
    );
    // Une file qui se remet à battre : un événement « repartie ».
    executer(
        &b,
        "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
         values ('gmflow-lecture', 'lecture', 'PgStorage', '', now())",
    )
    .await;
    let etat: Value = orchestre::tour(b.pool(), &mut memoire).await.unwrap();
    assert_eq!(etat["files"]["lecture"]["vivante"], true);
    assert_eq!(
        b.compter(
            "select count(*) from journal where action = 'file_repartie' and objet_id = 'lecture'"
        )
        .await,
        1
    );
    b.fermer().await;
}

#[tokio::test]
async fn relances_demandees_par_l_app() {
    let b = Banc::new().await;
    let s = b.societe("Frodis").await;
    let d = document(&b, s, "D1").await;
    let f = document(&b, s, "D2").await;
    // Lecture : le document planifié il y a peu repart tout de suite.
    executer(
        &b,
        &format!(
            "delete from apalis.jobs;
             update document set planifie_le = now() where id = {d};
             update document set etat = 'lu', type = 'facture', statut_depot = 'pret',
                valide_le = now() where id = {f};"
        ),
    )
    .await;
    traitements::demander(b.pool(), "lecture").await.unwrap();
    traitements::demander(b.pool(), "depot").await.unwrap();
    traitements::demander(b.pool(), "inconnue").await.unwrap();
    assert_eq!(traitements::demandes(b.pool()).await.unwrap().len(), 3);
    let faites = orchestre::executer_demandes(b.pool()).await.unwrap();
    assert_eq!(
        faites,
        vec![("lecture".to_string(), 1), ("depot".to_string(), 1)]
    );
    assert!(traitements::demandes(b.pool()).await.unwrap().is_empty());
    assert_eq!(file::documents_en_file(b.pool()).await.unwrap(), vec![d]);
    // Le dépôt déjà en file n'est pas poussé deux fois.
    assert!(
        traitements::documents_a_deposer(b.pool())
            .await
            .unwrap()
            .is_empty()
    );
    traitements::demander(b.pool(), "depot").await.unwrap();
    assert_eq!(
        orchestre::executer_demandes(b.pool()).await.unwrap(),
        vec![("depot".to_string(), 0)]
    );
    assert_eq!(
        b.compter("select count(*) from journal where action = 'traitement_relance'")
            .await,
        3
    );
    b.fermer().await;
}

#[tokio::test]
async fn sante_exige_le_battement_de_chaque_file() {
    let b = Banc::new().await;
    gmflow_worker::planif::tour(b.pool()).await.unwrap();
    assert!(!sante::controle(b.pool()).await, "aucune file inscrite");
    for f in traitements::FILES {
        executer(
            &b,
            &format!(
                "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
                 values ('gmflow-{f}', '{f}', 'PgStorage', '', now())"
            ),
        )
        .await;
    }
    assert!(sante::controle(b.pool()).await);
    // Une incarnation ancienne ne compte pas si la nouvelle bat.
    executer(
        &b,
        "insert into apalis.workers (id, worker_type, storage_name, layers, last_seen)
         values ('gmflow-envoi-r1', 'envoi', 'PgStorage', '', now() - interval '1 hour')",
    )
    .await;
    assert!(sante::controle(b.pool()).await);
    executer(
        &b,
        "update apalis.workers set last_seen = now() - interval '6 minutes' where worker_type = 'envoi'",
    )
    .await;
    assert!(!sante::controle(b.pool()).await);
    b.fermer().await;
}
