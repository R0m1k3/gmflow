//! Ce que la lecture garde et apprend des pièces : IBAN lu (clé vérifiée), exemples de pièces
//! validées et corrections récurrentes du fournisseur dans le prompt, coût des appels.

mod commun;

use commun::{Banc, WorkersTest, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::parametre;
use serde_json::{Value, json};

/// Faux Claude : journalise ses arguments, puis rend la réponse du fichier `reponse` du dossier
/// (modifiable entre deux lectures).
struct FauxClaude {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxClaude {
    fn new(reponse: &Value) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-claude-appr").unwrap();
        let journal = dossier.0.join("journal");
        let fichier = dossier.0.join("reponse");
        let script = format!(
            "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; echo ===; }} >> '{}'\ncat '{}'\n",
            journal.display(),
            fichier.display()
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let f = Self { dossier };
        f.repondre(reponse);
        f
    }

    fn repondre(&self, reponse: &Value) {
        std::fs::write(self.dossier.0.join("reponse"), reponse.to_string()).unwrap();
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

fn reponse(sortie: Value) -> Value {
    json!({ "type": "result", "is_error": false, "result": "", "structured_output": sortie })
}

fn facture(numero: &str, iban: Option<&str>) -> Value {
    json!({
        "type": "facture", "societe_id": null,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": "FR96552100554", "groupe": null, "entite": null },
        "numero": numero, "date": "2026-11-12", "numero_commande": null, "echeance": null,
        "montant_ht": 50.0, "montant_tva": 10.0, "montant_ttc": 60.0,
        "remise_ht": null, "remise_taux": null, "annexes_ht": null,
        "iban": iban, "bic": null, "lignes": [], "numeros_bl": [],
        "confiance": { "type": 0.95, "societe": 0.5, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0.9 }
    })
}

async fn banc(faux: &FauxClaude) -> Banc {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_appr")
        .await
        .unwrap();
    b
}

async fn societe(b: &Banc) -> i64 {
    sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Frouard', $1) returning id",
    )
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap()
}

/// Mail d'un expéditeur avec un PDF texte : rend l'id du mail.
async fn recevoir(b: &Banc, societe: i64, expediteur: &str, lignes: &[&str]) -> i64 {
    let pdf = fixtures::pdf_texte(lignes);
    let mut eml = fixtures::eml(
        &format!(
            "{}-{}@test",
            lignes[0].replace(' ', ""),
            uuid::Uuid::new_v4()
        ),
        "Votre facture",
        &[("f.pdf", "application/pdf", &pdf)],
        false,
    );
    // L'expéditeur du mail de test est remplacé par celui voulu.
    let texte = String::from_utf8_lossy(&eml).into_owned();
    if let Some(debut) = texte.find("From: ") {
        let fin = debut + texte[debut..].find("\r\n").unwrap_or(0);
        eml = format!("{}From: {expediteur}{}", &texte[..debut], &texte[fin..]).into_bytes();
    }
    mail(b, societe, &eml).await
}

async fn document_du_mail(b: &Banc, mail_id: i64) -> i64 {
    sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(mail_id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn iban_lu_garde_seulement_avec_une_cle_juste() {
    let faux = FauxClaude::new(&reponse(facture(
        "F-1",
        Some("FR76 3000 6000 0112 3456 7890 189"),
    )));
    let b = banc(&faux).await;
    let a = societe(&b).await;
    let m1 = recevoir(
        &b,
        a,
        "compta@durand.fr",
        &["FACTURE F-1", "Total TTC 60,00"],
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m1]).await;
    let d1 = document_du_mail(&b, m1).await;
    let iban: Option<String> = sqlx::query_scalar("select iban from document where id = $1")
        .bind(d1)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(iban.as_deref(), Some("FR7630006000011234567890189"));
    // La consigne de lecture de l'IBAN est dans le prompt.
    assert!(faux.appels()[0].contains("iban = l'IBAN du compte du fournisseur à payer"));

    // Clé fausse : champ vide, et la lecture le signale (nouvel essai, puis message).
    faux.repondre(&reponse(facture(
        "F-2",
        Some("FR7630006000011234567890188"),
    )));
    let m2 = recevoir(
        &b,
        a,
        "compta@durand.fr",
        &["FACTURE F-2", "Total TTC 60,00"],
    )
    .await;
    lire_tout(&b, &[m2]).await;
    w.arreter();
    let d2 = document_du_mail(&b, m2).await;
    let (iban, message): (Option<String>, Option<String>) =
        sqlx::query_as("select iban, message from document where id = $1")
            .bind(d2)
            .fetch_one(b.pool())
            .await
            .unwrap();
    assert_eq!(iban, None);
    assert!(message.unwrap_or_default().contains("IBAN"));
    b.fermer().await;
}

/// Valide à la main le document (comme « Valider » après une correction de l'échéance).
async fn valider(b: &Banc, document: i64, societe: i64) {
    sqlx::query(
        "update document set verifie_le = now(), societe_id = $2,
                corrections = '{\"date_echeance\": {\"avant\": null, \"apres\": \"2026-12-31\"}}'
         where id = $1",
    )
    .bind(document)
    .bind(societe)
    .execute(b.pool())
    .await
    .unwrap();
}

/// Prompt de la dernière lecture faite (le texte du PDF est lu par Claude dans un fichier, il
/// n'est pas dans le prompt).
fn derniere_lecture(faux: &FauxClaude) -> String {
    faux.appels()
        .into_iter()
        .rfind(|a| a.contains("Tu lis des documents reçus par mail"))
        .expect("aucune lecture")
}

#[tokio::test]
async fn prompt_avec_exemple_valide_et_corrections_de_la_meme_societe() {
    // Petit fournisseur qui écrit depuis une adresse gmail : ni domaine utile, ni SIREN dans le
    // texte ; seule l'adresse exacte, déjà vue sur une pièce validée, le désigne.
    let expediteur = "durand.menuiserie@gmail.com";
    let mut f1 = facture("F-1", None);
    f1["lignes"] = json!([{
        "reference": "PL-12", "designation": "Planche chêne 2 m", "quantite": 4.0, "unite": "u",
        "prix_unitaire_ht": 12.5, "montant_ht": 50.0, "gencod": null, "colis": null, "pcb": null,
        "remise_taux": null, "taux_tva": 20.0, "deee_ht": null, "ecomob_ht": null,
        "pv_conseille": null
    }]);
    let faux = FauxClaude::new(&reponse(f1));
    let b = banc(&faux).await;
    let a = societe(&b).await;
    let autre: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ('Magasin Houdemont', '73282932000074')
         returning id",
    )
    .fetch_one(b.pool())
    .await
    .unwrap();
    let w = WorkersTest::demarrer(b.ctx.clone());
    let m1 = recevoir(&b, a, expediteur, &["FACTURE F-1", "Total TTC 60,00"]).await;
    lire_tout(&b, &[m1]).await;
    let d1 = document_du_mail(&b, m1).await;
    // Avant toute validation : rien d'appris dans le prompt.
    let p1 = derniere_lecture(&faux);
    assert!(!p1.contains("ne recopie rien"), "{p1}");
    valider(&b, d1, a).await;

    // Même expéditeur, même société : exemple positif et résumé des corrections.
    faux.repondre(&reponse(facture("F-2", None)));
    let m2 = recevoir(&b, a, expediteur, &["FACTURE F-2", "Total TTC 60,00"]).await;
    lire_tout(&b, &[m2]).await;
    let p2 = derniere_lecture(&faux);
    assert!(p2.contains("dernière pièce de ce fournisseur validée par un humain (facture)"));
    assert!(p2.contains("ne recopie rien de cet exemple"));
    assert!(p2.contains("\"numero\":\"F-1\""), "{p2}");
    assert!(p2.contains("Planche chêne 2 m"));
    assert!(p2.contains(
        "- echeance : corrigé 1 fois sur 1 (dernière fois : lu vide, corrigé en « 2026-12-31 »)"
    ));

    // Même expéditeur reçu par une boîte d'une autre société : rien de la société A.
    faux.repondre(&reponse(facture("F-3", None)));
    let m3 = recevoir(&b, autre, expediteur, &["FACTURE F-3", "Total TTC 60,00"]).await;
    lire_tout(&b, &[m3]).await;
    w.arreter();
    let p3 = derniere_lecture(&faux);
    assert!(!p3.contains("F-1"), "{p3}");
    assert!(!p3.contains("Planche"));
    assert!(!p3.contains("corrigé 1 fois"));
    b.fermer().await;
}
