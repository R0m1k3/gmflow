//! Découpage d'un `.eml` et empreinte de déduplication.
//!
//! Empreinte = SHA-256( objet ‖ "\n" ‖ h1 ‖ "\n" ‖ h2 … ), où l'objet est décodé (RFC 2047) et
//! sans espaces autour, et h1, h2… les SHA-256 (hexadécimal minuscule) du contenu décodé de
//! chaque partie de `attachments()`, pièces inline comprises, triés. Ces valeurs ne dépendent
//! pas de la boîte qui reçoit le mail (contrairement aux en-têtes `Received` ou au pied de page).

use mail_parser::{MessageParser, MessagePart};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Piece {
    pub rang: i32,
    pub nom: String,
    pub type_mime: String,
    pub taille: i64,
    pub sha256: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MailDecoupe {
    pub message_id: String,
    pub expediteur: String,
    pub destinataires: Vec<String>,
    pub objet: String,
    pub date: Option<time::OffsetDateTime>,
    pub corps_texte: String,
    pub pieces: Vec<Piece>,
    pub empreinte_contenu: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ErreurEml {
    #[error("message illisible")]
    Illisible,
    #[error("pièce jointe {0} absente du message")]
    PieceAbsente(i32),
}

pub fn sha256(octets: &[u8]) -> Vec<u8> {
    Sha256::digest(octets).to_vec()
}

fn type_mime(part: &MessagePart<'_>) -> String {
    match part.content_type() {
        Some(ct) => match ct.subtype() {
            Some(sous) => format!("{}/{}", ct.ctype(), sous).to_lowercase(),
            None => ct.ctype().to_lowercase(),
        },
        None => "application/octet-stream".into(),
    }
}

/// Type réel d'une pièce d'après ses premiers octets (PDF et images affichables), quel que
/// soit le type déclaré : beaucoup de messageries envoient leurs PDF en
/// `application/octet-stream` ou `application/x-pdf`. Jamais de HTML ni de SVG.
pub fn type_reel(octets: &[u8]) -> Option<&'static str> {
    let debut = &octets[..octets.len().min(1024)];
    if debut.windows(5).any(|w| w == b"%PDF-") {
        Some("application/pdf")
    } else if octets.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if octets.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if octets.starts_with(b"GIF8") {
        Some("image/gif")
    } else if octets.len() > 12 && &octets[0..4] == b"RIFF" && &octets[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn nom_par_defaut(rang: i32, mime: &str) -> String {
    let ext = mime.split('/').nth(1).unwrap_or("bin");
    format!("piece-{}.{}", rang + 1, ext)
}

pub fn decouper(brut: &[u8]) -> Result<MailDecoupe, ErreurEml> {
    let message = MessageParser::default()
        .parse(brut)
        .ok_or(ErreurEml::Illisible)?;

    let objet = message.subject().unwrap_or("").trim().to_string();
    let expediteur = message
        .from()
        .and_then(|l| {
            l.mailboxes()
                .next()
                .and_then(|m| m.address().map(str::to_string))
        })
        .unwrap_or_default();
    let mut destinataires: Vec<String> = Vec::new();
    for liste in [message.to(), message.cc()].into_iter().flatten() {
        destinataires.extend(
            liste
                .mailboxes()
                .filter_map(|m| m.address().map(str::to_string)),
        );
    }
    let date = message
        .date()
        .and_then(|d| time::OffsetDateTime::from_unix_timestamp(d.to_timestamp()).ok());
    let corps_texte = message
        .body_text(0)
        .map(|c| c.into_owned())
        .unwrap_or_default();

    let mut pieces = Vec::new();
    let mut empreintes: Vec<String> = Vec::new();
    for (rang, part) in message.attachments().enumerate() {
        let rang = rang as i32;
        let contenu = part.decoded();
        let empreinte = sha256(&contenu);
        empreintes.push(hex::encode(&empreinte));
        let mime = type_reel(&contenu)
            .map(str::to_string)
            .unwrap_or_else(|| type_mime(&part));
        let nom = part
            .attachment_name()
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| nom_par_defaut(rang, &mime));
        pieces.push(Piece {
            rang,
            nom,
            type_mime: mime,
            taille: contenu.len() as i64,
            sha256: empreinte,
        });
    }
    empreintes.sort();

    let mut h = Sha256::new();
    h.update(objet.as_bytes());
    for e in &empreintes {
        h.update(b"\n");
        h.update(e.as_bytes());
    }
    let empreinte_contenu = h.finalize().to_vec();

    let message_id = match message
        .message_id()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(m) => m.to_string(),
        None => format!("sha256:{}", hex::encode(sha256(brut))),
    };

    Ok(MailDecoupe {
        message_id,
        expediteur,
        destinataires,
        objet,
        date,
        corps_texte,
        pieces,
        empreinte_contenu,
    })
}

/// Contenu décodé de la pièce de rang `rang` (rang dans `attachments()`).
pub fn extraire_piece(brut: &[u8], rang: i32) -> Result<Vec<u8>, ErreurEml> {
    let message = MessageParser::default()
        .parse(brut)
        .ok_or(ErreurEml::Illisible)?;
    let part = message
        .attachments()
        .nth(usize::try_from(rang).map_err(|_| ErreurEml::PieceAbsente(rang))?)
        .ok_or(ErreurEml::PieceAbsente(rang))?;
    Ok(part.decoded().into_owned())
}

/// Le mail vient d'une liste de diffusion (en-tête `List-Unsubscribe`).
pub fn liste_diffusion(brut: &[u8]) -> bool {
    MessageParser::default()
        .parse_headers(brut)
        .map(|m| m.headers().contains("List-Unsubscribe"))
        .unwrap_or(false)
}
