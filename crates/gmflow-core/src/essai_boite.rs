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
pub struct ParamImap {
    pub serveur: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParamSmtp {
    pub serveur: String,
    pub port: u16,
    pub securite: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
pub struct ResultatSmtp {
    pub ok: bool,
    pub message: String,
    pub detail: Option<String>,
}

/// Contenu de `parametre.valeur` pour un essai.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Essai {
    /// `en_cours` puis `termine`.
    pub etat: String,
    pub cree_le: String,
    pub boite_id: Option<i64>,
    pub identifiant: String,
    pub imap: Option<ParamImap>,
    pub smtp: Option<ParamSmtp>,
    pub resultat_imap: Option<ResultatImap>,
    pub resultat_smtp: Option<ResultatSmtp>,
    pub termine_le: Option<String>,
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

/// Erreur SMTP dite en clair, et le détail brut.
pub fn message_smtp(e: &anyhow::Error) -> (String, String) {
    let brut = format!("{e:#}");
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
