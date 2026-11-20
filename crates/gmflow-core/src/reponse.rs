//! Réponse à un mail : objet « Re: », fil de discussion (In-Reply-To, References), citation du
//! message d'origine. L'envoi ne part que sur un clic dans l'interface (tâche `EnvoyerReponse`).

use lettre::Message;
use lettre::message::{Mailbox, SinglePart};

/// Citation du message d'origine : au plus ce nombre de caractères.
pub const CITATION_MAX: usize = 20_000;

/// Le mail auquel on répond.
pub struct Original<'a> {
    /// Message-ID sans chevrons (ou `sha256:…` quand le mail n'en avait pas).
    pub message_id: &'a str,
    /// References du mail d'origine, sans chevrons.
    pub references: &'a [String],
    pub expediteur: &'a str,
    pub reply_to: Option<&'a str>,
    pub objet: &'a str,
    pub date: Option<time::OffsetDateTime>,
    pub corps: &'a str,
}

/// « Re: objet », sans doubler un « Re: » déjà présent.
pub fn objet_reponse(objet: &str) -> String {
    let o = objet.trim();
    let bas = o.to_lowercase();
    if bas.starts_with("re:") || bas.starts_with("re :") {
        o.to_string()
    } else if o.is_empty() {
        "Re:".into()
    } else {
        format!("Re: {o}")
    }
}

fn identifiant_valide(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with("sha256:")
        && id.contains('@')
        && !id.contains(['<', '>', ' '])
}

/// References de la réponse : celles du mail d'origine puis son Message-ID, sans doublon, en
/// gardant les 20 derniers.
pub fn references(origine: &[String], message_id: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for r in origine
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(message_id))
    {
        let r = r.trim().trim_start_matches('<').trim_end_matches('>');
        if identifiant_valide(r) && !v.iter().any(|x| x == r) {
            v.push(r.to_string());
        }
    }
    let n = v.len().saturating_sub(20);
    v.split_off(n)
}

/// Adresse à qui répondre : Reply-To s'il y en a un, sinon l'expéditeur.
pub fn destinataire<'a>(o: &Original<'a>) -> &'a str {
    o.reply_to
        .filter(|r| r.contains('@'))
        .unwrap_or(o.expediteur)
}

/// Le message d'origine cité sous la réponse.
pub fn citation(o: &Original<'_>) -> String {
    let date = o
        .date
        .map(|d| {
            format!(
                "Le {:02}/{:02}/{}, ",
                d.day(),
                u8::from(d.month()),
                d.year()
            )
        })
        .unwrap_or_default();
    let intro = if date.is_empty() {
        format!("{} a écrit :", o.expediteur)
    } else {
        format!("{date}{} a écrit :", o.expediteur)
    };
    let corps = crate::tri::tronquer(o.corps.trim_end(), CITATION_MAX);
    let mut s = intro;
    for l in corps.lines() {
        s.push('\n');
        if l.is_empty() {
            s.push('>');
        } else {
            s.push_str("> ");
            s.push_str(l);
        }
    }
    s
}

/// Corps complet : la réponse, puis la citation.
pub fn corps(texte: &str, o: &Original<'_>) -> String {
    format!("{}\n\n{}\n", texte.trim_end(), citation(o))
}

pub struct Reponse {
    pub message: Message,
    /// Message-ID de la réponse, avec chevrons.
    pub message_id: String,
    /// Message au format RFC 5322 (copie dans le dossier des envoyés).
    pub brut: Vec<u8>,
}

/// Construit la réponse envoyée depuis `de` (adresse de la boîte d'origine).
pub fn construire(
    de: &str,
    nom: Option<&str>,
    o: &Original<'_>,
    texte: &str,
) -> anyhow::Result<Reponse> {
    let de: Mailbox = Mailbox::new(
        nom.map(str::to_string).filter(|n| !n.trim().is_empty()),
        de.parse()
            .map_err(|_| anyhow::anyhow!("adresse d'expédition « {de} » invalide"))?,
    );
    let a = destinataire(o);
    let a: Mailbox = a
        .parse()
        .map_err(|_| anyhow::anyhow!("adresse du destinataire « {a} » invalide"))?;
    let domaine = de.email.domain().to_string();
    let message_id = format!("<{}@{domaine}>", uuid::Uuid::new_v4());
    let mut b = Message::builder()
        .from(de)
        .to(a)
        .subject(objet_reponse(o.objet))
        .message_id(Some(message_id.clone()))
        .date_now();
    let msgid = o
        .message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    if identifiant_valide(msgid) {
        b = b.in_reply_to(format!("<{msgid}>"));
    }
    let refs = references(o.references, o.message_id);
    if !refs.is_empty() {
        b = b.references(
            refs.iter()
                .map(|r| format!("<{r}>"))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let message = b.singlepart(SinglePart::plain(corps(texte, o)))?;
    let brut = message.formatted();
    Ok(Reponse {
        message,
        message_id,
        brut,
    })
}
