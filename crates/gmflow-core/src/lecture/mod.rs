//! Lecture des pièces jointes (lot 2). Chaque fichier passe par le chemin le moins cher qui
//! marche : facture électronique (XML), couche texte d'un PDF, cellules d'un tableur, et
//! seulement en dernier Claude, qui lit le scan ou l'image lui-même.

pub mod decoupage;
pub mod extraction;
pub mod ocr;
pub mod pdf;
pub mod societe;
pub mod tableau_articles;
pub mod tableur;
pub mod tranches;
pub mod xml;

/// Fichiers plus gros : non lus (Claude refuse au-delà, et ce n'est pas une facture).
pub const FICHIER_MAX: usize = 25 * 1024 * 1024;
/// Caractères utiles (lettres, chiffres) en dessous desquels un PDF est traité comme un scan.
pub const TEXTE_UTILE_MIN: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voie {
    FacturX,
    Xml,
    PdfTexte,
    Tableur,
    Scan,
}

impl Voie {
    pub fn code(self) -> &'static str {
        match self {
            Voie::FacturX => "facturx",
            Voie::Xml => "xml",
            Voie::PdfTexte => "pdf_texte",
            Voie::Tableur => "tableur",
            Voie::Scan => "scan",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Aiguillage {
    /// Pas un document à lire (logo, signature, type non lu) : raison.
    Ignore(String),
    /// Facture électronique, lue sans Claude.
    Structure {
        voie: Voie,
        donnees: Box<xml::DonneesXml>,
        /// Couche texte du PDF quand il y en a une (pour la recherche et le contrôle).
        texte: Option<String>,
    },
    /// Texte lu par le code, à faire classer et extraire par Claude. `tronque` : le texte
    /// donné à Claude n'est pas complet (tableur coupé, texte plus long que `TEXTE_MAX`), des
    /// lignes articles peuvent manquer.
    Texte {
        voie: Voie,
        texte: String,
        tronque: bool,
    },
    /// Fichier à faire lire par Claude (scan, photo), sous ce nom d'extension.
    Scan {
        extension: &'static str,
        pages: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Genre {
    Pdf,
    Xml,
    Classeur,
    Csv,
    Image(&'static str),
    Autre,
}

fn extension(nom: &str) -> String {
    nom.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

fn genre(nom: &str, mime: &str, c: &[u8]) -> Genre {
    let ext = extension(nom);
    let debut = &c[..c.len().min(1024)];
    if debut.windows(5).any(|w| w == b"%PDF-") {
        return Genre::Pdf;
    }
    if c.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Genre::Image("jpg");
    }
    if c.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Genre::Image("png");
    }
    if c.len() > 12 && &c[0..4] == b"RIFF" && &c[8..12] == b"WEBP" {
        return Genre::Image("webp");
    }
    if c.starts_with(b"GIF8") {
        return Genre::Autre;
    }
    let zip = c.starts_with(b"PK\x03\x04");
    let ole = c.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]);
    if (zip && matches!(ext.as_str(), "xlsx" | "xlsm" | "ods"))
        || (ole && ext == "xls")
        || (zip && (mime.contains("spreadsheet") || mime.contains("excel")))
        || (ole && mime.contains("excel"))
    {
        return Genre::Classeur;
    }
    if ext == "csv" || mime == "text/csv" {
        return Genre::Csv;
    }
    let texte = String::from_utf8_lossy(debut);
    let t = texte.trim_start_matches('\u{feff}').trim_start();
    if (ext == "xml" || mime.ends_with("/xml") || t.starts_with("<?xml"))
        && (texte.contains("CrossIndustryInvoice")
            || texte.contains("<Invoice")
            || texte.contains(":Invoice")
            || texte.contains("CreditNote"))
    {
        return Genre::Xml;
    }
    Genre::Autre
}

/// Texte que Claude ne recevra pas en entier (`extraction::debut_et_fin`).
fn trop_long(t: &str) -> bool {
    t.chars().count() > extraction::TEXTE_MAX
}

fn texte_utile(t: &str) -> usize {
    t.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Décide comment lire une pièce jointe. Fonction bloquante (analyse des PDF) : à lancer dans
/// `spawn_blocking`. `_liste_diffusion` (en-tête `List-Unsubscribe`) ne sert plus depuis que les
/// images ne sont plus lues ; gardé pour les appelants.
/// Aperçu d'un tableur (xlsx, xls, ods, csv) en feuilles de cellules ; rien pour les autres
/// fichiers ou un classeur illisible.
pub fn apercu_tableur(nom: &str, mime: &str, contenu: &[u8]) -> Option<Vec<tableur::Feuille>> {
    match genre(nom, &mime.to_ascii_lowercase(), contenu) {
        Genre::Classeur => tableur::apercu_classeur(contenu),
        Genre::Csv => Some(vec![tableur::apercu_csv(contenu)]),
        _ => None,
    }
}

/// Feuilles visibles d'un tableur (xlsx, xls, ods, csv) en cellules, pour en lire les lignes
/// d'articles (`tableau_articles`) ; rien pour les autres fichiers ou un classeur illisible.
pub fn grilles_tableur(
    nom: &str,
    mime: &str,
    contenu: &[u8],
) -> Option<Vec<tableau_articles::Grille>> {
    match genre(nom, &mime.to_ascii_lowercase(), contenu) {
        Genre::Classeur => tableau_articles::grilles_classeur(contenu),
        Genre::Csv => Some(vec![tableau_articles::grille_csv(contenu)]),
        _ => None,
    }
}

pub fn aiguiller(nom: &str, mime: &str, contenu: &[u8], _liste_diffusion: bool) -> Aiguillage {
    if contenu.len() > FICHIER_MAX {
        return Aiguillage::Ignore(format!(
            "fichier de plus de {} Mo",
            FICHIER_MAX / 1024 / 1024
        ));
    }
    match genre(nom, mime, contenu) {
        Genre::Pdf => {
            if let Some(x) = pdf::xml_facturx(contenu)
                && let Some(donnees) = xml::lire(&x)
            {
                return Aiguillage::Structure {
                    voie: Voie::FacturX,
                    donnees: Box::new(donnees),
                    texte: pdf::texte(contenu),
                };
            }
            match pdf::texte(contenu) {
                Some(t) if texte_utile(&t) >= TEXTE_UTILE_MIN => Aiguillage::Texte {
                    voie: Voie::PdfTexte,
                    tronque: trop_long(&t),
                    texte: t,
                },
                _ => Aiguillage::Scan {
                    extension: "pdf",
                    pages: pdf::pages(contenu),
                },
            }
        }
        Genre::Xml => match xml::lire(contenu) {
            Some(donnees) => Aiguillage::Structure {
                voie: Voie::Xml,
                donnees: Box::new(donnees),
                texte: None,
            },
            None => Aiguillage::Ignore("XML non reconnu comme facture".into()),
        },
        Genre::Classeur => match tableur::texte_classeur(contenu) {
            Some((t, coupe)) if texte_utile(&t) > 0 => Aiguillage::Texte {
                voie: Voie::Tableur,
                tronque: coupe || trop_long(&t),
                texte: t,
            },
            _ => Aiguillage::Ignore("classeur vide ou illisible".into()),
        },
        Genre::Csv => {
            let (t, coupe) = tableur::texte_csv(contenu);
            if texte_utile(&t) > 0 {
                Aiguillage::Texte {
                    voie: Voie::Tableur,
                    tronque: coupe || trop_long(&t),
                    texte: t,
                }
            } else {
                Aiguillage::Ignore("CSV vide".into())
            }
        }
        // Photos, captures, logos : jamais des factures, avoirs ou BL utiles (les scans passent
        // par des PDF). Les images ne sont pas lues.
        Genre::Image(_) => {
            Aiguillage::Ignore("image : seuls les PDF, XML et tableurs sont lus".into())
        }
        Genre::Autre => Aiguillage::Ignore(format!(
            "type de fichier non lu ({})",
            if mime.is_empty() { "inconnu" } else { mime }
        )),
    }
}
