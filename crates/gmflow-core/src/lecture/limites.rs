//! Bornes de l'analyse des pièces jointes (constat C9). Les pièces viennent de n'importe quel
//! expéditeur : un fichier piégé (bombe zip, PDF à des millions d'objets ou à flux qui se
//! décompressent en gigaoctets, tableur aux cellules éparpillées sur un million de lignes)
//! saturerait la mémoire ou le processeur de l'app ou du worker. Avant toute analyse (lopdf,
//! pdf-extract, calamine), [`controler`] mesure le fichier sans le charger en entier : un
//! dépassement rend la pièce « trop volumineuse pour être lue automatiquement » (elle reste
//! téléchargeable). Les bornes visent les fichiers anormaux : un BL de 40 pages passe.
//!
//! Réglables par variables d'environnement (lues une fois) : `GMFLOW_LECTURE_FICHIER_MAX_MO`,
//! `GMFLOW_LECTURE_DECOMPRESSE_MAX_MO`, `GMFLOW_LECTURE_PAGES_MAX`,
//! `GMFLOW_LECTURE_CELLULES_MAX`, `GMFLOW_LECTURE_DUREE_MAX_S`.

use std::io::{BufReader, Cursor, Read, Write};
use std::sync::OnceLock;
use std::time::Duration;

use super::Genre;

/// Taille du fichier (25 Mo, ce que Claude accepte au plus).
pub const FICHIER_MAX: usize = 25 * 1024 * 1024;
/// Octets décompressés au total : entrées d'un zip (xlsx, ods), flux d'un PDF hors images.
pub const DECOMPRESSE_MAX: u64 = 300 * 1024 * 1024;
/// Entrées d'un zip (un xlsx ordinaire en a une vingtaine).
pub const ENTREES_ZIP_MAX: usize = 2_000;
/// Cellules d'une feuille de classeur, comptées de A1 à la dernière cellule remplie (calamine
/// réserve ce rectangle en mémoire).
pub const CELLULES_MAX: u64 = 2_000_000;
/// Pages d'un PDF.
pub const PAGES_MAX: usize = 1_000;
/// Objets d'un PDF.
pub const OBJETS_PDF_MAX: usize = 500_000;
/// Durée de l'analyse d'une pièce (couche texte, tableur) avant abandon.
pub const DUREE_MAX: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    pub fichier_max: usize,
    pub decompresse_max: u64,
    pub entrees_zip_max: usize,
    pub cellules_max: u64,
    pub pages_max: usize,
    pub objets_pdf_max: usize,
    pub duree_max: Duration,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            fichier_max: FICHIER_MAX,
            decompresse_max: DECOMPRESSE_MAX,
            entrees_zip_max: ENTREES_ZIP_MAX,
            cellules_max: CELLULES_MAX,
            pages_max: PAGES_MAX,
            objets_pdf_max: OBJETS_PDF_MAX,
            duree_max: DUREE_MAX,
        }
    }
}

fn nombre_env(nom: &str) -> Option<u64> {
    std::env::var(nom)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
}

impl Limites {
    pub fn depuis_env() -> Self {
        let d = Self::default();
        const MO: u64 = 1024 * 1024;
        Self {
            fichier_max: nombre_env("GMFLOW_LECTURE_FICHIER_MAX_MO")
                .map_or(d.fichier_max, |n| (n * MO) as usize),
            decompresse_max: nombre_env("GMFLOW_LECTURE_DECOMPRESSE_MAX_MO")
                .map_or(d.decompresse_max, |n| n * MO),
            entrees_zip_max: d.entrees_zip_max,
            cellules_max: nombre_env("GMFLOW_LECTURE_CELLULES_MAX").unwrap_or(d.cellules_max),
            pages_max: nombre_env("GMFLOW_LECTURE_PAGES_MAX").map_or(d.pages_max, |n| n as usize),
            objets_pdf_max: d.objets_pdf_max,
            duree_max: nombre_env("GMFLOW_LECTURE_DUREE_MAX_S")
                .map_or(d.duree_max, Duration::from_secs),
        }
    }
}

/// Bornes en vigueur (variables d'environnement lues au premier appel).
pub fn limites() -> &'static Limites {
    static L: OnceLock<Limites> = OnceLock::new();
    L.get_or_init(Limites::depuis_env)
}

/// Ce qui a dépassé la borne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Depassement {
    Taille { octets: usize, max: usize },
    Decompresse { max: u64 },
    EntreesZip { entrees: usize },
    Cellules { cellules: u64 },
    Pages { pages: usize, max: usize },
    Objets { objets: usize },
    Duree { secondes: u64 },
}

const MO: f64 = 1024.0 * 1024.0;

impl Depassement {
    /// Code stable (gardé dans `document.confiance.trop_volumineux`).
    pub fn code(&self) -> &'static str {
        match self {
            Depassement::Taille { .. } => "taille",
            Depassement::Decompresse { .. } => "decompresse",
            Depassement::EntreesZip { .. } => "entrees_zip",
            Depassement::Cellules { .. } => "cellules",
            Depassement::Pages { .. } => "pages",
            Depassement::Objets { .. } => "objets",
            Depassement::Duree { .. } => "duree",
        }
    }

    fn detail(&self) -> String {
        match self {
            Depassement::Taille { octets, max } => format!(
                "{:.0} Mo, au-delà de {:.0} Mo",
                *octets as f64 / MO,
                *max as f64 / MO
            ),
            Depassement::Decompresse { max } => format!(
                "son contenu dépasse {:.0} Mo une fois décompressé",
                *max as f64 / MO
            ),
            Depassement::EntreesZip { entrees } => format!("{entrees} fichiers à l'intérieur"),
            Depassement::Cellules { cellules } => {
                format!("une feuille de {cellules} cellules")
            }
            Depassement::Pages { pages, max } => format!("{pages} pages, au-delà de {max}"),
            Depassement::Objets { objets } => format!("{objets} éléments internes"),
            Depassement::Duree { secondes } => {
                format!("l'analyse a duré plus de {secondes} secondes")
            }
        }
    }

    /// Constat seul : « Pièce trop volumineuse pour être lue automatiquement (…). »
    pub fn constat(&self) -> String {
        format!(
            "Pièce trop volumineuse pour être lue automatiquement ({}).",
            self.detail()
        )
    }

    /// Phrase montrée sur la pièce.
    pub fn message(&self) -> String {
        format!(
            "{} Ouvrez le fichier pour la contrôler à la main.",
            self.constat()
        )
    }
}

impl std::fmt::Display for Depassement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Contrôle une pièce avec les bornes en vigueur. Bloquant : à lancer dans `spawn_blocking`.
pub fn controler(nom: &str, mime: &str, contenu: &[u8]) -> Result<(), Depassement> {
    controler_avec(limites(), nom, mime, contenu)
}

/// Contrôle une pièce avec ces bornes. Les fichiers qui ne sont pas lus (images, autres types)
/// ne sont pas analysés et passent.
pub fn controler_avec(
    l: &Limites,
    nom: &str,
    mime: &str,
    contenu: &[u8],
) -> Result<(), Depassement> {
    let genre = super::genre(nom, &mime.to_ascii_lowercase(), contenu);
    controler_genre(l, genre, contenu)
}

pub(super) fn controler_genre(l: &Limites, genre: Genre, c: &[u8]) -> Result<(), Depassement> {
    if matches!(genre, Genre::Image(_) | Genre::Autre) {
        return Ok(());
    }
    if c.len() > l.fichier_max {
        return Err(Depassement::Taille {
            octets: c.len(),
            max: l.fichier_max,
        });
    }
    match genre {
        Genre::Pdf => controler_pdf(l, c),
        Genre::Classeur if c.starts_with(b"PK\x03\x04") => controler_zip(l, c),
        _ => Ok(()),
    }
}

/// Écrit nulle part en comptant les octets.
struct Compteur(u64);

impl Write for Compteur {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 += b.len() as u64;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Octets réellement produits par `r`, au plus `max + 1` (lecture arrêtée au-delà). Une erreur
/// de lecture arrête le compte : l'analyse elle-même échouera au même endroit.
fn mesurer(r: impl Read, max: u64) -> u64 {
    let mut c = Compteur(0);
    let _ = std::io::copy(&mut r.take(max.saturating_add(1)), &mut c);
    c.0
}

/// Zip (xlsx, xlsm, ods) : nombre d'entrées, taille réellement décompressée (les tailles
/// annoncées peuvent mentir), et cellules des feuilles d'un xlsx.
fn controler_zip(l: &Limites, c: &[u8]) -> Result<(), Depassement> {
    let Ok(mut z) = zip::ZipArchive::new(Cursor::new(c)) else {
        return Ok(()); // illisible : calamine refusera aussi, sans rien charger.
    };
    if z.len() > l.entrees_zip_max {
        return Err(Depassement::EntreesZip { entrees: z.len() });
    }
    let mut reste = l.decompresse_max;
    for i in 0..z.len() {
        let Ok(mut entree) = z.by_index(i) else {
            continue;
        };
        let nom = entree.name().map(|n| n.to_string()).unwrap_or_default();
        let feuille = nom.starts_with("xl/worksheets/") && nom.ends_with(".xml");
        let mut lu = Compteur(0);
        {
            let mut limite = Lecteur {
                r: (&mut entree).take(reste.saturating_add(1)),
                n: &mut lu.0,
            };
            if feuille {
                let cellules = cellules_feuille(BufReader::new(&mut limite));
                if cellules > l.cellules_max {
                    return Err(Depassement::Cellules { cellules });
                }
            }
            let _ = std::io::copy(&mut limite, &mut Compteur(0));
        }
        if lu.0 > reste {
            return Err(Depassement::Decompresse {
                max: l.decompresse_max,
            });
        }
        reste -= lu.0;
    }
    Ok(())
}

/// Lecteur qui compte ce qui passe.
struct Lecteur<'a, R> {
    r: R,
    n: &'a mut u64,
}

impl<R: Read> Read for Lecteur<'_, R> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = self.r.read(b)?;
        *self.n += n as u64;
        Ok(n)
    }
}

/// Colonne et ligne (base 1) d'une référence « AB12 ».
fn reference(r: &[u8]) -> Option<(u64, u64)> {
    let lettres = r.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    if lettres == 0 || lettres > 3 {
        return None;
    }
    let col = r[..lettres].iter().fold(0u64, |a, b| {
        a * 26 + u64::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    let ligne = std::str::from_utf8(&r[lettres..]).ok()?.parse().ok()?;
    Some((col, ligne))
}

/// Rectangle de cellules d'une feuille xlsx (de A1 à la ligne et la colonne les plus loin).
fn cellules_feuille(r: impl std::io::BufRead) -> u64 {
    use quick_xml::events::Event;
    let mut lecteur = quick_xml::Reader::from_reader(r);
    let mut tampon = Vec::new();
    let (mut ligne, mut col) = (0u64, 0u64);
    let (mut ligne_max, mut col_max) = (0u64, 0u64);
    loop {
        match lecteur.read_event_into(&mut tampon) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let attribut_r = || {
                    e.attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == "r")
                        .map(|a| a.value.to_string())
                };
                match e.local_name().as_ref() {
                    "row" => {
                        ligne = attribut_r()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(ligne + 1);
                        col = 0;
                    }
                    "c" => {
                        match attribut_r().and_then(|v| reference(v.as_bytes())) {
                            Some((c, l)) => (col, ligne) = (c, l),
                            None => col += 1,
                        }
                        ligne_max = ligne_max.max(ligne);
                        col_max = col_max.max(col);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        tampon.clear();
    }
    ligne_max.saturating_mul(col_max)
}

fn trouver(dans: &[u8], motif: &[u8]) -> Option<usize> {
    dans.windows(motif.len()).position(|w| w == motif)
}

fn trouver_dernier(dans: &[u8], motif: &[u8]) -> Option<usize> {
    dans.windows(motif.len()).rposition(|w| w == motif)
}

/// Objets déclarés en clair (« 12 0 obj »).
fn objets_en_clair(c: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(p) = trouver(&c[i..], b" obj") {
        let pos = i + p;
        let suivant = c.get(pos + 4).copied().unwrap_or(b' ');
        if pos > 0
            && c[pos - 1].is_ascii_digit()
            && (suivant.is_ascii_whitespace() || matches!(suivant, b'<' | b'[' | b'/' | b'('))
        {
            n += 1;
        }
        i = pos + 4;
    }
    n
}

/// Valeur entière après une clé d'un dictionnaire (« /N 12 »).
fn entier_apres(dict: &[u8], cle: &[u8]) -> Option<usize> {
    let mut i = 0;
    while let Some(p) = trouver(&dict[i..], cle) {
        let apres = &dict[i + p + cle.len()..];
        // « /N » et pas « /Name ».
        if apres.first().is_some_and(|b| b.is_ascii_whitespace()) {
            let texte: String = apres
                .iter()
                .skip_while(|b| b.is_ascii_whitespace())
                .take_while(|b| b.is_ascii_digit())
                .map(|b| *b as char)
                .collect();
            return texte.parse().ok();
        }
        i += p + cle.len();
    }
    None
}

/// PDF : objets (en clair et dans les flux d'objets), octets décompressés des flux que
/// l'analyse décompresse (texte, objets, fichiers joints ; pas les images, que seul le rendu
/// des scans ouvre, dans un autre programme), puis pages.
fn controler_pdf(l: &Limites, c: &[u8]) -> Result<(), Depassement> {
    let mut objets = objets_en_clair(c);
    if objets > l.objets_pdf_max {
        return Err(Depassement::Objets { objets });
    }
    let mut reste = l.decompresse_max;
    let mut i = 0;
    while let Some(p) = trouver(&c[i..], b"stream") {
        let pos = i + p;
        i = pos + 6;
        if pos >= 3 && &c[pos - 3..pos] == b"end" {
            continue;
        }
        let mut debut = pos + 6;
        match (c.get(debut), c.get(debut + 1)) {
            (Some(b'\r'), Some(b'\n')) => debut += 2,
            (Some(b'\n'), _) | (Some(b'\r'), _) => debut += 1,
            _ => continue,
        }
        let avant = &c[pos.saturating_sub(4096)..pos];
        let dict = trouver_dernier(avant, b" obj").map_or(avant, |k| &avant[k..]);
        if trouver(dict, b"/ObjStm").is_some() {
            objets += entier_apres(dict, b"/N").unwrap_or(0);
            if objets > l.objets_pdf_max {
                return Err(Depassement::Objets { objets });
            }
        }
        if trouver(dict, b"Flate").is_none() || trouver(dict, b"/Image").is_some() {
            continue;
        }
        let mut z = flate2::read::ZlibDecoder::new(&c[debut..]);
        let n = mesurer(&mut z, reste);
        if n > reste {
            return Err(Depassement::Decompresse {
                max: l.decompresse_max,
            });
        }
        reste -= n;
        i = debut + (z.total_in() as usize).max(1);
    }
    // Flux bornés : le chargement par lopdf reste raisonnable.
    if let Ok(doc) = lopdf::Document::load_mem(c) {
        if doc.objects.len() > l.objets_pdf_max {
            return Err(Depassement::Objets {
                objets: doc.objects.len(),
            });
        }
        let pages = doc.get_pages().len();
        if pages > l.pages_max {
            return Err(Depassement::Pages {
                pages,
                max: l.pages_max,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip_de(entrees: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut sortie = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut sortie);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (nom, contenu) in entrees {
                z.start_file(*nom, options).unwrap();
                z.write_all(contenu).unwrap();
            }
            z.finish().unwrap();
        }
        sortie.into_inner()
    }

    fn feuille(cellules: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{cellules}</sheetData></worksheet>"#
        )
        .into_bytes()
    }

    #[test]
    fn references_de_cellules() {
        assert_eq!(reference(b"A1"), Some((1, 1)));
        assert_eq!(reference(b"AB12"), Some((28, 12)));
        assert_eq!(reference(b"XFD1048576"), Some((16384, 1048576)));
        assert_eq!(reference(b"12"), None);
    }

    #[test]
    fn feuille_aux_cellules_eparpillees_refusee() {
        let l = Limites::default();
        let bombe = zip_de(&[(
            "xl/worksheets/sheet1.xml",
            feuille(
                r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>2</v></c></row>"#,
            ),
        )]);
        assert!(matches!(
            controler_avec(&l, "bl.xlsx", "", &bombe),
            Err(Depassement::Cellules { .. })
        ));
        // Une feuille ordinaire (2 000 lignes, 20 colonnes) passe, même sans attribut `r`.
        let mut lignes = String::new();
        for _ in 0..2000 {
            lignes.push_str("<row>");
            for _ in 0..20 {
                lignes.push_str("<c><v>1</v></c>");
            }
            lignes.push_str("</row>");
        }
        let normal = zip_de(&[("xl/worksheets/sheet1.xml", feuille(&lignes))]);
        assert_eq!(controler_avec(&l, "bl.xlsx", "", &normal), Ok(()));
    }

    #[test]
    fn bombe_zip_refusee() {
        let l = Limites {
            decompresse_max: 1024 * 1024,
            entrees_zip_max: 10,
            ..Limites::default()
        };
        let bombe = zip_de(&[("xl/sharedStrings.xml", vec![b'a'; 3 * 1024 * 1024])]);
        assert!(bombe.len() < 64 * 1024, "{}", bombe.len());
        assert_eq!(
            controler_avec(&l, "bl.xlsx", "", &bombe),
            Err(Depassement::Decompresse { max: 1024 * 1024 })
        );
        let nombreux: Vec<(String, Vec<u8>)> =
            (0..11).map(|i| (format!("x{i}.xml"), vec![b'a'])).collect();
        let refs: Vec<(&str, Vec<u8>)> = nombreux
            .iter()
            .map(|(n, c)| (n.as_str(), c.clone()))
            .collect();
        assert_eq!(
            controler_avec(&l, "bl.xlsx", "", &zip_de(&refs)),
            Err(Depassement::EntreesZip { entrees: 11 })
        );
    }

    fn pdf_avec_flux(octets_decompresses: usize, dict: &str) -> Vec<u8> {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&vec![b' '; octets_decompresses]).unwrap();
        let flux = z.finish().unwrap();
        let mut pdf = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec();
        pdf.extend_from_slice(
            format!(
                "2 0 obj\n<< /Length {} /Filter /FlateDecode {dict} >>\nstream\n",
                flux.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&flux);
        pdf.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
        pdf
    }

    #[test]
    fn flux_pdf_qui_gonfle_refuse() {
        let l = Limites {
            decompresse_max: 1024 * 1024,
            ..Limites::default()
        };
        let bombe = pdf_avec_flux(4 * 1024 * 1024, "");
        assert_eq!(
            controler_avec(&l, "f.pdf", "application/pdf", &bombe),
            Err(Depassement::Decompresse { max: 1024 * 1024 })
        );
        // Une image n'est jamais décompressée par l'analyse : elle ne compte pas.
        let image = pdf_avec_flux(4 * 1024 * 1024, "/Subtype /Image");
        assert_eq!(
            controler_avec(&l, "f.pdf", "application/pdf", &image),
            Ok(())
        );
        assert_eq!(
            controler_avec(&l, "f.pdf", "application/pdf", &pdf_avec_flux(1000, "")),
            Ok(())
        );
    }

    #[test]
    fn trop_d_objets_ou_trop_gros() {
        let l = Limites {
            objets_pdf_max: 1,
            fichier_max: 10_000,
            ..Limites::default()
        };
        let pdf = pdf_avec_flux(10, "");
        assert_eq!(
            controler_avec(&l, "f.pdf", "", &pdf),
            Err(Depassement::Objets { objets: 2 })
        );
        let gros = pdf_avec_flux(1, &" ".repeat(20_000));
        assert!(matches!(
            controler_avec(&l, "f.pdf", "", &gros),
            Err(Depassement::Taille { .. })
        ));
        // Un fichier qui n'est pas lu (image) n'est pas mesuré.
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(vec![0u8; 20_000]);
        assert_eq!(controler_avec(&l, "logo.png", "image/png", &png), Ok(()));
    }

    #[test]
    fn message_lisible() {
        assert_eq!(
            Depassement::Pages {
                pages: 5000,
                max: 1000
            }
            .message(),
            "Pièce trop volumineuse pour être lue automatiquement (5000 pages, au-delà de 1000). \
             Ouvrez le fichier pour la contrôler à la main."
        );
    }
}
