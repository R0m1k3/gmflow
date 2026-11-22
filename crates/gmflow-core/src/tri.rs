//! Tri des mails (lot 5) : règles sans IA d'abord (en-têtes, expéditeurs marqués, mail qui ne
//! fait que transmettre un document), Claude ensuite, avec un schéma JSON contrôlé.

use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const CATEGORIES: &[&str] = &[
    "question",
    "information",
    "document_seul",
    "publicite",
    "notification",
];
pub const PRIORITES: &[&str] = &["haute", "normale", "basse"];

/// Texte du mail envoyé à Claude pour le tri.
pub const CORPS_MAX: usize = 1500;
/// En dessous (lettres et chiffres, sans question), un mail avec pièce jointe ne fait que
/// transmettre le document.
pub const CORPS_DOCUMENT_SEUL: usize = 40;

pub const PROMPT_SYSTEME: &str = "Tu tries les mails reçus par des magasins. Tu réponds \
uniquement avec la sortie structurée demandée, selon le schéma. Catégories : « question » = \
l'expéditeur attend une réponse ou une action de notre part (question, demande, réclamation, \
litige, relance qui demande une réponse) ; « information » = message écrit par une personne qui \
n'attend pas de réponse ; « document_seul » = le mail sert seulement à transmettre un document \
(facture, avoir, bon de livraison, relevé) ; « publicite » = prospection, promotion, newsletter ; \
« notification » = message automatique (confirmation, avis d'expédition, alerte, accusé de \
réception). « question » est réservé à un message écrit par une personne, avec une vraie demande ou \
question explicite adressée à nous et qui attend une réponse : jamais un envoi de document, une \
confirmation, un remerciement, un message de masse ni un simple « pour information ». En cas \
de doute entre question et information, choisis information. « resume » : une \
ligne en français, 120 caractères au plus, qui dit ce que veut l'expéditeur. « priorite » : haute \
si c'est urgent ou bloquant (échéance proche, litige, réclamation), basse si rien ne presse, \
normale sinon.";

/// En-têtes utiles au tri et à la réponse.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entetes {
    pub liste_diffusion: bool,
    pub auto_submitted: Option<String>,
    pub precedence: Option<String>,
    /// Première adresse de Reply-To.
    pub reply_to: Option<String>,
    /// Message-ID sans chevrons.
    pub message_id: Option<String>,
    /// References, sans chevrons, dans l'ordre.
    pub references: Vec<String>,
}

fn brut_entete(m: &mail_parser::Message<'_>, nom: &str) -> Option<String> {
    m.headers()
        .get(nom)
        .map(|h| String::from_utf8_lossy(h.raw_value()).trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Lit les en-têtes d'un `.eml` (rien si le message est illisible).
pub fn entetes(brut: &[u8]) -> Entetes {
    let Some(m) = MessageParser::default().parse_headers(brut) else {
        return Entetes::default();
    };
    Entetes {
        liste_diffusion: m.headers().contains("List-Unsubscribe"),
        auto_submitted: brut_entete(&m, "Auto-Submitted"),
        precedence: brut_entete(&m, "Precedence"),
        reply_to: m
            .reply_to()
            .and_then(|l| l.first())
            .and_then(|b| b.address().map(str::to_string)),
        message_id: m.message_id().map(str::to_string),
        references: m
            .references()
            .map(|l| l.iter().map(str::to_string).collect())
            .unwrap_or_default(),
    }
}

/// Expéditeur (adresse) ou domaine toujours classé en publicité.
#[derive(Debug, Clone, PartialEq)]
pub enum Marque {
    Adresse(String),
    Domaine(String),
}

pub fn domaine(adresse: &str) -> Option<String> {
    adresse
        .rsplit_once('@')
        .map(|(_, d)| d.trim().trim_end_matches('>').to_lowercase())
        .filter(|d| !d.is_empty())
}

fn marquee(expediteur: &str, marques: &[Marque]) -> bool {
    let e = expediteur.trim().to_lowercase();
    let d = domaine(&e);
    marques.iter().any(|m| match m {
        Marque::Adresse(a) => a.to_lowercase() == e,
        Marque::Domaine(m) => {
            let m = m.to_lowercase();
            d.as_deref()
                .is_some_and(|d| d == m || d.ends_with(&format!(".{m}")))
        }
    })
}

/// Partie locale typique des expéditeurs automatiques.
fn automatique(expediteur: &str) -> bool {
    let local = expediteur
        .split('@')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .replace(['-', '_', '.'], "");
    const MOTS: &[&str] = &[
        "noreply",
        "nereply",
        "nepasrepondre",
        "nepasrépondre",
        "donotreply",
        "dontreply",
        "mailerdaemon",
        "postmaster",
        "notification",
        "notifications",
        "notif",
        "alert",
        "alerts",
        "alerte",
        "alertes",
    ];
    MOTS.iter().any(|m| local == *m || local.starts_with(m))
}

fn ligne_formule_fin(l: &str) -> bool {
    let l = l
        .trim()
        .trim_end_matches([',', '.', '!'])
        .trim()
        .to_lowercase();
    const FORMULES: &[&str] = &[
        "cordialement",
        "bien cordialement",
        "très cordialement",
        "bien à vous",
        "salutations",
        "sincères salutations",
        "meilleures salutations",
        "bonne journée",
        "bonne fin de journée",
        "bonne soirée",
        "merci",
        "merci d'avance",
        "best regards",
        "regards",
        "kind regards",
        "cdlt",
        "slts",
    ];
    FORMULES.contains(&l.as_str())
}

fn ligne_salutation(l: &str) -> bool {
    let l = l
        .trim()
        .trim_end_matches([',', '.', '!'])
        .trim()
        .to_lowercase();
    l.is_empty()
        || [
            "bonjour",
            "bonsoir",
            "hello",
            "madame, monsieur",
            "madame monsieur",
            "messieurs",
            "monsieur",
            "madame",
        ]
        .contains(&l.as_str())
        || l.starts_with("bonjour ")
}

fn debut_citation(lignes: &[&str], i: usize) -> bool {
    let l = lignes[i].trim();
    let bas = l.to_lowercase();
    if l.starts_with("-----") || bas.starts_with("________________") {
        return true;
    }
    if (bas.starts_with("le ") && (bas.ends_with("a écrit :") || bas.ends_with("a écrit:")))
        || (bas.starts_with("on ") && bas.ends_with("wrote:"))
    {
        return true;
    }
    // Bloc d'en-têtes d'Outlook : « De : … » suivi de « Envoyé : … » ou « Date : … ».
    if bas.starts_with("de :") || bas.starts_with("de:") || bas.starts_with("from:") {
        return lignes.iter().skip(i + 1).take(3).any(|s| {
            let s = s.trim().to_lowercase();
            s.starts_with("envoyé :")
                || s.starts_with("envoyé:")
                || s.starts_with("date :")
                || s.starts_with("date:")
                || s.starts_with("sent:")
        });
    }
    false
}

/// Le texte écrit par l'expéditeur : sans l'historique cité, sans la signature, sans les
/// formules d'appel et de politesse.
pub fn corps_utile(corps: &str) -> String {
    let lignes: Vec<&str> = corps.lines().collect();
    let mut garde: Vec<&str> = Vec::new();
    for i in 0..lignes.len() {
        let l = lignes[i];
        let t = l.trim();
        if t == "--" || l == "-- " || debut_citation(&lignes, i) || ligne_formule_fin(t) {
            break;
        }
        let bas = t.to_lowercase();
        if t.starts_with('>')
            || bas.starts_with("envoyé de mon")
            || bas.starts_with("sent from my")
            || bas.starts_with("envoyé depuis")
        {
            continue;
        }
        garde.push(l);
    }
    while garde.first().is_some_and(|l| ligne_salutation(l)) {
        garde.remove(0);
    }
    garde.join("\n").trim().to_string()
}

fn caracteres_utiles(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Ce que la règle sait du mail.
pub struct ContexteRegle<'a> {
    pub expediteur: &'a str,
    pub entetes: &'a Entetes,
    pub marques: &'a [Marque],
    /// Adresses des boîtes surveillées (mails envoyés par nous-mêmes).
    pub adresses_internes: &'a [String],
    pub corps: &'a str,
    /// Pièces jointes qui sont (ou peuvent être) des documents : pas les logos ignorés.
    pub pieces_utiles: usize,
}

/// Catégorie décidée sans IA, avec sa raison, quand les règles suffisent.
pub fn par_regle(c: &ContexteRegle<'_>) -> Option<(&'static str, String)> {
    let exp = c.expediteur.trim().to_lowercase();
    if marquee(&exp, c.marques) {
        return Some(("publicite", "expéditeur marqué comme publicité".into()));
    }
    if c.entetes.liste_diffusion {
        return Some((
            "publicite",
            "liste de diffusion (en-tête List-Unsubscribe)".into(),
        ));
    }
    let utile = corps_utile(c.corps);
    if c.pieces_utiles > 0
        && caracteres_utiles(&utile) < CORPS_DOCUMENT_SEUL
        && !utile.contains('?')
    {
        return Some((
            "document_seul",
            "pièce jointe avec un texte très court".into(),
        ));
    }
    if let Some(a) = &c.entetes.auto_submitted
        && !a.trim().eq_ignore_ascii_case("no")
    {
        return Some(("notification", format!("en-tête Auto-Submitted : {a}")));
    }
    if let Some(p) = &c.entetes.precedence {
        let p = p.trim().to_lowercase();
        if ["bulk", "list", "junk"].contains(&p.as_str()) {
            return Some(("notification", format!("en-tête Precedence : {p}")));
        }
    }
    if automatique(&exp) {
        return Some(("notification", "expéditeur automatique (no-reply)".into()));
    }
    if c.adresses_internes
        .iter()
        .any(|a| a.trim().eq_ignore_ascii_case(&exp))
    {
        return Some((
            "information",
            "mail envoyé depuis une boîte surveillée".into(),
        ));
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tri {
    pub categorie: String,
    pub resume: String,
    pub priorite: String,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "categorie": { "type": "string", "enum": CATEGORIES },
            "resume": { "type": "string" },
            "priorite": { "type": "string", "enum": PRIORITES }
        },
        "required": ["categorie", "resume", "priorite"],
        "additionalProperties": false
    })
}

pub fn tronquer(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Pièce jointe montrée à Claude : nom, et type du document s'il est déjà lu.
pub struct PieceVue<'a> {
    pub nom: &'a str,
    pub type_document: Option<&'a str>,
}

pub fn prompt(expediteur: &str, objet: &str, corps: &str, pieces: &[PieceVue<'_>]) -> String {
    let mut p = format!("De : {expediteur}\nObjet : {objet}\n");
    if !pieces.is_empty() {
        p.push_str("Pièces jointes :");
        for pj in pieces {
            match pj.type_document {
                Some(t) => p.push_str(&format!(" {} ({t}) ;", pj.nom)),
                None => p.push_str(&format!(" {} ;", pj.nom)),
            }
        }
        p.push('\n');
    }
    let utile = corps_utile(corps);
    let texte = if utile.is_empty() {
        corps.trim()
    } else {
        &utile
    };
    p.push_str("\nTexte du mail :\n<<<\n");
    p.push_str(tronquer(texte, CORPS_MAX));
    p.push_str("\n>>>\n");
    p
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ErreurTri {
    #[error("réponse sans sortie structurée")]
    Absente,
    #[error("sortie non conforme au schéma : {0}")]
    Schema(String),
}

/// Réponse JSON de Claude CLI -> tri contrôlé.
pub fn depuis_reponse(reponse: &Value) -> Result<Tri, ErreurTri> {
    let v = crate::claude::sortie_structuree(reponse).ok_or(ErreurTri::Absente)?;
    let mut t: Tri = serde_json::from_value(v).map_err(|e| ErreurTri::Schema(e.to_string()))?;
    if !CATEGORIES.contains(&t.categorie.as_str()) {
        return Err(ErreurTri::Schema(format!(
            "catégorie inconnue « {} »",
            t.categorie
        )));
    }
    if !PRIORITES.contains(&t.priorite.as_str()) {
        return Err(ErreurTri::Schema(format!(
            "priorité inconnue « {} »",
            t.priorite
        )));
    }
    t.resume = tronquer(t.resume.lines().next().unwrap_or("").trim(), 200).to_string();
    Ok(t)
}

/// Le texte d'un mail sans l'historique cité (la signature est gardée) : exemples de ton pour
/// les brouillons.
pub fn sans_citation(corps: &str) -> String {
    let lignes: Vec<&str> = corps.lines().collect();
    let mut garde: Vec<&str> = Vec::new();
    for i in 0..lignes.len() {
        if debut_citation(&lignes, i) {
            break;
        }
        if !lignes[i].trim_start().starts_with('>') {
            garde.push(lignes[i]);
        }
    }
    garde.join("\n").trim().to_string()
}
