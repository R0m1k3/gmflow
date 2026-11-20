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

async fn ouvrir(serveur: &str, port: u16, transport: Transport) -> Result<Box<dyn Flux>, ErreurImap> {
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
        client.login(identifiant, mot_de_passe).await.map_err(|(e, _)| match e {
            async_imap::error::Error::No(m) => ErreurImap::RefusAuth(m),
            autre => ErreurImap::from(autre),
        })
    };
    timeout(DELAI, travail)
        .await
        .map_err(|_| ErreurImap::Autre(format!("délai de {} s dépassé", DELAI.as_secs())))?
}

/// Noms de tous les dossiers du serveur (`LIST "" "*"`).
pub async fn dossiers(session: &mut Session) -> Result<Vec<String>, ErreurImap> {
    use futures::TryStreamExt;
    let noms: Vec<_> = timeout(DELAI, async {
        session.list(Some(""), Some("*")).await?.try_collect::<Vec<_>>().await
    })
    .await
    .map_err(|_| ErreurImap::Autre("délai dépassé pendant LIST".into()))??;
    let mut v: Vec<String> = noms.iter().map(|n| n.name().to_string()).collect();
    v.sort();
    Ok(v)
}
