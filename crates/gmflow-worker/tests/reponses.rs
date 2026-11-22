//! Lot 5 de bout en bout : tri des mails (règles puis Claude), brouillon à la demande, envoi
//! depuis la boîte d'origine avec copie dans les envoyés, purge des publicités.

mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, attendre, mail};
use gmflow_core::crypto::Chiffreur;
use gmflow_core::file::{self, TacheEnvoi};
use gmflow_core::test_utils::{self, CLE_TEST, SmtpTest};
use gmflow_core::{parametre, reglages};
use gmflow_worker::{purge, reponse, tri};
use serde_json::json;

const TRI_QUESTION: &str = r#"{"type":"result","is_error":false,"result":"","structured_output":{"categorie":"question","resume":"Demande la date de livraison de la commande 4471","priorite":"haute"}}"#;
const BROUILLON: &str = r#"{"type":"result","is_error":false,"result":"","structured_output":{"texte":"Bonjour Jean,\n\nLa commande 4471 part jeudi.\n\nBien cordialement,\nMS Frouard"}}"#;

/// Faux Claude : journalise ses arguments et les variables d'accès reçues, puis rend un tri
/// (schéma avec `categorie`) ou un brouillon (schéma avec `texte`).
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new() -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-l5").unwrap();
        let journal = dossier.0.join("journal");
        let script = format!(
            "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; \
             echo \"CLE=${{ANTHROPIC_API_KEY:-}}\"; echo \"JETON=${{CLAUDE_CODE_OAUTH_TOKEN:-}}\"; echo ===; }} >> '{j}'\n\
             case \"$*\" in *'\"categorie\"'*) cat <<'FIN'\n{TRI_QUESTION}\nFIN\n;; *) cat <<'FIN'\n{BROUILLON}\nFIN\n;; esac\n",
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
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-jeton_l5")
        .await
        .unwrap();
    b
}

/// Mail sans date (donc reçu maintenant, dans la fenêtre de tri).
fn eml(id: &str, de: &str, objet: &str, corps: &str, entetes: &str) -> Vec<u8> {
    format!(
        "From: {de}\r\nTo: factures@frouard.test\r\nSubject: {objet}\r\nMessage-ID: <{id}>\r\n\
         References: <debut-{id}>\r\n{entetes}MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{corps}\r\n"
    )
    .into_bytes()
}

const QUESTION: &str = "Bonjour,\r\n\r\nPourriez-vous me dire quand la commande 4471 sera livree ?\r\n\r\nCordialement,\r\nJean";

async fn trier_tout(b: &Banc) {
    tri::pompe(b.pool()).await.unwrap();
    let pool = b.pool().clone();
    let fini = attendre(Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "select count(*) from mail where tri_etat in ('a_trier', 'en_cours')",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
                == 0
        }
    })
    .await;
    assert!(fini, "tris non terminés");
}

type Classement = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

async fn classement(b: &Banc, id: i64) -> Classement {
    sqlx::query_as(
        "select categorie, categorie_par, statut_reponse, resume, tri_etat from mail where id = $1",
    )
    .bind(id)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn tri_par_regles_puis_par_claude() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let s = b.societe("MS Frouard").await;
    let pub_ = mail(
        &b,
        s,
        &eml(
            "p@x.test",
            "Promo <promo@x.test>",
            "Soldes",
            "Tout a -50 ?",
            "List-Unsubscribe: <mailto:u@x.test>\r\n",
        ),
    )
    .await;
    let notif = mail(
        &b,
        s,
        &eml(
            "n@x.test",
            "no-reply@transporteur.test",
            "Colis",
            QUESTION,
            "",
        ),
    )
    .await;
    let question = mail(
        &b,
        s,
        &eml(
            "q@x.test",
            "Jean <jean@client.test>",
            "Livraison",
            QUESTION,
            "",
        ),
    )
    .await;
    // Catégorie posée à la main : jamais retriée.
    let humain = mail(
        &b,
        s,
        &eml("h@x.test", "Jean <jean@client.test>", "Autre", QUESTION, ""),
    )
    .await;
    sqlx::query(
        "update mail set categorie = 'information', categorie_par = 'humain' where id = $1",
    )
    .bind(humain)
    .execute(b.pool())
    .await
    .unwrap();

    let w = WorkersTest::demarrer(b.ctx.clone());
    trier_tout(&b).await;
    w.arreter();

    let c = classement(&b, pub_).await;
    assert_eq!(
        (c.0.as_deref(), c.1.as_deref()),
        (Some("publicite"), Some("regle"))
    );
    assert_eq!(c.2, None);
    let c = classement(&b, notif).await;
    assert_eq!(
        (c.0.as_deref(), c.1.as_deref()),
        (Some("notification"), Some("regle"))
    );
    let c = classement(&b, question).await;
    assert_eq!(c.0.as_deref(), Some("question"));
    assert_eq!(c.1.as_deref(), Some("claude"));
    assert_eq!(c.2.as_deref(), Some("a_repondre"));
    assert_eq!(
        c.3.as_deref(),
        Some("Demande la date de livraison de la commande 4471")
    );
    assert_eq!(c.4.as_deref(), Some("trie"));
    let c = classement(&b, humain).await;
    assert_eq!(
        (c.0.as_deref(), c.1.as_deref()),
        (Some("information"), Some("humain"))
    );

    // Un seul appel à Claude (la question), modèle de la tâche, jeton OAuth seulement.
    let appels = faux.appels();
    assert_eq!(appels.len(), 1, "{appels:?}");
    assert!(appels[0].contains("--model\nhaiku\n"), "{}", appels[0]);
    assert!(appels[0].contains("CLE=\n"));
    assert!(appels[0].contains("JETON=sk-ant-oat01-jeton_l5\n"));
    assert!(!appels[0].contains("Soldes"));
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'tri_mail'")
            .await,
        1
    );
    assert_eq!(reglages::appels_tri_du_jour(b.pool()).await.unwrap(), 1);
    b.fermer().await;
}

#[tokio::test]
async fn quota_atteint_tri_auto_coupe_et_cle_api() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let s = b.societe("MS Frouard").await;
    let q1 = mail(
        &b,
        s,
        &eml("q1@x.test", "jean@client.test", "Livraison", QUESTION, ""),
    )
    .await;

    // Tri automatique coupé : rien n'est planifié.
    let mut r = reglages::lire(b.pool()).await.unwrap();
    r.tri_auto = false;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    assert_eq!(tri::pompe(b.pool()).await.unwrap(), 0);
    assert_eq!(classement(&b, q1).await.4, None);

    // Quota à zéro : le mail est examiné une fois (aucune règle), puis attend avec un message.
    r.tri_auto = true;
    r.quota_tri_jour = 0;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    let w = WorkersTest::demarrer(b.ctx.clone());
    tri::pompe(b.pool()).await.unwrap();
    let pool = b.pool().clone();
    assert!(
        attendre(Duration::from_secs(30), || {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, Option<String>>(
                    "select tri_message from mail where id = $1",
                )
                .bind(q1)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_some()
            }
        })
        .await
    );
    let c = classement(&b, q1).await;
    assert_eq!((c.0, c.4.as_deref()), (None, Some("a_trier")));
    assert!(faux.appels().is_empty());
    // Sans Claude, la pompe ne repousse pas un mail déjà examiné.
    sqlx::query("update mail set tri_planifie_le = null where id = $1")
        .bind(q1)
        .execute(b.pool())
        .await
        .unwrap();
    assert_eq!(tri::pompe(b.pool()).await.unwrap(), 0);

    // Clé API choisie : le secret passe par ANTHROPIC_API_KEY, jamais avec le jeton OAuth.
    let chiffre = Chiffreur::new(&CLE_TEST).chiffrer(b"sk-ant-api03-cle_l5");
    parametre::ecrire_secret(b.pool(), parametre::CLAUDE_CLE_API, &chiffre)
        .await
        .unwrap();
    parametre::ecrire(
        b.pool(),
        parametre::CLAUDE_ACCES,
        &json!({ "mode": "cle_api" }),
    )
    .await
    .unwrap();
    r.quota_tri_jour = 10;
    reglages::ecrire(b.pool(), &r).await.unwrap();
    trier_tout(&b).await;
    w.arreter();
    assert_eq!(classement(&b, q1).await.0.as_deref(), Some("question"));
    let appels = faux.appels();
    assert_eq!(appels.len(), 1);
    assert!(
        appels[0].contains("CLE=sk-ant-api03-cle_l5\n"),
        "{}",
        appels[0]
    );
    assert!(appels[0].contains("JETON=\n"));
    b.fermer().await;
}

/// Mail reçu par une vraie boîte de test (compte `a@test.local`) avec SMTP vers `smtp`.
async fn mail_de_boite(b: &Banc, smtp: &SmtpTest, envoyes: &str) -> (i64, i64) {
    let s = b.societe("MS Frouard").await;
    let boite = b.boite_ok(s, "a@test.local", &["INBOX"]).await;
    sqlx::query(
        "update boite_mail set smtp_serveur = '127.0.0.1', smtp_port = $2, smtp_securite = 'clair',
            dossier_envoyes = $3 where id = $1",
    )
    .bind(boite)
    .bind(i32::from(smtp.port))
    .bind(envoyes)
    .execute(b.pool())
    .await
    .unwrap();
    let m = mail(
        b,
        s,
        &eml(
            "orig-l5@client.test",
            "Jean <jean@client.test>",
            "Livraison",
            QUESTION,
            "Reply-To: achats@client.test\r\n",
        ),
    )
    .await;
    sqlx::query("update mail_boite set boite_id = $2 where mail_id = $1")
        .bind(m)
        .bind(boite)
        .execute(b.pool())
        .await
        .unwrap();
    sqlx::query("update mail set categorie = 'question', categorie_par = 'claude', statut_reponse = 'a_repondre' where id = $1")
        .bind(m)
        .execute(b.pool())
        .await
        .unwrap();
    (m, boite)
}

async fn statut_brouillon(b: &Banc, id: i64) -> (String, String, Option<String>) {
    sqlx::query_as("select statut, texte, erreur from brouillon_reponse where id = $1")
        .bind(id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

async fn attendre_brouillon(b: &Banc, id: i64, statut: &'static str) {
    let pool = b.pool().clone();
    let ok = attendre(Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("select statut from brouillon_reponse where id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                == statut
        }
    })
    .await;
    assert!(
        ok,
        "brouillon {id} jamais « {statut} » : {:?}",
        statut_brouillon(b, id).await
    );
}

#[tokio::test]
async fn brouillon_puis_envoi_dans_le_fil() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let smtp = SmtpTest::demarrer().await;
    let envoyes = test_utils::dossier_unique();
    test_utils::creer_dossier("a@test.local", &envoyes).await;
    // Une réponse déjà envoyée : exemple de ton pour Claude, sans son historique.
    test_utils::deposer(
        "a@test.local",
        &envoyes,
        b"From: a@test.local\r\nTo: x@client.test\r\nSubject: Re: x\r\n\r\nSalut Paul, c'est note. A+ Michel\r\n> vieux message\r\n",
    )
    .await;
    let (m, boite) = mail_de_boite(&b, &smtp, &envoyes).await;
    let w = WorkersTest::demarrer(b.ctx.clone());

    let br: i64 = sqlx::query_scalar(
        "insert into brouillon_reponse (mail_id, boite_id, statut) values ($1, $2, 'generation') returning id",
    )
    .bind(m)
    .bind(boite)
    .fetch_one(b.pool())
    .await
    .unwrap();
    file::pousser_brouillon(b.pool(), br).await.unwrap();
    attendre_brouillon(&b, br, "propose").await;
    let (_, texte, erreur) = statut_brouillon(&b, br).await;
    assert!(texte.contains("La commande 4471 part jeudi."), "{texte}");
    assert_eq!(erreur, None);
    let appels = faux.appels();
    assert_eq!(appels.len(), 1);
    assert!(appels[0].contains("--model\nsonnet\n"), "{}", appels[0]);
    assert!(appels[0].contains("Salut Paul, c'est note. A+ Michel"));
    assert!(!appels[0].contains("vieux message"));
    assert!(appels[0].contains("commande 4471 sera livree"));
    // Généré, mais rien ne part sans clic.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(smtp.recus().is_empty());

    // Clic « Envoyer » (ce que fait l'API) : statut `envoi` et tâche poussée.
    sqlx::query(
        "update brouillon_reponse set statut = 'envoi', texte = texte || '\n[relu]' where id = $1",
    )
    .bind(br)
    .execute(b.pool())
    .await
    .unwrap();
    file::pousser_envoi(
        b.pool(),
        TacheEnvoi::EnvoyerReponse {
            brouillon_id: br,
            auteur_id: None,
        },
    )
    .await
    .unwrap();
    attendre_brouillon(&b, br, "envoye").await;
    w.arreter();

    let recus = smtp.recus();
    assert_eq!(recus.len(), 1);
    let parti = &recus[0];
    assert!(parti.contains("To: achats@client.test"), "{parti}");
    assert!(
        parti.contains("From: \"MS Frouard\" <a@test.local>")
            || parti.contains("From: MS Frouard <a@test.local>"),
        "{parti}"
    );
    assert!(parti.contains("Subject: Re: Livraison"));
    assert!(parti.contains("In-Reply-To: <orig-l5@client.test>"));
    assert!(parti.contains("References: <debut-orig-l5@client.test> <orig-l5@client.test>"));
    assert_eq!(smtp.connexions(), ["a@test.local"]);
    let (statut_rep,): (Option<String>,) =
        sqlx::query_as("select statut_reponse from mail where id = $1")
            .bind(m)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(statut_rep.as_deref(), Some("repondu"));
    let (_, _, erreur) = statut_brouillon(&b, br).await;
    assert_eq!(erreur, None, "copie dans les envoyés");
    assert_eq!(
        b.compter("select count(*) from journal where action = 'reponse_envoyee'")
            .await,
        1
    );
    // Copie lue dans les envoyés : l'exemple de départ et la réponse.
    let drapeaux = test_utils::drapeaux("a@test.local", &envoyes).await;
    assert_eq!(drapeaux.len(), 2);
    assert!(
        drapeaux[1].iter().any(|d| d.contains("Seen")),
        "{drapeaux:?}"
    );

    // Une tâche rejouée ne renvoie rien.
    reponse::envoyer(&b.ctx, br, None).await.unwrap();
    assert_eq!(smtp.recus().len(), 1);
    test_utils::supprimer_dossier("a@test.local", &envoyes).await;
    b.fermer().await;
}

#[tokio::test]
async fn envoi_refuse_puis_interrompu() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let smtp = SmtpTest::demarrer().await;
    smtp.refuser
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (m, boite) = mail_de_boite(&b, &smtp, "").await;
    let br: i64 = sqlx::query_scalar(
        "insert into brouillon_reponse (mail_id, boite_id, statut, texte) values ($1, $2, 'envoi', 'Bonjour.') returning id",
    )
    .bind(m)
    .bind(boite)
    .fetch_one(b.pool())
    .await
    .unwrap();
    reponse::envoyer(&b.ctx, br, None).await.unwrap();
    let (statut, _, erreur) = statut_brouillon(&b, br).await;
    assert_eq!(statut, "modifie");
    assert!(erreur.unwrap().starts_with("envoi SMTP"));
    assert_eq!(classement(&b, m).await.2.as_deref(), Some("a_repondre"));
    assert_eq!(
        b.compter("select count(*) from journal where action = 'reponse_echec'")
            .await,
        1
    );

    // Envoi resté en cours (worker arrêté) : rendu à l'utilisateur, jamais relancé.
    sqlx::query("update brouillon_reponse set statut = 'envoi', modifie_le = now() - interval '1 hour' where id = $1")
        .bind(br)
        .execute(b.pool())
        .await
        .unwrap();
    reponse::interrompus(b.pool()).await.unwrap();
    let (statut, _, erreur) = statut_brouillon(&b, br).await;
    assert_eq!(statut, "modifie");
    assert!(erreur.unwrap().starts_with("envoi interrompu"));
    assert!(smtp.recus().is_empty());
    b.fermer().await;
}

#[tokio::test]
async fn purge_des_publicites() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let s = b.societe("MS Frouard").await;
    let mut ids = Vec::new();
    for i in 0..4 {
        ids.push(
            mail(
                &b,
                s,
                &eml(
                    &format!("pub{i}@x.test"),
                    "promo@x.test",
                    "Promo",
                    "Offre",
                    "",
                ),
            )
            .await,
        );
    }
    sqlx::query(
        "update mail set categorie = 'publicite', recu_le = now() - interval '100 days',
            date_mail = now() - interval '100 days' where id = any($1)",
    )
    .bind(&ids)
    .execute(b.pool())
    .await
    .unwrap();
    // ids[1] : récente ; ids[2] : porte une facture ; ids[3] : a un brouillon.
    sqlx::query("update mail set recu_le = now(), date_mail = now() where id = $1")
        .bind(ids[1])
        .execute(b.pool())
        .await
        .unwrap();
    sqlx::query(
        "with p as (
            insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256)
            values ($1, 1, 'f.pdf', 'application/pdf', 1, '\\x01') returning id, sha256)
         insert into document (piece_jointe_id, mail_id, sha256, etat, type)
         select id, $1, sha256, 'lu', 'facture' from p",
    )
    .bind(ids[2])
    .execute(b.pool())
    .await
    .unwrap();
    let boite: i64 = sqlx::query_scalar("select boite_id from mail_boite where mail_id = $1")
        .bind(ids[3])
        .fetch_one(b.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into brouillon_reponse (mail_id, boite_id, statut) values ($1, $2, 'abandonne')",
    )
    .bind(ids[3])
    .bind(boite)
    .execute(b.pool())
    .await
    .unwrap();
    let chemin: String = sqlx::query_scalar("select chemin_eml from mail where id = $1")
        .bind(ids[0])
        .fetch_one(b.pool())
        .await
        .unwrap();
    let absolu = gmflow_core::stockage::chemin_absolu(&b.ctx.config.fichiers, &chemin);
    assert!(absolu.exists());

    let n = purge::publicites(b.pool(), &b.ctx.config.fichiers, purge::PUBLICITES_JOURS)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let restants: Vec<i64> = sqlx::query_scalar("select id from mail order by id")
        .fetch_all(b.pool())
        .await
        .unwrap();
    assert_eq!(restants, ids[1..]);
    assert!(!absolu.exists());
    assert_eq!(
        b.compter("select count(*) from journal where action = 'purge_publicites'")
            .await,
        1
    );
    b.fermer().await;
}

#[tokio::test]
async fn tri_appris_des_classements_a_la_main() {
    let faux = FauxClaude::new();
    let b = banc(&faux).await;
    let s = b.societe("MS Frouard").await;
    // Trois mails de Jean classés « information » à la main.
    for i in 0..3 {
        let id = mail(
            &b,
            s,
            &eml(
                &format!("h{i}@x.test"),
                "Jean <jean@client.test>",
                "Point livraison",
                "Pour info, la livraison est prevue jeudi.",
                "",
            ),
        )
        .await;
        sqlx::query(
            "update mail set categorie = 'information', categorie_par = 'humain',
                categorie_auto = 'question', categorie_auto_par = 'claude',
                tri_etat = 'trie', trie_le = now() - make_interval(mins => $2) where id = $1",
        )
        .bind(id)
        .bind(10 - i)
        .execute(b.pool())
        .await
        .unwrap();
    }
    let de_jean = mail(
        &b,
        s,
        &eml("j4@x.test", "jean@client.test", "Question ?", QUESTION, ""),
    )
    .await;
    let autre = mail(
        &b,
        s,
        &eml(
            "p1@x.test",
            "Paul <paul@autre.test>",
            "Livraison",
            QUESTION,
            "",
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    trier_tout(&b).await;
    w.arreter();

    // Jean : la règle apprise suffit, sans Claude.
    let c = classement(&b, de_jean).await;
    assert_eq!(
        (c.0.as_deref(), c.1.as_deref()),
        (Some("information"), Some("regle"))
    );
    // Paul : Claude, avec les classements à la main en exemples.
    let c = classement(&b, autre).await;
    assert_eq!(c.1.as_deref(), Some("claude"));
    let appels = faux.appels();
    assert_eq!(appels.len(), 1, "{appels:?}");
    assert!(appels[0].contains("classés à la main"), "{}", appels[0]);
    assert!(
        appels[0].contains("=> information (et non question)"),
        "{}",
        appels[0]
    );
    b.fermer().await;
}
