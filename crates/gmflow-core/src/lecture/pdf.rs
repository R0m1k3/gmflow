//! PDF : XML Factur-X embarqué (lopdf) et couche texte (pdf-extract).

use lopdf::{Dictionary, Document, Object};

/// Noms de fichier du XML d'une facture électronique embarquée dans un PDF/A-3.
const NOMS_FACTURX: &[&str] = &[
    "factur-x.xml",
    "zugferd-invoice.xml",
    "xrechnung.xml",
    "order-x.xml",
];

fn chaine(o: &Object) -> Option<String> {
    match o {
        Object::String(octets, _) => {
            // UTF-16BE avec BOM, sinon octets bruts (PDFDocEncoding ~ latin-1).
            if octets.starts_with(&[0xFE, 0xFF]) {
                let u: Vec<u16> = octets[2..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                Some(String::from_utf16_lossy(&u))
            } else {
                Some(octets.iter().map(|&b| b as char).collect())
            }
        }
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => None,
    }
}

fn dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(o).ok()?.1.as_dict().ok()
}

/// Contenu du fichier décrit par une spécification de fichier (/EF /F ou /UF).
fn contenu_filespec(doc: &Document, fs: &Dictionary) -> Option<Vec<u8>> {
    let ef = dict(doc, fs.get(b"EF").ok()?)?;
    let flux = ef.get(b"F").or_else(|_| ef.get(b"UF")).ok()?;
    let (_, obj) = doc.dereference(flux).ok()?;
    let s = obj.as_stream().ok()?;
    s.decompressed_content()
        .ok()
        .or_else(|| Some(s.content.clone()))
}

fn nom_filespec(doc: &Document, fs: &Dictionary) -> Option<String> {
    for cle in [&b"UF"[..], b"F"] {
        if let Ok(o) = fs.get(cle)
            && let Ok((_, o)) = doc.dereference(o)
            && let Some(n) = chaine(o)
        {
            return Some(n);
        }
    }
    None
}

/// Parcours de l'arbre de noms /EmbeddedFiles (Names et Kids), avec une profondeur bornée.
fn parcourir(
    doc: &Document,
    noeud: &Dictionary,
    profondeur: u8,
    sortie: &mut Vec<(String, Vec<u8>)>,
) {
    if profondeur > 8 {
        return;
    }
    if let Ok(Object::Array(noms)) = noeud
        .get(b"Names")
        .and_then(|o| doc.dereference(o).map(|(_, o)| o))
    {
        for paire in noms.chunks(2) {
            let [cle, valeur] = paire else { continue };
            let Some(fs) = dict(doc, valeur) else {
                continue;
            };
            let nom = nom_filespec(doc, fs)
                .or_else(|| chaine(cle))
                .unwrap_or_default();
            if let Some(c) = contenu_filespec(doc, fs) {
                sortie.push((nom, c));
            }
        }
    }
    if let Ok(Object::Array(enfants)) = noeud
        .get(b"Kids")
        .and_then(|o| doc.dereference(o).map(|(_, o)| o))
    {
        for e in enfants {
            if let Some(d) = dict(doc, e) {
                parcourir(doc, d, profondeur + 1, sortie);
            }
        }
    }
}

/// Fichiers embarqués d'un PDF (nom, contenu).
pub fn fichiers_embarques(pdf: &[u8]) -> Vec<(String, Vec<u8>)> {
    let Ok(doc) = Document::load_mem(pdf) else {
        return Vec::new();
    };
    let Ok(catalogue) = doc.catalog() else {
        return Vec::new();
    };
    let mut sortie = Vec::new();
    if let Ok(noms) = catalogue.get(b"Names")
        && let Some(noms) = dict(&doc, noms)
        && let Ok(ef) = noms.get(b"EmbeddedFiles")
        && let Some(ef) = dict(&doc, ef)
    {
        parcourir(&doc, ef, 0, &mut sortie);
    }
    if sortie.is_empty()
        && let Ok(af) = catalogue.get(b"AF")
        && let Ok((_, Object::Array(af))) = doc.dereference(af)
    {
        for fs in af {
            if let Some(fs) = dict(&doc, fs)
                && let Some(c) = contenu_filespec(&doc, fs)
            {
                sortie.push((nom_filespec(&doc, fs).unwrap_or_default(), c));
            }
        }
    }
    sortie
}

/// XML de facture électronique embarqué (Factur-X, ZUGFeRD), s'il y en a un.
pub fn xml_facturx(pdf: &[u8]) -> Option<Vec<u8>> {
    let fichiers = fichiers_embarques(pdf);
    let mut xml: Vec<(String, Vec<u8>)> = fichiers
        .into_iter()
        .filter(|(n, _)| n.to_lowercase().ends_with(".xml"))
        .collect();
    xml.sort_by_key(|(n, _)| !NOMS_FACTURX.contains(&n.to_lowercase().as_str()));
    xml.into_iter().next().map(|(_, c)| c)
}

/// Couche texte du PDF. pdf-extract panique sur certains fichiers : l'appelant doit tourner
/// dans `spawn_blocking`, la panique est convertie ici en erreur.
pub fn texte(pdf: &[u8]) -> Option<String> {
    let pdf = pdf.to_vec();
    std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&pdf).ok())
        .ok()
        .flatten()
}

/// Couche texte page par page (même garde contre les paniques que `texte`).
pub fn texte_par_page(pdf: &[u8]) -> Option<Vec<String>> {
    let pdf = pdf.to_vec();
    std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem_by_pages(&pdf).ok())
        .ok()
        .flatten()
}

/// Nombre de pages, 0 si le PDF est illisible.
pub fn pages(pdf: &[u8]) -> usize {
    Document::load_mem(pdf)
        .map(|d| d.get_pages().len())
        .unwrap_or(0)
}
