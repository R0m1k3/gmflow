//! Plusieurs pièces dans un même PDF (L19) : un fichier qui regroupe 5 BL ou 3 factures est
//! découpé en sous-fichiers, lus chacun comme une pièce jointe (`piece_jointe.source_id`, pages
//! `page_debut`-`page_fin`).
//!
//! Le découpage est décidé par le code d'après le texte de chaque page (numéro de pièce qui
//! change, pagination « Page 1/2 » qui repart à 1) ; en cas de doute, par un petit appel à
//! Claude (`prompt`, `schema`). Prudence : dans le doute, on ne découpe pas (une facture de
//! 6 pages n'est jamais coupée en deux ; des CGV en dernière page ne sont pas une pièce).

use std::sync::LazyLock;

use lopdf::Document;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Pages au-delà desquelles un PDF n'est pas examiné (catalogue, relevé).
pub const PAGES_MAX: usize = 60;
/// Pièces au plus par fichier ; au-delà, on ne découpe pas.
pub const PIECES_MAX: usize = 20;

/// Pages d'une pièce (numérotées à partir de 1, bornes comprises) et son type présumé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plage {
    pub debut: usize,
    pub fin: usize,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Une seule pièce : lecture normale.
    Une,
    /// Plusieurs pièces, plages contiguës couvrant tout le fichier.
    Plusieurs(Vec<Plage>),
    /// Indices contradictoires : à demander à Claude.
    Doute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cle {
    genre: &'static str,
    numero: String,
}

static CLE_TYPE_PUIS_NUMERO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(facture|avoir|bon\s+de\s+livraison|bordereau\s+de\s+livraison|b\.?\s?l\.?)\s*(?:n\s?[°ºo]\.?|num[ée]ro|no\.?|#)\s*:?\s*([A-Z0-9][A-Z0-9/_.\-]{1,24})",
    )
    .expect("expression valide")
});

static CLE_NUMERO_PUIS_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:n\s?[°ºo]\.?|num[ée]ro)\s*(?:de\s+|du\s+|d')?(facture|avoir|bon\s+de\s+livraison|b\.?\s?l\.?)\s*:?\s*([A-Z0-9][A-Z0-9/_.\-]{1,24})",
    )
    .expect("expression valide")
});

static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{1,4}[/.\-]\d{1,2}[/.\-]\d{1,4}$").expect("expression valide")
});

static PAGINATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bpage\s*:?\s*(\d{1,3})\s*(?:/|sur|of)\s*(\d{1,3})\b")
        .expect("expression valide")
});

fn genre(mot: &str) -> &'static str {
    let m = mot.to_lowercase();
    if m.starts_with("facture") {
        "facture"
    } else if m.starts_with("avoir") {
        "avoir"
    } else {
        "bl"
    }
}

/// Numéro de la pièce d'une page : la première mention « Facture n° X », « BL : X »,
/// « N° de facture X »… (en-tête), avec au moins un chiffre.
fn cle_page(texte: &str) -> Option<Cle> {
    let mut trouvees: Vec<(usize, Cle)> = Vec::new();
    for re in [&*CLE_TYPE_PUIS_NUMERO, &*CLE_NUMERO_PUIS_TYPE] {
        for c in re.captures_iter(texte) {
            let numero = c[2].trim_end_matches(['.', '-', '/']).to_uppercase();
            if numero.chars().any(|c| c.is_ascii_digit()) && !DATE.is_match(&numero) {
                let pos = c.get(0).map_or(0, |m| m.start());
                trouvees.push((
                    pos,
                    Cle {
                        genre: genre(&c[1]),
                        numero,
                    },
                ));
                break;
            }
        }
    }
    trouvees.into_iter().min_by_key(|(p, _)| *p).map(|(_, c)| c)
}

/// Pagination imprimée sur la page (« Page 2/3 », « page 1 sur 4 »).
fn pagination(texte: &str) -> Option<(u32, u32)> {
    let c = PAGINATION.captures(texte)?;
    let (n, total): (u32, u32) = (c[1].parse().ok()?, c[2].parse().ok()?);
    (n >= 1 && n <= total).then_some((n, total))
}

/// Numéro de pièce et pagination lus sur une page.
type InfoPage = (Option<Cle>, Option<(u32, u32)>);

/// Décision du code d'après le texte de chaque page.
pub fn analyser(textes: &[String]) -> Verdict {
    let n = textes.len();
    if !(2..=PAGES_MAX).contains(&n) {
        return Verdict::Une;
    }
    let infos: Vec<InfoPage> = textes
        .iter()
        .map(|t| (cle_page(t), pagination(t)))
        .collect();
    let mut debuts: Vec<(usize, Cle)> = Vec::new();
    let mut cle_segment = infos[0].0.clone();
    if let Some(c) = &cle_segment {
        debuts.push((0, c.clone()));
    }
    let mut doute = false;
    for i in 1..n {
        let (cle, pag) = &infos[i];
        let repart = matches!(pag, Some((1, _))) && infos[i - 1].1.is_some();
        let suite = matches!(pag, Some((k, _)) if *k > 1);
        match cle {
            Some(c) if cle_segment.as_ref() != Some(c) => {
                if suite {
                    // « Page 2/3 » avec un autre numéro : une pièce citée, ou une erreur.
                    doute = true;
                } else if let Some(s) = &cle_segment {
                    if s.genre == c.genre || repart {
                        debuts.push((i, c.clone()));
                        cle_segment = Some(c.clone());
                    } else {
                        // Un BL cité dans une facture sans pagination : à voir.
                        doute = true;
                    }
                } else {
                    // Première pièce sans numéro reconnu.
                    doute = true;
                }
            }
            // Même numéro : même pièce, même si la pagination repart (duplicata).
            Some(_) => {}
            None => {
                if repart {
                    // Nouvelle pagination sans numéro reconnu.
                    doute = true;
                }
            }
        }
    }
    if doute {
        return Verdict::Doute;
    }
    if debuts.len() < 2 {
        return Verdict::Une;
    }
    // Un même numéro sur deux pièces non voisines : incohérent.
    for (k, (_, a)) in debuts.iter().enumerate() {
        if debuts[k + 1..].iter().any(|(_, b)| b == a) {
            return Verdict::Doute;
        }
    }
    if debuts.len() > PIECES_MAX {
        return Verdict::Une;
    }
    let plages = debuts
        .iter()
        .enumerate()
        .map(|(k, (d, c))| Plage {
            debut: d + 1,
            fin: debuts.get(k + 1).map_or(n, |(f, _)| *f),
            r#type: Some(c.genre.to_string()),
        })
        .collect();
    Verdict::Plusieurs(plages)
}

/// Plages valides : contiguës, de la page 1 à la dernière, au plus `PIECES_MAX`. Une seule
/// plage (ou des plages invalides) : `Une`.
pub fn verifier(plages: Vec<Plage>, pages: usize) -> Verdict {
    if plages.len() < 2 || plages.len() > PIECES_MAX {
        return Verdict::Une;
    }
    let mut attendu = 1;
    for p in &plages {
        if p.debut != attendu || p.fin < p.debut || p.fin > pages {
            return Verdict::Une;
        }
        attendu = p.fin + 1;
    }
    if attendu != pages + 1 {
        return Verdict::Une;
    }
    Verdict::Plusieurs(plages)
}

fn extrait(t: &str) -> String {
    let t = t.trim();
    let n = t.chars().count();
    if n <= 700 {
        return t.to_string();
    }
    let debut: String = t.chars().take(500).collect();
    let fin: String = t.chars().skip(n - 200).collect();
    format!("{debut}\n[...]\n{fin}")
}

pub const PROMPT_SYSTEME: &str = "Tu découpes un fichier PDF reçu par un magasin en pièces \
distinctes (factures, avoirs, bons de livraison). Tu réponds uniquement avec la sortie \
structurée demandée. Dans le doute, une seule pièce.";

const CONSIGNE: &str = "Une pièce a son propre numéro (facture, avoir ou bon de livraison) et \
peut tenir sur plusieurs pages (« Page 1/3 », « report », « à reporter ») : elle reste alors une \
seule pièce. Une facture qui cite des bons de livraison reste une seule pièce. Des conditions \
générales de vente, une lettre, un relevé ou une page blanche ne sont pas des pièces : \
rattache-les à la pièce qui précède (à la suivante pour une page de garde). Donne pieces = les \
pièces dans l'ordre, chacune avec page_debut et page_fin (pages comprises, à partir de 1, \
contiguës, de la première à la dernière page) et son type ; une seule pièce de 1 à la \
dernière page si le fichier n'en contient qu'une ou en cas de doute.\n";

/// Prompt de découpage d'après le texte des pages (début et fin de chacune).
pub fn prompt_textes(textes: &[String]) -> String {
    let mut p = format!(
        "Voici le texte de chacune des {} pages d'un fichier PDF (début et fin de chaque page).\n\n",
        textes.len()
    );
    for (i, t) in textes.iter().enumerate() {
        p.push_str(&format!("=== Page {} ===\n{}\n\n", i + 1, extrait(t)));
    }
    p.push_str(CONSIGNE);
    p
}

/// Prompt de découpage d'un scan sans texte : Claude lit le fichier.
pub fn prompt_fichier(fichier: &str, pages: usize) -> String {
    format!(
        "Le fichier {fichier} du dossier courant est un PDF de {pages} pages : lis-le avec \
         l'outil Read (au plus 10 pages à la fois, paramètre pages), en regardant surtout l'en-tête \
         de chaque page (type et numéro de la pièce, pagination).\n{CONSIGNE}"
    )
}

/// Schéma JSON de la réponse.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pieces": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "page_debut": { "type": "integer", "minimum": 1 },
                        "page_fin": { "type": "integer", "minimum": 1 },
                        "type": { "type": "string", "enum": ["facture", "avoir", "bl", "autre"] }
                    },
                    "required": ["page_debut", "page_fin", "type"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["pieces"],
        "additionalProperties": false
    })
}

/// Décision lue dans la réponse de Claude (sortie structurée), vérifiée.
pub fn depuis_reponse(valeur: &Value, pages: usize) -> Verdict {
    let Some(pieces) = valeur.get("pieces").and_then(Value::as_array) else {
        return Verdict::Une;
    };
    let mut plages = Vec::new();
    for p in pieces {
        let (Some(d), Some(f)) = (
            p.get("page_debut").and_then(Value::as_u64),
            p.get("page_fin").and_then(Value::as_u64),
        ) else {
            return Verdict::Une;
        };
        plages.push(Plage {
            debut: d as usize,
            fin: f as usize,
            r#type: p.get("type").and_then(Value::as_str).map(str::to_string),
        });
    }
    verifier(plages, pages)
}

/// Sous-fichier des pages `debut` à `fin` (à partir de 1), sans les fichiers embarqués (un
/// XML Factur-X décrit le fichier entier). Rien si le PDF est illisible ou les pages hors du
/// fichier.
pub fn extraire_pages(pdf: &[u8], debut: usize, fin: usize) -> Option<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).ok()?;
    let n = doc.get_pages().len();
    if debut < 1 || fin > n || debut > fin {
        return None;
    }
    let retirees: Vec<u32> = (1..=n)
        .filter(|p| *p < debut || *p > fin)
        .map(|p| p as u32)
        .collect();
    doc.delete_pages(&retirees);
    if let Ok(cat) = doc.catalog_mut() {
        cat.remove(b"AF");
        if let Ok(noms) = cat.get_mut(b"Names").and_then(|n| n.as_dict_mut()) {
            noms.remove(b"EmbeddedFiles");
        }
    }
    doc.prune_objects();
    if doc.get_pages().len() != fin - debut + 1 {
        return None;
    }
    let mut sortie = Vec::new();
    doc.save_to(&mut sortie).ok()?;
    Some(sortie)
}

/// Contenu d'une pièce jointe : la pièce du mail, ou les pages d'une pièce découpée.
pub fn contenu_piece(
    brut_eml: &[u8],
    rang: i32,
    pages: Option<(i32, i32)>,
) -> anyhow::Result<Vec<u8>> {
    let piece = crate::eml::extraire_piece(brut_eml, rang)?;
    match pages {
        None => Ok(piece),
        Some((d, f)) => extraire_pages(
            &piece,
            usize::try_from(d).unwrap_or(0),
            usize::try_from(f).unwrap_or(0),
        )
        .ok_or_else(|| anyhow::anyhow!("pages {d}-{f} introuvables dans la pièce")),
    }
}

/// Nom d'une pièce découpée : « bl.pdf » → « bl (p. 3-4).pdf ».
pub fn nom_partie(nom: &str, p: &Plage) -> String {
    let pages = if p.debut == p.fin {
        format!("p. {}", p.debut)
    } else {
        format!("p. {}-{}", p.debut, p.fin)
    };
    match nom.rsplit_once('.') {
        Some((base, ext)) => format!("{base} ({pages}).{ext}"),
        None => format!("{nom} ({pages})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn numeros_et_pagination() {
        let c = cle_page("DURAND SAS\nBON DE LIVRAISON N° BL-4512 du 12/11").unwrap();
        assert_eq!((c.genre, c.numero.as_str()), ("bl", "BL-4512"));
        let c = cle_page("N° de facture : F2026-77\nFacture n° 9").unwrap();
        assert_eq!((c.genre, c.numero.as_str()), ("facture", "F2026-77"));
        assert_eq!(
            cle_page("Conditions générales de vente, facture payable à 30 jours"),
            None
        );
        assert_eq!(
            cle_page("Date facture : 12/11/2026, facture non payée"),
            None
        );
        assert_eq!(cle_page("Facture n° 12/11/2026"), None);
        assert_eq!(pagination("Page 2/3"), Some((2, 3)));
        assert_eq!(pagination("page 1 sur 4"), Some((1, 4)));
    }

    #[test]
    fn trois_bl() {
        let t = pages(&[
            "BL n° 101\nPage 1/1",
            "BL n° 102\nPage 1/2",
            "BL n° 102\nPage 2/2",
            "BL n° 103",
        ]);
        let Verdict::Plusieurs(p) = analyser(&t) else {
            panic!("{:?}", analyser(&t))
        };
        assert_eq!(
            p.iter().map(|p| (p.debut, p.fin)).collect::<Vec<_>>(),
            [(1, 1), (2, 3), (4, 4)]
        );
    }

    #[test]
    fn facture_de_plusieurs_pages_jamais_coupee() {
        let t = pages(&[
            "FACTURE N° F-1\nPage 1/4",
            "FACTURE N° F-1\nPage 2/4",
            "Page 3/4\nreport",
            "FACTURE N° F-1\nPage 4/4\nTotal TTC",
            "Conditions générales de vente",
        ]);
        assert_eq!(analyser(&t), Verdict::Une);
        // Une page qui cite un BL sans pagination : doute, pas de découpage par le code.
        let t = pages(&["FACTURE N° F-1", "BL n° 4512 du 10/11 : 3 articles"]);
        assert_eq!(analyser(&t), Verdict::Doute);
        // Page 2/2 qui cite un autre numéro : doute.
        let t = pages(&["FACTURE N° F-1 Page 1/2", "Facture n° F-0 rappel Page 2/2"]);
        assert_eq!(analyser(&t), Verdict::Doute);
    }

    #[test]
    fn plages_de_claude_verifiees() {
        let p = |d, f| Plage {
            debut: d,
            fin: f,
            r#type: None,
        };
        assert!(matches!(
            verifier(vec![p(1, 2), p(3, 3)], 3),
            Verdict::Plusieurs(_)
        ));
        assert_eq!(verifier(vec![p(1, 2), p(4, 4)], 4), Verdict::Une);
        assert_eq!(verifier(vec![p(1, 3)], 3), Verdict::Une);
        assert_eq!(verifier(vec![p(1, 1), p(2, 2)], 3), Verdict::Une);
        let v = json!({ "pieces": [
            { "page_debut": 1, "page_fin": 1, "type": "bl" },
            { "page_debut": 2, "page_fin": 2, "type": "bl" }
        ]});
        assert!(matches!(depuis_reponse(&v, 2), Verdict::Plusieurs(_)));
        assert_eq!(nom_partie("bl.pdf", &p(3, 4)), "bl (p. 3-4).pdf");
    }
}
