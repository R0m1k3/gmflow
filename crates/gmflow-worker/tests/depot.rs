//! Lot 3 de bout en bout : fiche fournisseur, contrôles, dépôt automatique par mail (serveur
//! SMTP de test), doublons, échec d'envoi et alertes de relève.

mod commun;

use std::time::Duration;

use commun::{Banc, WorkersTest, attendre, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::test_utils::SmtpTest;
use gmflow_core::{depot, expedition, parametre};
use gmflow_worker::envoi;
use serde_json::{Value, json};

const SIRET_FOURNISSEUR: &str = "55210055400013";
const TVA_FOURNISSEUR: &str = "FR96552100554";

/// Faux Claude : réponse d'identification si le prompt système est celui du rapprochement des
/// fournisseurs, réponse d'extraction sinon.
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(extraction: &Value, identification: &Value) -> Self {
        let rep = |v: &Value| {
            json!({ "type": "result", "is_error": false, "result": "", "structured_output": v })
                .to_string()
        };
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-depot").unwrap();
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in *'rapproches le fournisseur'*) cat <<'FIN'\n{}\nFIN\nexit 0;; esac; done\ncat <<'FIN'\n{}\nFIN\n",
            rep(identification),
            rep(extraction)
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
}

struct Monde {
    b: Banc,
    smtp: SmtpTest,
    _faux: Option<FauxClaude>,
}

async fn monde(faux: Option<FauxClaude>) -> Monde {
    let mut b = Banc::new().await;
    if let Some(f) = &faux {
        let mut config = (*b.ctx.config).clone();
        config.claude_bin = f.bin();
        b.ctx.config = std::sync::Arc::new(config);
        parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_depot")
            .await
            .unwrap();
    }
    let smtp = SmtpTest::demarrer().await;
    parametre::ecrire(
        b.pool(),
        expedition::CLE,
        &serde_json::to_value(smtp.expedition()).unwrap(),
    )
    .await
    .unwrap();
    parametre::ecrire(
        b.pool(),
        expedition::CLE_ALERTES,
        &json!(["michael@test.local"]),
    )
    .await
    .unwrap();
    Monde {
        b,
        smtp,
        _faux: faux,
    }
}

async fn societe(b: &Banc, nom: &str, siret: &str, auto: bool) -> i64 {
    sqlx::query_scalar(
        "insert into societe (raison_sociale, siret, outil_depot, adresse_depot, depot_auto)
         values ($1, $2, 'dext', 'depot@dext.test', $3) returning id",
    )
    .bind(nom)
    .bind(siret)
    .bind(auto)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

async fn fournisseur(b: &Banc, nom: &str, siret: Option<&str>, statut: &str) -> i64 {
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, siret, statut) values ($1, $2, $3, $4) returning id",
    )
    .bind(nom)
    .bind(gmflow_core::fournisseurs::normaliser(nom))
    .bind(siret)
    .bind(statut)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

fn facture_electronique(numero: &str) -> Vec<u8> {
    let xml = fixtures::xml_cii_du(numero, SIRET_A, false, &fixtures::aujourdhui_aaaammjj());
    fixtures::pdf_facturx(&xml, &[&format!("Facture {numero}")])
}

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    id: i64,
    statut_depot: Option<String>,
    controles: Value,
    fournisseur_id: Option<i64>,
    societe_id: Option<i64>,
}

async fn doc(b: &Banc, mail_id: i64) -> Doc {
    sqlx::query_as(
        "select id, statut_depot, controles, fournisseur_id, societe_id from document
         where mail_id = $1 order by id limit 1",
    )
    .bind(mail_id)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

async fn attendre_statut(b: &Banc, id: i64, statut: &str) {
    let pool = b.pool().clone();
    let ok = attendre(Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "select statut_depot from document where id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .as_deref()
                == Some(statut)
        }
    })
    .await;
    let reel: Option<String> =
        sqlx::query_scalar("select statut_depot from document where id = $1")
            .bind(id)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert!(ok, "statut attendu {statut}, obtenu {reel:?}");
}

fn codes(d: &Doc) -> Vec<String> {
    d.controles
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn facture_conforme_deposee_automatiquement_et_doublon_bloque() {
    let m = monde(None).await;
    let b = &m.b;
    let a = societe(b, "Magasin Frouard", SIRET_A, true).await;
    let f = fournisseur(
        b,
        "Grossiste Durand & Fils",
        Some(SIRET_FOURNISSEUR),
        "valide",
    )
    .await;
    let pdf = facture_electronique("FA-31");
    let m1 = mail(
        b,
        a,
        &fixtures::eml(
            "d1@test",
            "Facture FA-31",
            &[("FA-31.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(b, &[m1]).await;
    let d1 = doc(b, m1).await;
    assert_eq!(d1.fournisseur_id, Some(f));
    attendre_statut(b, d1.id, "depose").await;

    let recus = m.smtp.recus();
    assert_eq!(recus.len(), 1);
    let envoye = gmflow_core::eml::decouper(recus[0].as_bytes()).unwrap();
    assert_eq!(envoye.destinataires, ["depot@dext.test"]);
    assert!(envoye.objet.contains("FA-31"), "objet : {}", envoye.objet);
    assert_eq!(envoye.pieces.len(), 1);
    assert_eq!(envoye.pieces[0].nom, "FA-31.pdf");
    assert_eq!(envoye.pieces[0].sha256, gmflow_core::eml::sha256(&pdf));
    assert_eq!(
        b.compter(
            "select count(*) from envoi_depot where etat = 'envoye' and message_id_smtp is not null"
        )
        .await,
        1
    );
    // Domaine appris : la fiche a été trouvée par son SIRET.
    let domaines: Vec<String> =
        sqlx::query_scalar("select domaines from fournisseur where id = $1")
            .bind(f)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(domaines, ["fournisseur.test"]);

    // Même fichier renvoyé dans un autre mail : doublon, rien n'est envoyé.
    let m2 = mail(
        b,
        a,
        &fixtures::eml(
            "d2@test",
            "Relance FA-31",
            &[("FA-31.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    lire_tout(b, &[m2]).await;
    let d2 = doc(b, m2).await;
    assert_eq!(d2.statut_depot.as_deref(), Some("doublon"));
    assert_eq!(codes(&d2)[0], "doublon");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(m.smtp.recus().len(), 1);
    w.arreter();
    m.b.fermer().await;
}

#[tokio::test]
async fn fournisseur_inconnu_puis_fiche_validee() {
    let m = monde(None).await;
    let b = &m.b;
    let a = societe(b, "Magasin Frouard", SIRET_A, true).await;
    let pdf = facture_electronique("FA-40");
    let m1 = mail(
        b,
        a,
        &fixtures::eml(
            "n1@test",
            "Facture FA-40",
            &[("FA-40.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(b, &[m1]).await;
    let d = doc(b, m1).await;
    let f = d.fournisseur_id.expect("fiche créée");
    let fiche: (String, String, Option<String>, Option<String>, Vec<String>) =
        sqlx::query_as("select nom, statut, siret, tva, domaines from fournisseur where id = $1")
            .bind(f)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(fiche.0, "Grossiste Durand & Fils");
    assert_eq!(fiche.1, "a_confirmer");
    assert_eq!(fiche.2.as_deref(), Some(SIRET_FOURNISSEUR));
    assert_eq!(fiche.3.as_deref(), Some(TVA_FOURNISSEUR));
    assert_eq!(fiche.4, ["fournisseur.test"]);
    assert_eq!(d.statut_depot.as_deref(), Some("a_verifier"));
    assert_eq!(codes(&d), ["fournisseur"]);
    assert!(m.smtp.recus().is_empty());

    // La fiche validée par un administrateur : la facture repasse les contrôles et part.
    sqlx::query("update fournisseur set statut = 'valide' where id = $1")
        .bind(f)
        .execute(b.pool())
        .await
        .unwrap();
    depot::reevaluer(b.pool(), None, Some(f)).await.unwrap();
    attendre_statut(b, d.id, "depose").await;
    assert_eq!(m.smtp.recus().len(), 1);

    // Un second document du même fournisseur, trouvé par le domaine, va sur la même fiche.
    let m2 = mail(
        b,
        a,
        &fixtures::eml(
            "n2@test",
            "Facture FA-41",
            &[(
                "FA-41.pdf",
                "application/pdf",
                &facture_electronique("FA-41"),
            )],
            false,
        ),
    )
    .await;
    lire_tout(b, &[m2]).await;
    assert_eq!(doc(b, m2).await.fournisseur_id, Some(f));
    assert_eq!(b.compter("select count(*) from fournisseur").await, 1);
    w.arreter();
    m.b.fermer().await;
}

#[tokio::test]
async fn claude_departage_les_fiches_proches_et_structure_par_defaut() {
    let extraction = json!({
        "type": "facture", "societe_id": null,
        "fournisseur": { "nom": "METRO", "siret": null, "tva": null },
        "numero": "M-9", "date": null, "numero_commande": null,
        "montant_ht": 10.0, "montant_tva": 2.0, "montant_ttc": 12.0,
        "confiance": { "type": 0.95, "societe": 0.2, "fournisseur": 0.9, "numero": 1, "date": 0, "montants": 0.9 }
    });
    // L'id de la bonne fiche n'est connu qu'une fois la base créée : le faux Claude répond la
    // première fiche créée (id 1 dans une base neuve).
    let faux = FauxClaude::new(&extraction, &json!({ "fournisseur_id": 1 }));
    let m = monde(Some(faux)).await;
    let b = &m.b;
    let a = societe(b, "Magasin Frouard", SIRET_A, false).await;
    let cc = fournisseur(b, "Metro Cash & Carry", None, "valide").await;
    assert_eq!(cc, 1);
    let autre = fournisseur(b, "Metro Libre Service", None, "valide").await;
    sqlx::query("update fournisseur set societe_defaut_id = $1 where id = $2")
        .bind(a)
        .bind(cc)
        .execute(b.pool())
        .await
        .unwrap();
    let pdf = fixtures::pdf_texte(&[
        "FACTURE M-9 METRO",
        "Total HT 10,00 EUR TVA 2,00 EUR Total TTC 12,00 EUR",
        "Conditions de paiement : 30 jours fin de mois, pénalités de retard au taux légal",
    ]);
    let m1 = mail(
        b,
        a,
        &fixtures::eml(
            "c1@test",
            "Votre facture",
            &[("M-9.pdf", "application/pdf", &pdf)],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(b, &[m1]).await;
    let d = doc(b, m1).await;
    assert_eq!(d.fournisseur_id, Some(cc), "autre fiche : {autre}");
    assert_eq!(d.societe_id, Some(a), "structure par défaut du fournisseur");
    assert_eq!(
        b.compter("select count(*) from appel_claude where tache = 'identification_fournisseur'")
            .await,
        1
    );
    // Société par la règle du fournisseur (confiance 0,9) : le contrôle « societe » passe.
    assert!(
        !codes(&d).contains(&"societe".to_string()),
        "{:?}",
        codes(&d)
    );
    assert_eq!(d.statut_depot.as_deref(), Some("a_verifier"));
    w.arreter();
    m.b.fermer().await;
}

#[tokio::test]
async fn echec_d_envoi_puis_redepot_valide() {
    let m = monde(None).await;
    let b = &m.b;
    m.smtp
        .refuser
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let a = societe(b, "Magasin Frouard", SIRET_A, true).await;
    fournisseur(
        b,
        "Grossiste Durand & Fils",
        Some(SIRET_FOURNISSEUR),
        "valide",
    )
    .await;
    let m1 = mail(
        b,
        a,
        &fixtures::eml(
            "e1@test",
            "Facture FA-50",
            &[(
                "FA-50.pdf",
                "application/pdf",
                &facture_electronique("FA-50"),
            )],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(b, &[m1]).await;
    let d = doc(b, m1).await;
    attendre_statut(b, d.id, "erreur").await;
    w.arreter();
    assert_eq!(
        b.compter("select count(*) from envoi_depot where etat = 'erreur'")
            .await,
        1
    );
    m.smtp
        .refuser
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // Sans validation humaine, un dépôt en erreur n'est jamais relancé.
    envoi::deposer(&b.ctx, d.id, false, None).await.unwrap();
    assert!(m.smtp.recus().is_empty());
    // Validé (« Redéposer » d'un administrateur) : il part.
    sqlx::query("update document set valide_le = now() where id = $1")
        .bind(d.id)
        .execute(b.pool())
        .await
        .unwrap();
    envoi::deposer(&b.ctx, d.id, false, None).await.unwrap();
    assert_eq!(doc(b, m1).await.statut_depot.as_deref(), Some("depose"));
    assert_eq!(m.smtp.recus().len(), 1);
    m.b.fermer().await;
}

#[tokio::test]
async fn alertes_de_releve_et_envoi_interrompu() {
    let m = monde(None).await;
    let b = &m.b;
    let a = societe(b, "Magasin Frouard", SIRET_A, false).await;
    let boite = b.boite_ok(a, "a@test.local", &["INBOX"]).await;
    sqlx::query(
        "update boite_mail set erreur_depuis = now() - interval '40 minutes',
            derniere_erreur = 'connexion refusée' where id = $1",
    )
    .bind(boite)
    .execute(b.pool())
    .await
    .unwrap();
    // Un envoi resté « en cours » depuis 20 minutes (worker arrêté pendant l'envoi).
    let m1 = mail(
        b,
        a,
        &fixtures::eml(
            "i1@test",
            "Facture",
            &[(
                "FA-60.pdf",
                "application/pdf",
                &facture_electronique("FA-60"),
            )],
            false,
        ),
    )
    .await;
    gmflow_worker::lecture::pompe(b.pool()).await.unwrap();
    let id: i64 = sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(m1)
        .fetch_one(b.pool())
        .await
        .unwrap();
    sqlx::query("update document set etat = 'lu', type = 'facture', statut_depot = 'en_envoi' where id = $1")
        .bind(id)
        .execute(b.pool())
        .await
        .unwrap();
    sqlx::query(
        "insert into envoi_depot (document_id, societe_id, adresse, quand) values ($1, $2, 'x@dext.test', now() - interval '20 minutes')",
    )
    .bind(id)
    .bind(a)
    .execute(b.pool())
    .await
    .unwrap();

    envoi::alertes(&b.ctx).await.unwrap();
    envoi::alertes(&b.ctx).await.unwrap();
    let objets: Vec<String> = m
        .smtp
        .recus()
        .iter()
        .map(|r| gmflow_core::eml::decouper(r.as_bytes()).unwrap().objet)
        .collect();
    assert_eq!(objets.len(), 2, "{objets:?}");
    assert!(objets.iter().any(|o| o.contains("dépôt interrompu")));
    assert!(objets.iter().any(|o| o.contains("relève IMAP en échec")));
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>("select statut_depot from document where id = $1")
            .bind(id)
            .fetch_one(b.pool())
            .await
            .unwrap()
            .as_deref(),
        Some("erreur")
    );

    sqlx::query("update boite_mail set erreur_depuis = null where id = $1")
        .bind(boite)
        .execute(b.pool())
        .await
        .unwrap();
    envoi::alertes(&b.ctx).await.unwrap();
    let dernier = m.smtp.recus().last().cloned().unwrap();
    assert!(
        gmflow_core::eml::decouper(dernier.as_bytes())
            .unwrap()
            .objet
            .contains("rétablie")
    );
    assert_eq!(
        b.compter("select count(*) from boite_mail where alerte_le is not null")
            .await,
        0
    );
    m.b.fermer().await;
}
