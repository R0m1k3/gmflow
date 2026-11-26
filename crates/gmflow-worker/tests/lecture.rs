//! Lecture des pièces jointes de bout en bout, avec un faux Claude par test (script qui
//! journalise ses arguments et rend une réponse fixée).

mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, attendre, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A, SIRET_B};
use gmflow_core::{parametre, reglages};
use gmflow_worker::lecture;
use serde_json::{Value, json};

fn extraction(type_doc: &str, societe_id: Option<i64>, avec_texte: bool) -> Value {
    let mut v = json!({
        "type": type_doc, "societe_id": societe_id,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": "FR96552100554" },
        "numero": "F-1", "date": "2026-11-12", "numero_commande": null,
        "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0,
        "confiance": { "type": 0.95, "societe": 0.5, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0.9 }
    });
    if avec_texte {
        v["texte"] = json!("FACTURE F-1 transcrite");
    }
    v
}

fn reponse(sortie: &Value) -> String {
    json!({ "type": "result", "is_error": false, "result": "", "structured_output": sortie })
        .to_string()
}

const INVALIDE: &str =
    r#"{"type":"result","is_error":false,"result":"Je n'arrive pas à lire ce document."}"#;

/// Faux Claude : journalise les arguments et le dossier courant dans `journal`, puis rend
/// `haiku` ou `autre` selon le modèle demandé.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(haiku: &str, autre: &str) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude").unwrap();
        let journal = dossier.0.join("journal");
        let script = format!(
            "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; echo \"PWD=$PWD\"; ls; echo ===; }} >> '{j}'\n\
             modele=''; prec=''\nfor a in \"$@\"; do [ \"$prec\" = '--model' ] && modele=\"$a\"; prec=\"$a\"; done\n\
             if [ \"$modele\" = haiku ]; then cat <<'FIN'\n{haiku}\nFIN\nelse cat <<'FIN'\n{autre}\nFIN\nfi\n",
            j = journal.display()
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn bin(&self) -> String {
        self.dossier.0.join("claude").display().to_string()
    }

    fn appels(&self) -> Vec<String> {
        std::fs::read_to_string(self.dossier.0.join("journal"))
            .unwrap_or_default()
            .split("===\n")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

async fn banc(faux: &FauxClaude) -> Banc {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    b
}

async fn societe(b: &Banc, nom: &str, siret: &str) -> i64 {
    sqlx::query_scalar("insert into societe (raison_sociale, siret) values ($1, $2) returning id")
        .bind(nom)
        .bind(siret)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

async fn jeton(b: &Banc) {
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_lecture")
        .await
        .unwrap();
}

async fn quota(b: &Banc, n: i64) {
    let mut r = reglages::lire(b.pool()).await.unwrap();
    r.quota_lecture_jour = n;
    reglages::ecrire(b.pool(), &r).await.unwrap();
}

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    id: i64,
    etat: String,
    voie: Option<String>,
    message: Option<String>,
    r#type: Option<String>,
    societe_id: Option<i64>,
    numero: Option<String>,
    montant_ttc: Option<f64>,
    texte: Option<String>,
    modele: Option<String>,
    copie_de: Option<i64>,
}

async fn docs(b: &Banc, mail_id: i64) -> Vec<Doc> {
    sqlx::query_as(
        "select d.id, d.etat, d.voie, d.message, d.type, d.societe_id, d.numero,
                d.montant_ttc::float8 as montant_ttc, d.texte, d.modele, d.copie_de
         from document d join piece_jointe p on p.id = d.piece_jointe_id
         where d.mail_id = $1 order by p.rang",
    )
    .bind(mail_id)
    .fetch_all(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn facturx_et_logo_sans_appel_claude() {
    let faux = FauxClaude::new(INVALIDE, INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    let pdf = fixtures::pdf_facturx(
        &fixtures::xml_cii("FA-9", SIRET_A, false),
        &["Facture FA-9"],
    );
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "fx@test",
            "Facture FA-9",
            &[
                ("facture.pdf", "application/pdf", &pdf),
                ("image001.png", "image/png", &fixtures::png_logo()),
            ],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = docs(&b, m).await;
    assert_eq!(d[0].etat, "lu");
    assert_eq!(d[0].voie.as_deref(), Some("facturx"));
    assert_eq!(d[0].r#type.as_deref(), Some("facture"));
    assert_eq!(d[0].numero.as_deref(), Some("FA-9"));
    assert_eq!(d[0].societe_id, Some(a));
    assert_eq!(d[0].montant_ttc, Some(120.0));
    assert_eq!(d[1].etat, "ignore");
    assert!(faux.appels().is_empty(), "aucun appel attendu");
    assert_eq!(
        b.compter("select count(*) from journal where action = 'lecture'")
            .await,
        2
    );
    b.fermer().await;
}

#[tokio::test]
async fn pdf_texte_lu_par_claude_avec_le_texte_seul() {
    let faux = FauxClaude::new(&reponse(&extraction("facture", None, false)), INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    societe(&b, "Magasin Houdemont", SIRET_B).await;
    jeton(&b).await;
    let pdf = fixtures::pdf_texte(&[
        "FACTURE F-1 du 12 novembre 2026",
        "Client Magasin Frouard SIRET 123 456 789 00014",
        "Bougies parfumees 24 unites",
        "Total HT 50,00 TVA 10,00 TTC 60,00",
    ]);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "t@test",
            "Votre facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = &docs(&b, m).await[0];
    assert_eq!(
        (d.etat.as_str(), d.voie.as_deref()),
        ("lu", Some("pdf_texte"))
    );
    // Société trouvée par le code (SIRET dans le texte), même si Claude n'en donne pas.
    assert_eq!(d.societe_id, Some(a));
    assert_eq!(d.modele.as_deref(), Some("haiku"));
    assert!(d.texte.as_deref().unwrap().contains("FACTURE F-1"));
    let appels = faux.appels();
    assert_eq!(appels.len(), 1);
    let args = &appels[0];
    assert!(
        args.contains("FACTURE F-1"),
        "le texte doit être dans le prompt"
    );
    assert!(
        args.contains("--tools\n\n"),
        "aucun outil pour un texte : {args}"
    );
    assert!(args.contains("--effort\nlow\n"));
    assert!(args.contains("--json-schema\n"));
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'extraction_texte' and succes")
            .await,
        1
    );
    b.fermer().await;
}

#[tokio::test]
async fn photo_jpg_ignoree_sans_appel_a_claude() {
    let faux = FauxClaude::new(&reponse(&extraction("bl", None, true)), INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let photo = fixtures::jpeg(90_000);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "s@test",
            "BL",
            &[("IMG_2041.jpg", "image/jpeg", &photo)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = &docs(&b, m).await[0];
    // Les images ne sont plus lues : seuls les PDF, XML et tableurs le sont.
    assert_eq!(d.etat, "ignore");
    assert!(faux.appels().is_empty());
    b.fermer().await;
}

#[tokio::test]
async fn reponse_non_conforme_nouvel_essai_puis_erreur() {
    // haiku répond mal, sonnet (nouvel essai) répond bien.
    let faux = FauxClaude::new(INVALIDE, &reponse(&extraction("avoir", None, false)));
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let pdf = fixtures::pdf_texte(&["AVOIR A-12", "Montant 12,00 euros, remboursement partiel"]);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "n@test",
            "Avoir",
            &[("a.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    let d = &docs(&b, m).await[0];
    assert_eq!(
        (d.etat.as_str(), d.r#type.as_deref(), d.modele.as_deref()),
        ("lu", Some("avoir"), Some("sonnet"))
    );
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'nouvel_essai'")
            .await,
        1
    );

    // Les deux répondent mal : erreur, et pas de troisième appel.
    let faux2 = FauxClaude::new(INVALIDE, INVALIDE);
    w.arreter();
    let mut ctx = b.ctx.clone();
    let mut config = (*ctx.config).clone();
    config.claude_bin = faux2.bin();
    ctx.config = std::sync::Arc::new(config);
    let pdf2 = fixtures::pdf_texte(&["DEVIS D-4", "Proposition de prix pour 40 cartons"]);
    let m2 = mail(
        &b,
        a,
        &fixtures::eml(
            "n2@test",
            "Devis",
            &[("d.pdf", "application/pdf", &pdf2)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(ctx);
    lire_tout(&b, &[m2]).await;
    w.arreter();
    let d = &docs(&b, m2).await[0];
    assert_eq!(d.etat, "erreur");
    // Phrase lisible dans À traiter ; le détail technique part au journal.
    assert_eq!(
        d.message.as_deref(),
        Some("Claude n’a pas réussi à le lire, même au second essai.")
    );
    assert_eq!(faux2.appels().len(), 2);
    b.fermer().await;
}

#[tokio::test]
async fn facture_sans_ttc_relue_par_sonnet_avec_le_pdf() {
    // haiku oublie le TTC ; sonnet, qui reçoit aussi le PDF, le lit.
    let mut incomplete = extraction("facture", None, false);
    incomplete["montant_ttc"] = Value::Null;
    let faux = FauxClaude::new(
        &reponse(&incomplete),
        &reponse(&extraction("facture", None, false)),
    );
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let pdf = fixtures::pdf_texte(&[
        "FACTURE F-1 du 12 novembre 2026",
        "Client Magasin Frouard, livraison du 10 novembre",
        "Bougies parfumees 24 unites",
        "Total HT 50,00 TVA 10,00 Net a payer TTC 60,00",
    ]);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "t@test",
            "Facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = &docs(&b, m).await[0];
    assert_eq!(
        (d.etat.as_str(), d.modele.as_deref()),
        ("lu", Some("sonnet"))
    );
    assert_eq!(d.message, None);
    let appels = faux.appels();
    assert_eq!(appels.len(), 2);
    assert!(!appels[0].contains("document.pdf"), "{}", appels[0]);
    let second = &appels[1];
    assert!(second.contains("le montant TTC manque"), "{second}");
    assert!(second.contains("--tools\nRead\n"), "{second}");
    // Read limité au dossier jetable de la pièce, le reste refusé sans demander.
    assert!(
        second.contains("--allowedTools\nRead(./**)\n--permission-mode\ndontAsk\n"),
        "{second}"
    );
    assert!(
        second.contains(&format!(
            "--settings\n{}\n",
            gmflow_worker::lecture::REGLAGES_LECTURE
        )),
        "{second}"
    );
    assert!(!second.contains("--bare"), "{second}");
    assert!(
        second.contains("document.pdf"),
        "PDF joint au nouvel essai : {second}"
    );
    b.fermer().await;
}

#[tokio::test]
async fn deux_lectures_incompletes_la_meilleure_est_gardee_et_signalee() {
    let mut sans_ttc = extraction("facture", None, false);
    sans_ttc["montant_ttc"] = Value::Null;
    let mut sans_rien = sans_ttc.clone();
    sans_rien["numero"] = Value::Null;
    let faux = FauxClaude::new(&reponse(&sans_ttc), &reponse(&sans_rien));
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let pdf = fixtures::pdf_texte(&[
        "FACTURE F-1 du 12 novembre 2026",
        "Quelques articles livres",
    ]);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "t@test",
            "Facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = &docs(&b, m).await[0];
    // La première (une anomalie) l'emporte sur la seconde (deux).
    assert_eq!(
        (d.etat.as_str(), d.modele.as_deref()),
        ("lu", Some("haiku"))
    );
    assert_eq!(
        d.message.as_deref(),
        Some("Lecture à vérifier : le montant TTC manque.")
    );
    b.fermer().await;
}

#[tokio::test]
async fn quota_atteint_ou_jeton_absent_le_document_attend() {
    let faux = FauxClaude::new(&reponse(&extraction("facture", None, false)), INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    let pdf = fixtures::pdf_texte(&[
        "FACTURE F-77 du 10 novembre 2026",
        "Marchandises livrees le 10 novembre",
        "Total HT 50,00 TVA 10,00 TTC 60,00",
    ]);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "q@test",
            "Facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    // Pas de jeton.
    lire_tout(&b, &[m]).await;
    let d = &docs(&b, m).await[0];
    assert_eq!(
        (d.etat.as_str(), d.voie.as_deref()),
        ("a_lire", Some("pdf_texte"))
    );
    assert!(
        d.message
            .as_deref()
            .unwrap_or_default()
            .starts_with("Claude n’est pas connecté"),
        "{:?}",
        d.message
    );
    // Jeton présent mais quota à 0 : toujours en attente, sans appel.
    jeton(&b).await;
    quota(&b, 0).await;
    sqlx::query("update document set planifie_le = null")
        .execute(b.pool())
        .await
        .unwrap();
    assert_eq!(
        lecture::pompe(b.pool()).await.unwrap(),
        0,
        "la pompe ne renvoie pas un document qui attend Claude"
    );
    assert!(faux.appels().is_empty());
    // Quota rendu : la pompe le relance et il est lu.
    quota(&b, 100).await;
    sqlx::query("update document set message = null")
        .execute(b.pool())
        .await
        .unwrap();
    lire_tout(&b, &[m]).await;
    w.arreter();
    assert_eq!(docs(&b, m).await[0].etat, "lu");
    assert_eq!(faux.appels().len(), 1);
    b.fermer().await;
}

#[tokio::test]
async fn document_identique_jamais_relu_sauf_relecture_forcee() {
    let faux = FauxClaude::new(&reponse(&extraction("facture", None, false)), INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let pdf = fixtures::pdf_texte(&["FACTURE F-5", "Total 60,00 euros toutes taxes comprises"]);
    let m1 = mail(
        &b,
        a,
        &fixtures::eml(
            "d1@test",
            "Facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m1]).await;
    let m2 = mail(
        &b,
        a,
        &fixtures::eml(
            "d2@test",
            "Relance facture",
            &[("f.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    lire_tout(&b, &[m2]).await;
    let (d1, d2) = (&docs(&b, m1).await[0], &docs(&b, m2).await[0]);
    assert_eq!(d2.etat, "lu");
    assert_eq!(d2.copie_de, Some(d1.id));
    assert_eq!(faux.appels().len(), 1);
    // Relecture forcée : nouvel appel.
    sqlx::query("update document set etat = 'a_lire', planifie_le = now() where id = $1")
        .bind(d1.id)
        .execute(b.pool())
        .await
        .unwrap();
    gmflow_core::file::pousser_lecture(b.pool(), d1.id, true)
        .await
        .unwrap();
    let pool = b.pool().clone();
    let id = d1.id;
    assert!(
        attendre(Duration::from_secs(20), || {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, String>("select etat from document where id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    == "lu"
            }
        })
        .await
    );
    w.arreter();
    assert_eq!(faux.appels().len(), 2);
    b.fermer().await;
}

#[tokio::test]
async fn liste_de_diffusion_images_ignorees_et_lecture_auto_coupee() {
    let faux = FauxClaude::new(&reponse(&extraction("autre", None, true)), INVALIDE);
    let b = banc(&faux).await;
    let a = societe(&b, "Magasin Frouard", SIRET_A).await;
    jeton(&b).await;
    let photo = fixtures::jpeg(90_000);
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "l@test",
            "Nos promos",
            &[("promo.jpg", "image/jpeg", &photo)],
            true,
        ),
    )
    .await;
    let mut r = reglages::lire(b.pool()).await.unwrap();
    r.lecture_auto = false;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    assert_eq!(lecture::pompe(b.pool()).await.unwrap(), 0);
    assert_eq!(
        b.compter("select count(*) from document where etat = 'a_lire'")
            .await,
        1
    );
    r.lecture_auto = true;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let d = &docs(&b, m).await[0];
    assert_eq!(d.etat, "ignore");
    assert!(faux.appels().is_empty());
    b.fermer().await;
}
