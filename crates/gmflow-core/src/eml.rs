//! Découpage d'un `.eml` et empreinte de déduplication.
//!
//! Empreinte = SHA-256( objet ‖ "\n" ‖ h1 ‖ "\n" ‖ h2 … ), où l'objet est décodé (RFC 2047) et
//! sans espaces autour, et h1, h2… les SHA-256 (hexadécimal minuscule) du contenu décodé de
//! chaque partie de `attachments()`, pièces inline comprises, triés. Ces valeurs ne dépendent
//! pas de la boîte qui reçoit le mail (contrairement aux en-têtes `Received` ou au pied de page).

use mail_parser::{MessageParser, MessagePart, PartKind};
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

/// Feuille de style par défaut de l'affichage HTML : police lisible et images contenues dans la
/// largeur. `:where` garde une priorité nulle, le style du mail l'emporte toujours.
const STYLE_HTML: &str = ":where(html){font-family:-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;font-size:15px;line-height:1.45;color:#1C2520;word-wrap:break-word}:where(body){margin:0;padding:16px}img{max-width:100%}pre{white-space:pre-wrap}blockquote{margin-left:0;padding-left:12px;border-left:2px solid #d5dbd6}";

/// Le message a un vrai corps HTML (partie `text/html`), et pas seulement du texte.
pub fn a_corps_html(brut: &[u8]) -> bool {
    MessageParser::default()
        .parse(brut)
        .is_some_and(|m| m.html_body().any(|p| matches!(p.kind(), PartKind::Html)))
}

/// Page HTML complète du corps d'origine, comme un lecteur de mails l'afficherait : parties
/// `text/html` du corps dans l'ordre, images intégrées (`cid:`) remplacées par des URI `data:`
/// (images reconnues seulement), liens ouverts dans un nouvel onglet. `None` sans partie HTML.
/// Le HTML n'est pas nettoyé : la page doit être servie avec une CSP sans script et affichée
/// dans un `iframe` `sandbox`.
pub fn corps_html(brut: &[u8]) -> Result<Option<String>, ErreurEml> {
    use base64::Engine;
    let message = MessageParser::default()
        .parse(brut)
        .ok_or(ErreurEml::Illisible)?;
    let mut html = String::new();
    for part in message.html_body() {
        if matches!(part.kind(), PartKind::Html)
            && let Some(t) = part.text()
        {
            html.push_str(&t);
        }
    }
    if html.trim().is_empty() {
        return Ok(None);
    }
    let mut images = std::collections::HashMap::new();
    for part in message.parts() {
        let Some(cid) = part.content_id() else {
            continue;
        };
        let cid = cid
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim();
        if cid.is_empty() {
            continue;
        }
        let contenu = part.decoded();
        if let Some(t) = type_reel(&contenu).filter(|t| t.starts_with("image/")) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&contenu);
            images.insert(cid.to_ascii_lowercase(), format!("data:{t};base64,{b64}"));
        }
    }
    let html = remplacer_cid(&html, &images);
    Ok(Some(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"referrer\" content=\"no-referrer\"><base target=\"_blank\"><style>{STYLE_HTML}</style></head><body>{html}</body></html>"
    )))
}

/// Remplace chaque `cid:xxx` (insensible à la casse, `%xx` décodés) par son URI `data:` ;
/// une référence inconnue est laissée telle quelle (l'image ne s'affiche pas).
fn remplacer_cid(html: &str, images: &std::collections::HashMap<String, String>) -> String {
    if images.is_empty() {
        return html.to_string();
    }
    let bas = html.to_ascii_lowercase();
    let mut sortie = String::with_capacity(html.len());
    let mut i = 0;
    while let Some(pos) = bas[i..].find("cid:") {
        let debut = i + pos;
        let fin = html[debut + 4..]
            .find(|c: char| matches!(c, '"' | '\'' | ')' | '>' | '<') || c.is_whitespace())
            .map_or(html.len(), |n| debut + 4 + n);
        let cle = decoder_pourcent(&html[debut + 4..fin]).to_ascii_lowercase();
        sortie.push_str(&html[i..debut]);
        match images.get(&cle) {
            Some(uri) => sortie.push_str(uri),
            None => sortie.push_str(&html[debut..fin]),
        }
        i = fin;
    }
    sortie.push_str(&html[i..]);
    sortie
}

fn decoder_pourcent(s: &str) -> String {
    let o = s.as_bytes();
    let mut v = Vec::with_capacity(o.len());
    let mut i = 0;
    while i < o.len() {
        if o[i] == b'%'
            && i + 2 < o.len()
            && o[i + 1].is_ascii_hexdigit()
            && o[i + 2].is_ascii_hexdigit()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            v.push(b);
            i += 3;
        } else {
            v.push(o[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&v).into_owned()
}
