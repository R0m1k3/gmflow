//! Essai de connexion d'une boîte mail avec des paramètres pas encore enregistrés (assistant
//! d'ajout), ou essai SMTP d'une boîte existante. L'app écrit la demande dans `parametre`
//! (clé `essai_boite:<uuid>`, mot de passe chiffré dans `valeur_chiffree`) et pousse
//! `TacheBoite::Essai` ; le worker se connecte, écrit le résultat dans `valeur` et efface le
//! mot de passe. L'app ne se connecte jamais elle-même à un serveur de mail.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::imap::{Dossier, ErreurImap};

pub const PREFIXE: &str = "essai_boite:";

/// Durée de vie d'un essai (les plus vieux sont effacés à chaque nouvel essai).
pub const DUREE_MINUTES: i32 = 60;

pub fn cle(id: Uuid) -> String {
    format!("{PREFIXE}{id}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ParamImap {
    pub serveur: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ParamSmtp {
    pub serveur: String,
    pub port: u16,
    pub securite: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultatImap {
    pub ok: bool,
    /// Phrase pour l'utilisateur (« Mot de passe refusé : … »).
    pub message: String,
    /// Erreur brute du serveur, pour un administrateur.
    pub detail: Option<String>,
    pub dossiers: Vec<Dossier>,
    /// `INBOX` si le serveur l'a.
    pub reception: Option<String>,
    /// Dossier des envoyés repéré (nom IMAP).
    pub envoyes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultatSmtp {
    pub ok: bool,
    pub message: String,
    pub detail: Option<String>,
}

/// Contenu de `parametre.valeur` pour un essai.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, rename = "EssaiBoite"))]
pub struct Essai {
    /// `en_cours` puis `termine`.
    #[cfg_attr(feature = "ts", ts(type = "'en_cours' | 'termine'"))]
    pub etat: String,
    pub cree_le: String,
    pub boite_id: Option<i64>,
    pub identifiant: String,
    pub imap: Option<ParamImap>,
    pub smtp: Option<ParamSmtp>,
    pub resultat_imap: Option<ResultatImap>,
    pub resultat_smtp: Option<ResultatSmtp>,
    pub termine_le: Option<String>,
    /// Empreinte du mot de passe essayé (`empreinte`), pour qu'une création de boîte reprenne
    /// un essai réussi sans le refaire. Jamais rendue par l'API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub mdp_empreinte: Option<String>,
}

/// Empreinte d'un mot de passe pour un essai : SHA-256 de l'id de l'essai suivi du mot de
/// passe, en hexadécimal (salée par l'id : une empreinte ne sert que pour son essai).
pub fn empreinte(id: Uuid, mot_de_passe: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(id.as_bytes());
    h.update(mot_de_passe.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Âge en minutes d'un essai (`cree_le`, RFC 3339) ; `None` si la date est illisible.
pub fn age_minutes(e: &Essai) -> Option<i64> {
    let cree =
        time::OffsetDateTime::parse(&e.cree_le, &time::format_description::well_known::Rfc3339)
            .ok()?;
    Some((time::OffsetDateTime::now_utc() - cree).whole_minutes())
}

/// Erreur IMAP dite en clair, et le détail brut.
pub fn message_imap(e: &ErreurImap) -> (String, String) {
    let brut = e.to_string();
    let m = match e {
        ErreurImap::RefusAuth(_) => {
            "Mot de passe refusé : pour Gmail et Infomaniak, il faut un mot de passe d'application \
             (pas le mot de passe habituel du compte)."
                .to_string()
        }
        ErreurImap::Autre(_) => message_reseau(&brut, "IMAP", "993"),
    };
    (m, brut)
}

/// Chaîne des causes sans redite : lettre répète souvent la cause dans le message
/// (« Connection error: Connection error: received corrupt message… »).
fn chaine_sans_redite(e: &anyhow::Error) -> String {
    let mut parts: Vec<String> = Vec::new();
    for cause in e.chain() {
        let s = cause.to_string();
        if parts.iter().any(|p| p.contains(&s)) {
            continue;
        }
        parts.push(s);
    }
    // Préfixe répété dans un même message (« Connection error: Connection error: … »).
    let mut morceaux: Vec<&str> = Vec::new();
    let tout = parts.join(" : ");
    for m in tout.split(": ") {
        if morceaux.last() != Some(&m) {
            morceaux.push(m);
        }
    }
    morceaux.join(": ")
}

/// Erreur SMTP dite en clair, et le détail brut.
pub fn message_smtp(e: &anyhow::Error) -> (String, String) {
    let brut = chaine_sans_redite(e);
    let b = brut.to_lowercase();
    let m = if b.contains("535") || b.contains("534") || b.contains("authentication") {
        "Mot de passe refusé par le serveur d'envoi : il faut un mot de passe d'application."
            .to_string()
    } else {
        message_reseau(&brut, "SMTP", "465 (TLS) ou 587 (STARTTLS)")
    };
    (m, brut)
}

fn message_reseau(brut: &str, protocole: &str, ports: &str) -> String {
    let b = brut.to_lowercase();
    if b.contains("délai") || b.contains("timed out") || b.contains("timeout") {
        format!("Le serveur {protocole} ne répond pas : vérifiez l'adresse du serveur et le port.")
    } else if b.contains("lookup")
        || b.contains("name or service")
        || b.contains("nodename")
        || b.contains("dns")
        || b.contains("nom de serveur invalide")
    {
        format!("Serveur {protocole} introuvable : vérifiez son adresse.")
    } else if b.contains("refused") || b.contains("unreachable") || b.contains("reset") {
        format!("Le serveur {protocole} refuse la connexion : vérifiez le port ({ports}).")
    } else if b.contains("tls")
        || b.contains("certificate")
        || b.contains("handshake")
        || b.contains("corrupt message")
        || b.contains("invalid")
    {
        format!(
            "Connexion sécurisée impossible avec le serveur {protocole} : vérifiez le port ({ports})."
        )
    } else {
        format!("Connexion {protocole} impossible : {brut}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causes_sans_redite() {
        let e = anyhow::anyhow!("received corrupt message of type InvalidContentType")
            .context("Connection error: received corrupt message of type InvalidContentType")
            .context("essai SMTP");
        assert_eq!(
            chaine_sans_redite(&anyhow::anyhow!(
                "Connection error: Connection error: refusée"
            )),
            "Connection error: refusée"
        );
        assert_eq!(
            chaine_sans_redite(&e),
            "essai SMTP : Connection error: received corrupt message of type InvalidContentType"
        );
    }
}
