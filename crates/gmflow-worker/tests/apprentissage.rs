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
