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
/// Un flux d'un PDF une fois décodé, images comprises (une page A4 scannée en couleur à
/// 300 points par pouce, sans compression JPEG, fait 26 Mo).
pub const FLUX_MAX: u64 = 64 * 1024 * 1024;
/// Travail du premier contrôle d'un PDF : octets lus et produits par tous les décodages
/// essayés (quelques secondes de calcul).
pub const TRAVAIL_PDF_MAX: u64 = 2 * 1024 * 1024 * 1024;
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
    pub flux_max: u64,
    pub travail_pdf_max: u64,
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
            flux_max: FLUX_MAX,
            travail_pdf_max: TRAVAIL_PDF_MAX,
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
            flux_max: d.flux_max,
            travail_pdf_max: d.travail_pdf_max,
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
    Taille {
        octets: usize,
        max: usize,
    },
    Decompresse {
        max: u64,
    },
    EntreesZip {
        entrees: usize,
    },
    Cellules {
        cellules: u64,
    },
    Pages {
        pages: usize,
        max: usize,
    },
    Objets {
        objets: usize,
    },
    Duree {
        secondes: u64,
    },
    /// Structure interne anormale (fichier fait pour épuiser l'analyse, format de classeur
    /// non pris en charge).
    Structure,
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
            Depassement::Structure => "structure",
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
            Depassement::Structure => "structure interne anormale".to_string(),
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
        Genre::Classeur => controler_xls(l, c),
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
/// annoncées peuvent mentir), cellules de chaque partie XML (calamine trouve les feuilles par
/// les relations du classeur, sous n'importe quel nom) et taille annoncée de la table des
/// textes (calamine la réserve d'avance). Un classeur binaire (xlsb) déguisé est refusé.
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
        let nom = entree
            .name()
            .map(|n| n.to_ascii_lowercase())
            .unwrap_or_default();
        // Classeur binaire : ses feuilles échappent au contrôle des cellules.
        if nom == "xl/workbook.bin" {
            return Err(Depassement::Structure);
        }
        let xml = nom.ends_with(".xml");
        let ods = nom == "content.xml";
        let mut lu = Compteur(0);
        {
            let mut limite = Lecteur {
                r: (&mut entree).take(reste.saturating_add(1)),
                n: &mut lu.0,
            };
            if xml {
                let m = mesurer_xml(BufReader::new(&mut limite), ods);
                if m.cellules > l.cellules_max {
                    return Err(Depassement::Cellules {
                        cellules: m.cellules,
                    });
                }
                if m.textes > l.cellules_max {
                    return Err(Depassement::Cellules { cellules: m.textes });
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

/// Ce que calamine réserverait pour une partie XML.
#[derive(Debug, Default, PartialEq, Eq)]
struct MesureXml {
    /// Plus grand rectangle de cellules d'une feuille (de A1 à la ligne et à la colonne
    /// remplies les plus loin).
    cellules: u64,
    /// `uniqueCount` annoncé d'une table de textes (`<sst>`).
    textes: u64,
}

/// Lignes et colonnes au plus d'une feuille (calamine ne va pas au-delà).
const LIGNES_MAX: u64 = 1_048_576;
const COLONNES_MAX: u64 = 16_384;

/// Mesure une partie XML : feuille xlsx (`<row>`, `<c r="AB12">`), table de textes, ou contenu
/// ods (`content.xml` : lignes et cellules répétées, seules les cellules remplies comptent,
/// comme dans calamine qui ignore les lignes vides de la fin).
fn mesurer_xml(r: impl std::io::BufRead, ods: bool) -> MesureXml {
    use quick_xml::events::Event;
    let mut lecteur = quick_xml::Reader::from_reader(r);
    let mut tampon = Vec::new();
    let mut m = MesureXml::default();
    let (mut ligne, mut col) = (0u64, 0u64);
    let (mut ligne_max, mut col_max) = (0u64, 0u64);
    // ods : répétitions de la ligne en cours.
    let mut repetees = 1u64;
    loop {
        let evenement = lecteur.read_event_into(&mut tampon);
        let (e, vide) = match evenement {
            Ok(Event::Start(e)) => (e, false),
            Ok(Event::Empty(e)) => (e, true),
            Ok(Event::End(e)) => {
                if ods && e.name().as_ref() == "table:table-row" {
                    ligne = (ligne + repetees).min(LIGNES_MAX);
                    repetees = 1;
                }
                tampon.clear();
                continue;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {
                tampon.clear();
                continue;
            }
        };
        let attribut = |nom: &str| {
            e.attributes()
                .flatten()
                .find(|a| a.key.as_ref() == nom)
                .map(|a| a.value.to_string())
        };
        let nombre = |nom: &str| {
            attribut(nom)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(1)
                .max(1)
        };
        if ods {
            match e.name().as_ref() {
                "table:table" => {
                    m.cellules = m.cellules.max(ligne_max.saturating_mul(col_max));
                    (ligne, ligne_max, col_max) = (0, 0, 0);
                }
                "table:table-row" => {
                    repetees = nombre("table:number-rows-repeated");
                    col = 0;
                    if vide {
                        ligne = (ligne + repetees).min(LIGNES_MAX);
                        repetees = 1;
                    }
                }
                "table:table-cell" | "table:covered-table-cell" => {
                    let n = nombre("table:number-columns-repeated");
                    let fin = (col + n).min(COLONNES_MAX);
                    if !vide || attribut("office:value-type").is_some() {
                        col_max = col_max.max(fin);
                        ligne_max = ligne_max.max((ligne + repetees).min(LIGNES_MAX));
                    }
                    col = fin;
                }
                _ => {}
            }
        } else {
            match e.local_name().as_ref() {
                "row" => {
                    ligne = attribut("r")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(ligne + 1);
                    col = 0;
                }
                "c" => {
                    match attribut("r").and_then(|v| reference(v.as_bytes())) {
                        Some((c, l)) => (col, ligne) = (c, l),
                        None => col += 1,
                    }
                    ligne_max = ligne_max.max(ligne);
                    col_max = col_max.max(col);
                }
                "sst" => {
                    let n = attribut("uniqueCount").and_then(|v| v.trim().parse::<u64>().ok());
                    m.textes = m.textes.max(n.unwrap_or(0));
                }
                _ => {}
            }
        }
        tampon.clear();
    }
    m.cellules = m.cellules.max(ligne_max.saturating_mul(col_max));
    m
}

/// Flux « Workbook » (ou « Book ») d'un classeur xls (conteneur OLE), lu sans rien allouer
/// au-delà de la taille du fichier. `None` : conteneur illisible (calamine refusera aussi).
fn flux_xls(c: &[u8]) -> Option<Vec<u8>> {
    const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    const SPECIAL: u32 = 0xFFFF_FFFA;
    let u16a = |o: usize| c.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32a = |o: usize| {
        c.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if !c.starts_with(&SIGNATURE) {
        return None;
    }
    let decalage = u16a(0x1E)?;
    let mini_decalage = u16a(0x20)?;
    if !(7..=16).contains(&decalage) || mini_decalage >= decalage {
        return None;
    }
    let taille = 1usize << decalage;
    let mini_taille = 1usize << mini_decalage;
    let secteurs_max = c.len() / taille + 1;
    let secteur = |n: u32| -> Option<&[u8]> {
        let o = (n as usize).checked_add(1)?.checked_mul(taille)?;
        c.get(o..o.checked_add(taille)?)
    };
    let mots = |b: &[u8]| -> Vec<u32> {
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|m| u32::from_le_bytes(*m))
            .collect()
    };
    // Secteurs de la FAT : 109 dans l'en-tête, puis la chaîne DIFAT.
    let mut secteurs_fat: Vec<u32> = (0..109).filter_map(|i| u32a(0x4C + 4 * i)).collect();
    let mut d = u32a(0x44)?;
    for _ in 0..secteurs_max {
        if d >= SPECIAL {
            break;
        }
        let Some(s) = secteur(d) else { break };
        let m = mots(s);
        let (suite, dernier) = m.split_at(m.len() - 1);
        secteurs_fat.extend_from_slice(suite);
        d = dernier[0];
    }
    let mut fat = Vec::new();
    for s in secteurs_fat
        .into_iter()
        .filter(|s| *s < SPECIAL)
        .take(secteurs_max)
    {
        if let Some(b) = secteur(s) {
            fat.extend(mots(b));
        }
    }
    let chaine = |debut: u32| -> Vec<u8> {
        let mut sortie = Vec::new();
        let mut n = debut;
        for _ in 0..secteurs_max {
            if n >= SPECIAL {
                break;
            }
            let Some(b) = secteur(n) else { break };
            sortie.extend_from_slice(b);
            n = fat.get(n as usize).copied().unwrap_or(u32::MAX);
        }
        sortie
    };
    let repertoire = chaine(u32a(0x30)?);
    let entrees: Vec<&[u8]> = repertoire
        .as_chunks::<128>()
        .0
        .iter()
        .map(|e| &e[..])
        .collect();
    let racine = entrees.iter().find(|e| e[66] == 5)?;
    let nom = |e: &[u8]| -> String {
        let n = (u16::from_le_bytes([e[64], e[65]]) as usize).min(64);
        let unites: Vec<u16> = e[..n]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|u| u16::from_le_bytes(*u))
            .take_while(|u| *u != 0)
            .collect();
        String::from_utf16_lossy(&unites)
    };
    let classeur = entrees.iter().find(|e| {
        e[66] == 2 && matches!(nom(e).to_ascii_lowercase().as_str(), "workbook" | "book")
    })?;
    let debut = u32::from_le_bytes(classeur[116..120].try_into().ok()?);
    let longueur = u64::from_le_bytes(classeur[120..128].try_into().ok()?);
    let longueur = if decalage == 9 {
        longueur & 0xFFFF_FFFF
    } else {
        longueur
    };
    let longueur = usize::try_from(longueur).unwrap_or(usize::MAX);
    let seuil_mini = u64::from(u32a(0x38)?);
    let mut flux = if (longueur as u64) < seuil_mini {
        // Petit flux : dans le mini-flux de la racine, chaîné par la mini-FAT.
        let mini_flux = chaine(u32::from_le_bytes(racine[116..120].try_into().ok()?));
        let mini_fat = mots(&chaine(u32a(0x3C)?));
        let mini_max = mini_flux.len() / mini_taille + 1;
        let mut sortie = Vec::new();
        let mut n = debut;
        for _ in 0..mini_max {
            if n >= SPECIAL {
                break;
            }
            let o = n as usize * mini_taille;
            let Some(b) = mini_flux.get(o..o + mini_taille) else {
                break;
            };
            sortie.extend_from_slice(b);
            n = mini_fat.get(n as usize).copied().unwrap_or(u32::MAX);
        }
        sortie
    } else {
        chaine(debut)
    };
    flux.truncate(longueur);
    Some(flux)
}

/// Cellules qu'un classeur xls ferait réserver à calamine : pour chaque feuille (positions
/// des `BoundSheet8`), l'enregistrement `DIMENSIONS` (réservé tel quel) et le rectangle des
/// cellules lues (lignes et colonnes de chaque enregistrement de cellule).
fn cellules_xls(flux: &[u8]) -> u64 {
    fn enregistrements(f: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
        let mut reste = f;
        std::iter::from_fn(move || {
            if reste.len() < 4 {
                return None;
            }
            let t = u16::from_le_bytes([reste[0], reste[1]]);
            let n = u16::from_le_bytes([reste[2], reste[3]]) as usize;
            let (e, suite) = reste.split_at_checked(4 + n)?;
            reste = suite;
            Some((t, &e[4..]))
        })
    }
    let u16a = |d: &[u8], o: usize| {
        d.get(o..o + 2)
            .map_or(0, |b| u64::from(u16::from_le_bytes([b[0], b[1]])))
    };
    let u32a = |d: &[u8], o: usize| {
        d.get(o..o + 4).map_or(0, |b| {
            u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        })
    };
    let mut positions = vec![];
    for (t, d) in enregistrements(flux) {
        if t == 0x0085 && d.len() >= 4 {
            positions.push(u32a(d, 0) as usize);
        }
    }
    let mut plus_grand = 0u64;
    for p in positions {
        let Some(feuille) = flux.get(p..) else {
            continue;
        };
        let (mut ligne_max, mut col_max) = (0u64, 0u64);
        for (t, d) in enregistrements(feuille) {
            match t {
                // DIMENSIONS : calamine réserve lignes × colonnes avant de lire la feuille.
                0x0200 => {
                    let (rf, rl, mut cf, cl) = match d.len() {
                        10 => (u16a(d, 0), u16a(d, 2), u16a(d, 4), u16a(d, 6)),
                        14 | 16 => (u32a(d, 0), u32a(d, 4), u16a(d, 8), u16a(d, 10)),
                        _ => continue,
                    };
                    if cf > 0xFF || cl < cf {
                        cf = 0;
                    }
                    if rl >= 1 && cl >= 1 {
                        // Fin avant début : calamine déborde, la réservation est énorme.
                        let lignes = (rl - 1).checked_sub(rf).map_or(u64::MAX, |n| n + 1);
                        let cols = (cl - 1).checked_sub(cf).map_or(u64::MAX, |n| n + 1);
                        plus_grand = plus_grand.max(lignes.saturating_mul(cols));
                    }
                }
                // Cellules : ligne puis colonne en tête ; MULRK / MULBLANK : dernière colonne
                // à la fin.
                0x0006 | 0x0201 | 0x0203 | 0x0204 | 0x0205 | 0x00D6 | 0x00FD | 0x027E | 0x00BD
                | 0x00BE => {
                    if d.len() < 4 {
                        continue;
                    }
                    ligne_max = ligne_max.max(u16a(d, 0) + 1);
                    let col = if matches!(t, 0x00BD | 0x00BE) && d.len() >= 6 {
                        u16a(d, d.len() - 2)
                    } else {
                        u16a(d, 2)
                    };
                    col_max = col_max.max(col + 1);
                }
                _ => {}
            }
        }
        plus_grand = plus_grand.max(ligne_max.saturating_mul(col_max));
    }
    plus_grand
}

/// Classeur xls : cellules réservées par calamine (enregistrement `DIMENSIONS` ou cellules
/// éparpillées), dont une seule valeur fausse suffit à demander des téraoctets.
fn controler_xls(l: &Limites, c: &[u8]) -> Result<(), Depassement> {
    let Some(flux) = flux_xls(c) else {
        return Ok(());
    };
    let cellules = cellules_xls(&flux);
    if cellules > l.cellules_max {
        return Err(Depassement::Cellules { cellules });
    }
    Ok(())
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

/// Données d'un flux décodées par ASCII85 comme lopdf (blancs ignorés, `z` = 4 zéros, arrêt
/// au premier caractère étranger) ; rend les octets produits (au plus `max`) et les octets lus.
fn ascii85(d: &[u8], max: usize) -> (Vec<u8>, usize) {
    let mut sortie = Vec::new();
    let (mut tampon, mut n, mut lus) = (0u32, 0, 0);
    for &ch in d {
        if sortie.len() > max {
            break;
        }
        lus += 1;
        if ch == b'z' && n == 0 {
            sortie.extend_from_slice(&[0; 4]);
            continue;
        }
        if ch.is_ascii_whitespace() {
            continue;
        }
        if !(b'!'..=b'u').contains(&ch) {
            break;
        }
        tampon = tampon.wrapping_mul(85).wrapping_add(u32::from(ch - b'!'));
        n += 1;
        if n == 5 {
            sortie.extend_from_slice(&tampon.to_be_bytes());
            (tampon, n) = (0, 0);
        }
    }
    (sortie, lus)
}

/// Octets produits par un décodage Flate, comme lopdf : zlib, et deflate brut après l'en-tête
/// si zlib ne donne rien. Rend (produits, au plus `max + 1` ; lus).
fn mesurer_flate(d: &[u8], max: u64) -> (u64, u64) {
    let mut z = flate2::read::ZlibDecoder::new(d);
    let n = mesurer(&mut z, max);
    if n > 0 || d.len() <= 2 {
        return (n, z.total_in());
    }
    let mut brut = flate2::read::DeflateDecoder::new(&d[2..]);
    let n = mesurer(&mut brut, max);
    (n, brut.total_in() + 2)
}

/// Décodage LZW (les deux variantes de lopdf) arrêté au-delà de `max` octets produits ; rend
/// (produits, lus) pour la variante qui produit le plus, et garde ses octets dans `garde` si
/// demandé.
fn lzw(d: &[u8], max: u64, mut garde: Option<&mut Vec<u8>>) -> (u64, u64) {
    use weezl::{BitOrder, LzwStatus, decode::Decoder};
    let mut meilleur = (0, 0);
    for mut dec in [
        Decoder::with_tiff_size_switch(BitOrder::Msb, 8),
        Decoder::new(BitOrder::Msb, 8),
    ] {
        let mut tampon = vec![0u8; 64 * 1024];
        let mut sortie = Vec::new();
        let (mut produits, mut lus) = (0u64, 0usize);
        while produits <= max && lus < d.len() {
            let r = dec.decode_bytes(&d[lus..], &mut tampon);
            lus += r.consumed_in;
            produits += r.consumed_out as u64;
            if garde.is_some() {
                sortie.extend_from_slice(&tampon[..r.consumed_out]);
            }
            match r.status {
                Ok(LzwStatus::Ok) if r.consumed_in + r.consumed_out > 0 => {}
                _ => break,
            }
        }
        if produits >= meilleur.0 {
            meilleur = (produits, lus as u64);
            if let Some(g) = garde.as_deref_mut() {
                *g = sortie;
            }
        }
    }
    meilleur
}

fn mesurer_lzw(d: &[u8], max: u64) -> (u64, u64) {
    lzw(d, max, None)
}

/// Ce que donne au plus le flux qui commence en `d` une fois décodé par lopdf (Flate, LZW, ou
/// ASCII85 suivi de l'un d'eux), au plus `max + 1`. Le travail fait (octets lus et produits par
/// tous les essais) s'ajoute à `travail`.
fn mesurer_flux(d: &[u8], max: u64, travail: &mut u64) -> u64 {
    let mut plus_grand = 0;
    let mut essai = |(produits, lus): (u64, u64)| {
        *travail += produits + lus;
        plus_grand = plus_grand.max(produits);
    };
    essai(mesurer_flate(d, max));
    essai(mesurer_lzw(d, max));
    if d.iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| (b'!'..=b'z').contains(b))
    {
        let (a, lus) = ascii85(d, usize::try_from(max).unwrap_or(usize::MAX));
        essai((a.len() as u64, lus as u64));
        if !a.is_empty() {
            essai(mesurer_flate(&a, max));
            essai(mesurer_lzw(&a, max));
        }
    }
    plus_grand
}

/// Décode un flux parsé par lopdf selon ses filtres (ceux que lopdf sait décoder) sans dépasser
/// `max + 1` octets ; rend le nombre d'octets obtenus.
fn mesurer_flux_parse(flux: &lopdf::Stream, max: u64) -> u64 {
    let Ok(filtres) = flux.filters() else {
        return flux.content.len() as u64;
    };
    let mut donnees: std::borrow::Cow<[u8]> = std::borrow::Cow::Borrowed(&flux.content);
    for (i, f) in filtres.iter().enumerate() {
        let dernier = i + 1 == filtres.len();
        let lire = |r: &mut dyn Read| -> Vec<u8> {
            let mut v = Vec::new();
            let _ = r.take(max.saturating_add(1)).read_to_end(&mut v);
            v
        };
        donnees = match *f {
            b"FlateDecode" => {
                if dernier {
                    return mesurer_flate(&donnees, max).0;
                }
                let mut z = flate2::read::ZlibDecoder::new(&donnees[..]);
                let mut v = lire(&mut z);
                if v.is_empty() && donnees.len() > 2 {
                    v = lire(&mut flate2::read::DeflateDecoder::new(&donnees[2..]));
                }
                v.into()
            }
            b"LZWDecode" => {
                if dernier {
                    return mesurer_lzw(&donnees, max).0;
                }
                let mut v = Vec::new();
                lzw(&donnees, max, Some(&mut v));
                v.into()
            }
            b"ASCII85Decode" => ascii85(&donnees, usize::try_from(max).unwrap_or(usize::MAX))
                .0
                .into(),
            // lopdf ne décode pas les autres filtres : le contenu reste tel quel.
            _ => return donnees.len() as u64,
        };
        if donnees.len() as u64 > max {
            return donnees.len() as u64;
        }
    }
    donnees.len() as u64
}

/// PDF, en deux temps.
///
/// 1. Avant toute ouverture par lopdf (qui décompresse les flux d'objets et de références
///    sans borne) : chaque flux du fichier, quel que soit son dictionnaire (un nom peut être
///    écrit `/Fl#61teDecode`, un dictionnaire peut être long ou se dire image), est décodé
///    sans rien garder, de toutes les façons que lopdf sait faire. Aucun ne doit dépasser
///    `flux_max`, et le travail total reste borné (fichier fait pour épuiser le processeur).
/// 2. Fichier ouvert : objets, pages, et octets des flux que l'analyse décode (tout sauf les
///    images, que seul le rendu des scans ouvre, dans un autre programme ; un contenu de page
///    compte toujours), au plus `decompresse_max`.
///
/// Reste non borné : un PDF chiffré dont un flux d'objets se décompresse en gigaoctets (le
/// chiffrement cache le contenu au premier temps).
fn controler_pdf(l: &Limites, c: &[u8]) -> Result<(), Depassement> {
    let mut objets = objets_en_clair(c);
    if objets > l.objets_pdf_max {
        return Err(Depassement::Objets { objets });
    }
    let mut travail = 0u64;
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
        let reste = l.travail_pdf_max.saturating_sub(travail);
        let d = &c[debut
            ..c.len()
                .min(debut.saturating_add(usize::try_from(reste).unwrap_or(usize::MAX)))];
        if mesurer_flux(d, l.flux_max, &mut travail) > l.flux_max {
            return Err(Depassement::Decompresse { max: l.flux_max });
        }
        if travail > l.travail_pdf_max {
            return Err(Depassement::Structure);
        }
    }
    let Ok(doc) = lopdf::Document::load_mem(c) else {
        return Ok(()); // illisible : l'analyse échouera de même, sans rien charger de plus.
    };
    if doc.objects.len() > l.objets_pdf_max {
        return Err(Depassement::Objets {
            objets: doc.objects.len(),
        });
    }
    let pages = doc.get_pages();
    if pages.len() > l.pages_max {
        return Err(Depassement::Pages {
            pages: pages.len(),
            max: l.pages_max,
        });
    }
    let contenus: std::collections::HashSet<lopdf::ObjectId> = pages
        .values()
        .flat_map(|p| doc.get_page_contents(*p))
        .collect();
    let mut reste = l.decompresse_max;
    for (id, o) in &doc.objects {
        let lopdf::Object::Stream(flux) = o else {
            continue;
        };
        let image = flux
            .dict
            .get(b"Subtype")
            .and_then(lopdf::Object::as_name)
            .is_ok_and(|n| n == b"Image");
        if image && !contenus.contains(id) {
            continue;
        }
        let n = mesurer_flux_parse(flux, reste);
        if n > reste {
            return Err(Depassement::Decompresse {
                max: l.decompresse_max,
            });
        }
        reste -= n;
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
            flux_max: 1024 * 1024,
            ..Limites::default()
        };
        let bombe = pdf_avec_flux(4 * 1024 * 1024, "");
        assert_eq!(
            controler_avec(&l, "f.pdf", "application/pdf", &bombe),
            Err(Depassement::Decompresse { max: 1024 * 1024 })
        );
        // Le premier contrôle ne lit pas le dictionnaire : se dire image ne sert à rien.
        let image = pdf_avec_flux(4 * 1024 * 1024, "/Subtype /Image");
        assert!(controler_avec(&l, "f.pdf", "application/pdf", &image).is_err());
        assert_eq!(
            controler_avec(&l, "f.pdf", "application/pdf", &pdf_avec_flux(1000, "")),
            Ok(())
        );
    }

    /// PDF complet (lopdf) : une page dont le contenu est `contenu` (dictionnaire du flux
    /// complété par `dict`) et une image de `image` octets (Flate) en ressource.
    fn pdf_page(contenu: Vec<u8>, dict: lopdf::Dictionary, image: usize) -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let contenu_id = doc.add_object(Stream::new(dict, contenu));
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&vec![0x80; image]).unwrap();
        let image_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 1024,
                "Height" => (image / 1024).max(1) as i64, "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8, "Filter" => "FlateDecode",
            },
            z.finish().unwrap(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => contenu_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalogue = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalogue);
        let mut sortie = Vec::new();
        doc.save_to(&mut sortie).unwrap();
        sortie
    }

    fn zlib(d: &[u8]) -> Vec<u8> {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(d).unwrap();
        z.finish().unwrap()
    }

    #[test]
    fn contournements_du_dictionnaire_vus() {
        use lopdf::dictionary;
        let mo = 1024 * 1024;
        let l = Limites {
            flux_max: mo,
            ..Limites::default()
        };
        let gros = zlib(&vec![b' '; 4 * mo as usize]);
        // Nom de filtre écrit avec un échappement, dictionnaire de plus de 4 Ko.
        let pdf = pdf_page(
            gros.clone(),
            dictionary! { "Filter" => "FlateDecode", "Bourrage" => lopdf::Object::string_literal(" ".repeat(8000)) },
            0,
        );
        let i = trouver(&pdf, b"/FlateDecode").unwrap();
        let mut piege = pdf[..i].to_vec();
        piege.extend_from_slice(b"/Fl#61teDecode");
        piege.extend_from_slice(&pdf[i + b"/FlateDecode".len()..]);
        assert!(matches!(
            controler_avec(&l, "f.pdf", "", &piege),
            Err(Depassement::Decompresse { .. })
        ));
        // ASCII85 puis Flate, et LZW.
        let a85: Vec<u8> = {
            let mut s = Vec::new();
            for bloc in gros.chunks(4) {
                let mut b = [0u8; 4];
                b[..bloc.len()].copy_from_slice(bloc);
                let mut v = u32::from_be_bytes(b);
                let mut c = [0u8; 5];
                for k in (0..5).rev() {
                    c[k] = (v % 85) as u8 + b'!';
                    v /= 85;
                }
                s.extend_from_slice(&c[..bloc.len() + 1]);
            }
            s.extend_from_slice(b"~>");
            s
        };
        let pdf = pdf_page(
            a85,
            dictionary! { "Filter" => vec!["ASCII85Decode".into(), "FlateDecode".into()] },
            0,
        );
        assert!(matches!(
            controler_avec(&l, "f.pdf", "", &pdf),
            Err(Depassement::Decompresse { .. })
        ));
        let lzw = weezl::encode::Encoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8)
            .encode(&vec![0u8; 4 * mo as usize])
            .unwrap();
        let pdf = pdf_page(lzw, dictionary! { "Filter" => "LZWDecode" }, 0);
        assert!(matches!(
            controler_avec(&l, "f.pdf", "", &pdf),
            Err(Depassement::Decompresse { .. })
        ));
    }

    #[test]
    fn images_hors_du_compte_mais_pas_un_contenu_de_page() {
        use lopdf::dictionary;
        let mo = 1024 * 1024;
        let l = Limites {
            decompresse_max: mo,
            ..Limites::default()
        };
        // Une image de 4 Mo (scan sans JPEG) : l'analyse ne la décode pas, elle passe.
        let scan = pdf_page(b"q Q".to_vec(), dictionary! {}, 4 * mo as usize);
        assert_eq!(controler_avec(&l, "scan.pdf", "", &scan), Ok(()));
        // Un contenu de page qui se dit image compte quand même.
        let contenu = zlib(&vec![b' '; 4 * mo as usize]);
        let piege = pdf_page(
            contenu,
            dictionary! { "Filter" => "FlateDecode", "Subtype" => "Image" },
            0,
        );
        assert_eq!(
            controler_avec(&l, "f.pdf", "", &piege),
            Err(Depassement::Decompresse { max: mo })
        );
        // Travail du premier contrôle borné.
        let l = Limites {
            travail_pdf_max: 1000,
            ..Limites::default()
        };
        assert_eq!(
            controler_avec(&l, "scan.pdf", "", &scan),
            Err(Depassement::Structure)
        );
    }

    #[test]
    fn ods_aux_cellules_repetees() {
        let l = Limites::default();
        let contenu = |lignes: &str| {
            format!(
                r#"<?xml version="1.0"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet><table:table table:name="BL">{lignes}</table:table></office:spreadsheet></office:body></office:document-content>"#
            )
            .into_bytes()
        };
        let piege = contenu(
            r#"<table:table-row table:number-rows-repeated="100000"><table:table-cell table:number-columns-repeated="16384" office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell></table:table-row>"#,
        );
        let ods = zip_de(&[
            (
                "mimetype",
                b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
            ),
            ("content.xml", piege),
        ]);
        assert!(matches!(
            controler_avec(&l, "bl.ods", "", &ods),
            Err(Depassement::Cellules { .. })
        ));
        // Les lignes vides répétées jusqu'au bout de la feuille (LibreOffice) ne comptent pas.
        let normal = contenu(
            r#"<table:table-row><table:table-cell office:value-type="string"><text:p>BL 42</text:p></table:table-cell><table:table-cell table:number-columns-repeated="1000"/></table:table-row><table:table-row table:number-rows-repeated="1048575"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>"#,
        );
        let ods = zip_de(&[
            (
                "mimetype",
                b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
            ),
            ("content.xml", normal),
        ]);
        assert_eq!(controler_avec(&l, "bl.ods", "", &ods), Ok(()));
    }

    #[test]
    fn xlsx_feuille_ailleurs_table_de_textes_et_xlsb() {
        let l = Limites::default();
        let ailleurs = zip_de(&[(
            "xl/autre/f.xml",
            feuille(r#"<row r="1048576"><c r="XFD1048576"><v>2</v></c></row>"#),
        )]);
        assert!(matches!(
            controler_avec(&l, "bl.xlsx", "", &ailleurs),
            Err(Depassement::Cellules { .. })
        ));
        let textes = zip_de(&[(
            "xl/sharedStrings.xml",
            br#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="999999999999"><si><t>x</t></si></sst>"#.to_vec(),
        )]);
        assert!(matches!(
            controler_avec(&l, "bl.xlsx", "", &textes),
            Err(Depassement::Cellules { .. })
        ));
        let xlsb = zip_de(&[("xl/workbook.bin", vec![0; 10])]);
        assert_eq!(
            controler_avec(&l, "bl.xlsx", "", &xlsb),
            Err(Depassement::Structure)
        );
    }

    /// Classeur xls (BIFF8) d'une feuille : `dimensions` (lignes de, à ; colonnes de, à) et
    /// cellules numériques (ligne, colonne).
    fn biff(dimensions: (u32, u32, u16, u16), cellules: &[(u16, u16)]) -> Vec<u8> {
        fn enr(t: u16, d: &[u8]) -> Vec<u8> {
            let mut v = t.to_le_bytes().to_vec();
            v.extend_from_slice(&(d.len() as u16).to_le_bytes());
            v.extend_from_slice(d);
            v
        }
        let bof = |dt: u16| {
            let mut d = 0x0600u16.to_le_bytes().to_vec();
            d.extend_from_slice(&dt.to_le_bytes());
            d.extend_from_slice(&[0; 12]);
            enr(0x0809, &d)
        };
        let nom = b"BL";
        let feuille_enr = |pos: u32| {
            let mut d = pos.to_le_bytes().to_vec();
            d.extend_from_slice(&[0, 0, nom.len() as u8, 0]);
            d.extend_from_slice(nom);
            enr(0x0085, &d)
        };
        let mut globaux = bof(0x0005);
        let taille_globaux = globaux.len() + feuille_enr(0).len() + 4;
        globaux.extend(feuille_enr(taille_globaux as u32));
        globaux.extend(enr(0x000A, &[]));
        let mut feuille = bof(0x0010);
        let (rf, rl, cf, cl) = dimensions;
        let mut d = rf.to_le_bytes().to_vec();
        d.extend_from_slice(&rl.to_le_bytes());
        d.extend_from_slice(&cf.to_le_bytes());
        d.extend_from_slice(&cl.to_le_bytes());
        d.extend_from_slice(&[0, 0]);
        feuille.extend(enr(0x0200, &d));
        for (r, c) in cellules {
            let mut d = r.to_le_bytes().to_vec();
            d.extend_from_slice(&c.to_le_bytes());
            d.extend_from_slice(&0u16.to_le_bytes());
            d.extend_from_slice(&42f64.to_le_bytes());
            feuille.extend(enr(0x0203, &d));
        }
        feuille.extend(enr(0x000A, &[]));
        globaux.extend(feuille);
        globaux
    }

    /// Conteneur OLE (CFB v3, secteurs de 512 octets) qui porte `classeur` dans « Workbook » :
    /// dans le mini-flux s'il fait moins de 4 096 octets, comme Excel.
    fn cfb(classeur: &[u8]) -> Vec<u8> {
        const FIN: u32 = 0xFFFF_FFFE;
        const LIBRE: u32 = 0xFFFF_FFFF;
        let mini = classeur.len() < 4096;
        let secteurs = |n: usize| n.div_ceil(512);
        let mut fat = vec![0xFFFF_FFFD, FIN];
        let mut donnees = Vec::new();
        let (debut_classeur, debut_racine, taille_racine, mini_fat_debut);
        if mini {
            // Secteur 2 : mini-FAT ; puis le mini-flux.
            fat.push(FIN);
            let n_mini = classeur.len().div_ceil(64);
            let mut mini_fat: Vec<u32> = (1..n_mini as u32).collect();
            mini_fat.push(FIN);
            mini_fat.resize(128, LIBRE);
            let mut s: Vec<u8> = mini_fat.iter().flat_map(|m| m.to_le_bytes()).collect();
            let mut flux = classeur.to_vec();
            flux.resize(n_mini * 64, 0);
            let n = secteurs(flux.len());
            for k in 0..n {
                fat.push(if k + 1 == n { FIN } else { 4 + k as u32 });
            }
            flux.resize(n * 512, 0);
            s.extend(flux);
            donnees = s;
            (debut_classeur, debut_racine, taille_racine, mini_fat_debut) =
                (0, 3, (n_mini * 64) as u32, 2);
        } else {
            let n = secteurs(classeur.len());
            for k in 0..n {
                fat.push(if k + 1 == n { FIN } else { 3 + k as u32 });
            }
            donnees.extend_from_slice(classeur);
            donnees.resize(n * 512, 0);
            (debut_classeur, debut_racine, taille_racine, mini_fat_debut) = (2, FIN, 0, FIN);
        }
        fat.resize(128, LIBRE);
        let entree = |nom: &str, genre: u8, debut: u32, taille: u32| {
            let mut e = vec![0u8; 128];
            let unites: Vec<u8> = nom
                .encode_utf16()
                .chain([0])
                .flat_map(|u| u.to_le_bytes())
                .collect();
            e[..unites.len()].copy_from_slice(&unites);
            e[64..66].copy_from_slice(&(unites.len() as u16).to_le_bytes());
            e[66] = genre;
            e[68..80].copy_from_slice(&[0xFF; 12]);
            if genre == 5 {
                e[76..80].copy_from_slice(&1u32.to_le_bytes());
            }
            e[116..120].copy_from_slice(&debut.to_le_bytes());
            e[120..124].copy_from_slice(&taille.to_le_bytes());
            e
        };
        let mut repertoire = entree("Root Entry", 5, debut_racine, taille_racine);
        repertoire.extend(entree("Workbook", 2, debut_classeur, classeur.len() as u32));
        repertoire.resize(512, 0);
        let mut en_tete = vec![0u8; 512];
        en_tete[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
        en_tete[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
        en_tete[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
        en_tete[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
        en_tete[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        en_tete[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        en_tete[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes());
        en_tete[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
        en_tete[0x38..0x3C].copy_from_slice(&4096u32.to_le_bytes());
        en_tete[0x3C..0x40].copy_from_slice(&mini_fat_debut.to_le_bytes());
        en_tete[0x40..0x44].copy_from_slice(&u32::from(mini).to_le_bytes());
        en_tete[0x44..0x48].copy_from_slice(&FIN.to_le_bytes());
        for k in 0..109 {
            let v = if k == 0 { 0 } else { LIBRE };
            en_tete[0x4C + 4 * k..0x50 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        let mut f = en_tete;
        f.extend(fat.iter().flat_map(|m| m.to_le_bytes()));
        f.extend(repertoire);
        f.extend(donnees);
        f
    }

    #[test]
    fn xls_dimensions_ou_cellules_eparpillees_refuses() {
        let l = Limites::default();
        // Un classeur ordinaire passe, et calamine le lit (le conteneur fabriqué est valable),
        // dans le mini-flux comme dans des secteurs ordinaires.
        let petit = biff((0, 2, 0, 2), &[(0, 0), (1, 1)]);
        let mut grand_cellules: Vec<(u16, u16)> = (0..400).map(|r| (r, 1)).collect();
        grand_cellules.push((0, 0));
        let grand = biff((0, 400, 0, 2), &grand_cellules);
        assert!(grand.len() >= 4096);
        for classeur in [&petit, &grand] {
            let xls = cfb(classeur);
            assert_eq!(flux_xls(&xls).as_deref(), Some(&classeur[..]));
            assert_eq!(controler_avec(&l, "bl.xls", "", &xls), Ok(()));
            let feuilles = crate::lecture::tableur::apercu_classeur(&xls).expect("xls lisible");
            assert_eq!(feuilles[0].lignes[1][1], "42");
        }
        // DIMENSIONS de 4 milliards de lignes : calamine réserverait des téraoctets.
        let piege = cfb(&biff((0, u32::MAX, 0, 255), &[(0, 0)]));
        assert!(matches!(
            controler_avec(&l, "bl.xls", "", &piege),
            Err(Depassement::Cellules { .. })
        ));
        // Fin avant le début : calamine déborde.
        let piege = cfb(&biff((10, 2, 0, 2), &[(0, 0)]));
        assert!(matches!(
            controler_avec(&l, "bl.xls", "", &piege),
            Err(Depassement::Cellules { .. })
        ));
        // DIMENSIONS sage, cellules aux deux bouts.
        let piege = cfb(&biff((0, 1, 0, 1), &[(0, 0), (65_535, 65_535)]));
        assert!(matches!(
            controler_avec(&l, "bl.xls", "", &piege),
            Err(Depassement::Cellules { .. })
        ));
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
