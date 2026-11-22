//! Connexion IMAP : IMAPS (port 993, rustls + racines webpki). Le transport en clair n'existe
//! que pour les tests (fonction `test-utils`).

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::{ClientConfig, RootCertStore, pki_types::ServerName};

pub const DELAI: Duration = Duration::from_secs(15);

pub trait Flux: AsyncRead + AsyncWrite + Unpin + Send + Debug {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Debug> Flux for T {}

pub type Session = async_imap::Session<Box<dyn Flux>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tls,
    #[cfg(feature = "test-utils")]
    ClairTest,
}

impl Transport {
    /// Transport du worker : TLS, sauf build de test avec `GMFLOW_IMAP_CLAIR_TEST=1`.
    pub fn depuis_env() -> Self {
        #[cfg(feature = "test-utils")]
        if std::env::var("GMFLOW_IMAP_CLAIR_TEST").as_deref() == Ok("1") {
            return Transport::ClairTest;
        }
        Transport::Tls
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErreurImap {
    /// Le serveur a répondu NO au LOGIN : identifiant ou mot de passe refusé.
    #[error("authentification refusée : {0}")]
    RefusAuth(String),
    #[error("{0}")]
    Autre(String),
}

impl From<async_imap::error::Error> for ErreurImap {
    fn from(e: async_imap::error::Error) -> Self {
        ErreurImap::Autre(e.to_string())
    }
}

impl From<std::io::Error> for ErreurImap {
    fn from(e: std::io::Error) -> Self {
        ErreurImap::Autre(e.to_string())
    }
}

fn connecteur_tls() -> TlsConnector {
    let mut racines = RootCertStore::empty();
    racines.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("versions TLS par défaut")
    .with_root_certificates(racines)
    .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

async fn ouvrir(
    serveur: &str,
    port: u16,
    transport: Transport,
) -> Result<Box<dyn Flux>, ErreurImap> {
    let tcp = TcpStream::connect((serveur, port)).await?;
    match transport {
        Transport::Tls => {
            let nom = ServerName::try_from(serveur.to_string())
                .map_err(|_| ErreurImap::Autre(format!("nom de serveur invalide : {serveur}")))?;
            let tls = connecteur_tls().connect(nom, tcp).await?;
            Ok(Box::new(tls))
        }
        #[cfg(feature = "test-utils")]
        Transport::ClairTest => Ok(Box::new(tcp)),
    }
}

/// Ouvre la connexion et fait le LOGIN, en 15 s au plus.
pub async fn connecter(
    serveur: &str,
    port: u16,
    identifiant: &str,
    mot_de_passe: &str,
    transport: Transport,
) -> Result<Session, ErreurImap> {
    let travail = async {
        let flux = ouvrir(serveur, port, transport).await?;
        let mut client = async_imap::Client::new(flux);
        // Message d'accueil du serveur (« * OK ... »), à lire avant toute commande.
        client
            .read_response()
            .await?
            .ok_or_else(|| ErreurImap::Autre("connexion fermée avant l'accueil".into()))?;
        client
            .login(identifiant, mot_de_passe)
            .await
            .map_err(|(e, _)| match e {
                async_imap::error::Error::No(m) => ErreurImap::RefusAuth(texte_serveur(m)),
                autre => ErreurImap::from(autre),
            })
    };
    timeout(DELAI, travail)
        .await
        .map_err(|_| ErreurImap::Autre(format!("délai de {} s dépassé", DELAI.as_secs())))?
}

/// Texte de la réponse du serveur, sans l'enrobage d'async-imap
/// (« code: None, info: Some("[AUTHENTICATIONFAILED] …") » donne « [AUTHENTICATIONFAILED] … »).
fn texte_serveur(m: String) -> String {
    const DEBUT: &str = "info: Some(\"";
    if let Some(i) = m.find(DEBUT) {
        let reste = &m[i + DEBUT.len()..];
        if let Some(j) = reste.rfind("\")") {
            return reste[..j].replace("\\\"", "\"");
        }
    }
    m
}

/// Noms de tous les dossiers du serveur (`LIST "" "*"`).
pub async fn dossiers(session: &mut Session) -> Result<Vec<String>, ErreurImap> {
    use futures::TryStreamExt;
    let noms: Vec<_> = timeout(DELAI, async {
        session
            .list(Some(""), Some("*"))
            .await?
            .try_collect::<Vec<_>>()
            .await
    })
    .await
    .map_err(|_| ErreurImap::Autre("délai dépassé pendant LIST".into()))??;
    let mut v: Vec<String> = noms.iter().map(|n| n.name().to_string()).collect();
    v.sort();
    Ok(v)
}

/// Dossier vu par `LIST`, pour l'assistant d'ajout d'une boîte.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Dossier {
    /// Nom IMAP tel quel (UTF-7 modifié) : c'est lui qu'on enregistre.
    pub nom: String,
    /// Nom lisible (UTF-7 modifié décodé).
    pub affiche: String,
    /// Marqué `\Sent` par le serveur (RFC 6154).
    pub envoyes: bool,
    /// Non sélectionnable (`\Noselect`) : simple niveau de hiérarchie.
    pub non_selectionnable: bool,
}

/// Tous les dossiers du serveur avec leurs attributs, triés par nom.
pub async fn dossiers_detail(session: &mut Session) -> Result<Vec<Dossier>, ErreurImap> {
    use async_imap::types::NameAttribute;
    use futures::TryStreamExt;
    let noms: Vec<_> = timeout(DELAI, async {
        session
            .list(Some(""), Some("*"))
            .await?
            .try_collect::<Vec<_>>()
            .await
    })
    .await
    .map_err(|_| ErreurImap::Autre("délai dépassé pendant LIST".into()))??;
    let mut v: Vec<Dossier> = noms
        .iter()
        .map(|n| Dossier {
            nom: n.name().to_string(),
            affiche: decoder_utf7(n.name()),
            envoyes: n
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::Sent)),
            non_selectionnable: n
                .attributes()
                .iter()
                .any(|a| matches!(a, NameAttribute::NoSelect)),
        })
        .collect();
    v.sort_by(|a, b| a.nom.cmp(&b.nom));
    Ok(v)
}

/// Noms usuels du dossier des envoyés, du plus sûr au moins sûr (comparés en minuscules,
/// sur le nom décodé).
const NOMS_ENVOYES: &[&str] = &[
    "sent",
    "inbox.sent",
    "inbox/sent",
    "envoyés",
    "inbox.envoyés",
    "messages envoyés",
    "éléments envoyés",
    "sent messages",
    "sent items",
    "sent mail",
    "[gmail]/sent mail",
    "[gmail]/messages envoyés",
];

/// Dossier des envoyés repéré : attribut `\Sent`, sinon un nom usuel, sinon un nom qui
/// contient « sent » ou « envoy ».
pub fn dossier_envoyes(dossiers: &[Dossier]) -> Option<&Dossier> {
    let utiles = || dossiers.iter().filter(|d| !d.non_selectionnable);
    utiles().find(|d| d.envoyes).or_else(|| {
        NOMS_ENVOYES
            .iter()
            .find_map(|n| utiles().find(|d| d.affiche.to_lowercase() == *n))
            .or_else(|| {
                utiles().find(|d| {
                    let f = d
                        .affiche
                        .rsplit(['/', '.'])
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    f.contains("sent") || f.contains("envoy")
                })
            })
    })
}

/// Décode un nom de dossier en UTF-7 modifié (RFC 3501 §5.1.3) : `Envoy&AOk-s` → `Envoyés`.
/// Un nom mal formé est rendu tel quel.
pub fn decoder_utf7(nom: &str) -> String {
    use base64::Engine;
    let mut sortie = String::new();
    let mut reste = nom;
    while let Some(debut) = reste.find('&') {
        sortie.push_str(&reste[..debut]);
        let apres = &reste[debut + 1..];
        let Some(fin) = apres.find('-') else {
            return nom.to_string();
        };
        let code = &apres[..fin];
        if code.is_empty() {
            sortie.push('&');
        } else {
            let b64 = code.replace(',', "/");
            let Ok(octets) = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64) else {
                return nom.to_string();
            };
            if octets.len() % 2 != 0 {
                return nom.to_string();
            }
            let unites: Vec<u16> = octets
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            match String::from_utf16(&unites) {
                Ok(t) => sortie.push_str(&t),
                Err(_) => return nom.to_string(),
            }
        }
        reste = &apres[fin + 1..];
    }
    sortie.push_str(reste);
    sortie
}

/// Copie un message dans un dossier (APPEND), marqué comme lu : la réponse envoyée apparaît
/// dans le dossier des envoyés de la boîte.
pub async fn ajouter(session: &mut Session, dossier: &str, brut: &[u8]) -> Result<(), ErreurImap> {
    timeout(DELAI, session.append(dossier, Some("(\\Seen)"), None, brut))
        .await
        .map_err(|_| ErreurImap::Autre(format!("délai de {} s dépassé", DELAI.as_secs())))??;
    Ok(())
}

/// Les `n` derniers messages d'un dossier (bruts), sans les marquer comme lus.
pub async fn derniers_messages(
    session: &mut Session,
    dossier: &str,
    n: u32,
) -> Result<Vec<Vec<u8>>, ErreurImap> {
    use futures::TryStreamExt;
    let boite = timeout(DELAI, session.examine(dossier))
        .await
        .map_err(|_| ErreurImap::Autre("délai dépassé".into()))??;
    if boite.exists == 0 || n == 0 {
        return Ok(Vec::new());
    }
    let debut = boite.exists.saturating_sub(n - 1).max(1);
    let plage = format!("{debut}:{}", boite.exists);
    let messages: Vec<Vec<u8>> = timeout(DELAI, async {
        let fetches: Vec<_> = session
            .fetch(&plage, "BODY.PEEK[]")
            .await?
            .try_collect()
            .await?;
        Ok::<_, async_imap::error::Error>(
            fetches
                .iter()
                .filter_map(|f| f.body().map(<[u8]>::to_vec))
                .collect(),
        )
    })
    .await
    .map_err(|_| ErreurImap::Autre("délai dépassé".into()))??;
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_serveur_sans_enrobage() {
        assert_eq!(
            texte_serveur(
                r#"code: None, info: Some("[AUTHENTICATIONFAILED] Authentication failed.")"#.into()
            ),
            "[AUTHENTICATIONFAILED] Authentication failed."
        );
        assert_eq!(texte_serveur("refusé".into()), "refusé");
    }

    fn d(nom: &str, envoyes: bool) -> Dossier {
        Dossier {
            nom: nom.into(),
            affiche: decoder_utf7(nom),
            envoyes,
            non_selectionnable: false,
        }
    }

    #[test]
    fn utf7_modifie() {
        assert_eq!(decoder_utf7("Envoy&AOk-s"), "Envoyés");
        assert_eq!(
            decoder_utf7("&AMk-l&AOk-ments envoy&AOk-s"),
            "Éléments envoyés"
        );
        assert_eq!(decoder_utf7("A&-B"), "A&B");
        assert_eq!(decoder_utf7("INBOX"), "INBOX");
        assert_eq!(decoder_utf7("cass&é"), "cass&é");
    }

    #[test]
    fn envoyes_repere() {
        let v = vec![
            d("INBOX", false),
            d("Archives", false),
            d("Envoy&AOk-s", false),
        ];
        assert_eq!(dossier_envoyes(&v).unwrap().nom, "Envoy&AOk-s");
        let v = vec![d("INBOX", false), d("Sent", false), d("Boite", true)];
        assert_eq!(dossier_envoyes(&v).unwrap().nom, "Boite");
        let v = vec![d("INBOX", false), d("[Gmail]/Messages envoy&AOk-s", false)];
        assert!(dossier_envoyes(&v).is_some());
        assert!(dossier_envoyes(&[d("INBOX", false), d("Drafts", false)]).is_none());
    }
}
