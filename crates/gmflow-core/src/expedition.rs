//! Adresse d'expédition dédiée (dépôts Dext/Yooz, alertes) : configuration dans `parametre`
//! (clé `smtp_expedition`, mot de passe chiffré dans la même ligne) et envoi SMTP.

use std::time::Duration;

use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::crypto::Chiffreur;
use crate::parametre;

pub const CLE: &str = "smtp_expedition";
pub const CLE_ALERTES: &str = "alerte_destinataires";
pub const CLE_TEST: &str = "smtp_expedition_test";

pub const DELAI: Duration = Duration::from_secs(60);

/// `tls` : TLS dès la connexion (port 465) ; `starttls` : port 587. `clair` : serveur de
/// test seulement (fonction `test-utils`).
pub const SECURITES: &[&str] = &["tls", "starttls"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Expedition {
    pub serveur: String,
    pub port: u16,
    pub securite: String,
    /// Identifiant SMTP ; vide : pas d'authentification.
    #[serde(default)]
    pub identifiant: String,
    /// Adresse From des mails envoyés.
    pub adresse: String,
    #[serde(default)]
    pub nom: Option<String>,
}

impl Expedition {
    pub fn controler(&self) -> Result<(), String> {
        if self.serveur.trim().is_empty() {
            return Err("serveur SMTP obligatoire".into());
        }
        if !SECURITES.contains(&self.securite.as_str()) && !securite_test(&self.securite) {
            return Err("sécurité : tls ou starttls".into());
        }
        if self.adresse.parse::<Mailbox>().is_err() {
            return Err(format!(
                "adresse d'expédition « {} » invalide",
                self.adresse
            ));
        }
        Ok(())
    }

    fn expediteur(&self) -> anyhow::Result<Mailbox> {
        let adresse = self.adresse.parse()?;
        Ok(Mailbox::new(
            self.nom.clone().filter(|n| !n.trim().is_empty()),
            adresse,
        ))
    }
}

/// Adresse mail utilisable comme destinataire ou expéditeur.
pub fn adresse_valide(a: &str) -> bool {
    a.parse::<Mailbox>().is_ok()
}

fn securite_test(s: &str) -> bool {
    cfg!(feature = "test-utils") && s == "clair"
}

/// Configuration et mot de passe en clair, si l'expédition est configurée.
pub async fn lire(
    pool: &PgPool,
    chiffreur: &Chiffreur,
) -> anyhow::Result<Option<(Expedition, Option<String>)>> {
    let Some(v) = parametre::lire(pool, CLE).await? else {
        return Ok(None);
    };
    let Ok(e) = serde_json::from_value::<Expedition>(v) else {
        return Ok(None);
    };
    let mdp = match parametre::lire_secret(pool, CLE).await? {
        Some(c) => Some(chiffreur.dechiffrer_texte(&c)?),
        None => None,
    };
    Ok(Some((e, mdp)))
}

pub async fn destinataires_alertes(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    Ok(parametre::lire(pool, CLE_ALERTES)
        .await?
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default())
}

fn transport(
    e: &Expedition,
    mdp: Option<&str>,
) -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    let mut b = match e.securite.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&e.serveur)?,
        "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&e.serveur)?,
        s if securite_test(s) => {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&e.serveur)
        }
        s => anyhow::bail!("sécurité SMTP inconnue : {s}"),
    }
    .port(e.port)
    .timeout(Some(DELAI));
    if !e.identifiant.is_empty() {
        b = b.credentials(Credentials::new(
            e.identifiant.clone(),
            mdp.unwrap_or_default().to_string(),
        ));
    }
    Ok(b.build())
}

/// Pièce jointe d'un mail envoyé.
pub struct PieceJointe<'a> {
    pub nom: &'a str,
    pub type_mime: &'a str,
    pub contenu: Vec<u8>,
}

/// Envoie un mail texte, avec au plus une pièce jointe. Rend le Message-ID.
pub async fn envoyer(
    e: &Expedition,
    mdp: Option<&str>,
    a: &[String],
    objet: &str,
    corps: &str,
    piece: Option<PieceJointe<'_>>,
) -> anyhow::Result<String> {
    let domaine = e
        .adresse
        .rsplit_once('@')
        .map(|(_, d)| d)
        .unwrap_or("gmflow");
    let message_id = format!("<{}@{domaine}>", uuid::Uuid::new_v4());
    let mut b = Message::builder()
        .from(e.expediteur()?)
        .subject(objet)
        .message_id(Some(message_id.clone()));
    if a.is_empty() {
        anyhow::bail!("aucun destinataire");
    }
    for d in a {
        b = b.to(d
            .parse()
            .map_err(|_| anyhow::anyhow!("adresse « {d} » invalide"))?);
    }
    let texte = SinglePart::plain(corps.to_string());
    let message = match piece {
        None => b.singlepart(texte)?,
        Some(p) => {
            let ct = ContentType::parse(p.type_mime)
                .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());
            b.multipart(
                MultiPart::mixed()
                    .singlepart(texte)
                    .singlepart(Attachment::new(p.nom.to_string()).body(p.contenu, ct)),
            )?
        }
    };
    transport(e, mdp)?.send(message).await?;
    Ok(message_id)
}

/// Envoie un message déjà construit (réponse à un mail) avec cette configuration SMTP.
pub async fn envoyer_message(
    e: &Expedition,
    mdp: Option<&str>,
    message: lettre::Message,
) -> anyhow::Result<()> {
    transport(e, mdp)?.send(message).await?;
    Ok(())
}

/// Connexion SMTP et authentification, sans rien envoyer (test d'une boîte avant ses réponses).
pub async fn tester_connexion(e: &Expedition, mdp: Option<&str>) -> anyhow::Result<()> {
    if !transport(e, mdp)?.test_connection().await? {
        anyhow::bail!("le serveur SMTP ne répond plus après la connexion");
    }
    Ok(())
}
