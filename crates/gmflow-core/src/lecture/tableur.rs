//! Tableurs (xlsx, xls, ods) et CSV, mis en texte pour Claude.

use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};

const FEUILLES_MAX: usize = 5;
const LIGNES_MAX: usize = 400;

fn cellule(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().replace(['\n', '\r', ';'], " "),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => date_excel(dt.as_f64()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(_) => String::new(),
    }
}

fn ligne_vide(ligne: &[Data]) -> bool {
    ligne.iter().all(|c| cellule(c).is_empty())
}

/// Texte d'un classeur : une section par feuille, cellules séparées par « ; ». Vrai en second :
/// des cellules non vides ont été laissées de côté (au-delà de `LIGNES_MAX` lignes ou de
/// `FEUILLES_MAX` feuilles).
pub fn texte_classeur(octets: &[u8]) -> Option<(String, bool)> {
    let mut classeur = open_workbook_auto_from_rs(Cursor::new(octets.to_vec())).ok()?;
    let mut sortie = String::new();
    let mut coupe = false;
    let noms = classeur.sheet_names();
    for nom in noms.iter().skip(FEUILLES_MAX) {
        if let Ok(plage) = classeur.worksheet_range(nom)
            && plage.rows().any(|l| !ligne_vide(l))
        {
            coupe = true;
        }
    }
    for nom in noms.into_iter().take(FEUILLES_MAX) {
        let Ok(plage) = classeur.worksheet_range(&nom) else {
            continue;
        };
        if plage.rows().skip(LIGNES_MAX).any(|l| !ligne_vide(l)) {
            coupe = true;
        }
        sortie.push_str(&format!("## Feuille {nom}\n"));
        for ligne in plage.rows().take(LIGNES_MAX) {
            let cellules: Vec<String> = ligne.iter().map(cellule).collect();
            if cellules.iter().all(String::is_empty) {
                continue;
            }
            let fin = cellules.iter().rposition(|c| !c.is_empty()).unwrap_or(0);
            sortie.push_str(&cellules[..=fin].join(";"));
            sortie.push('\n');
        }
    }
    Some((sortie, coupe))
}

/// Feuille d'un tableur pour l'aperçu à l'écran.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Feuille {
    pub nom: String,
    pub lignes: Vec<Vec<String>>,
    /// Lignes ou colonnes coupées (aperçu limité).
    pub tronque: bool,
}

const APERCU_LIGNES: usize = 500;
const APERCU_COLONNES: usize = 40;

fn rogner(lignes: Vec<Vec<String>>) -> Feuille {
    let mut tronque = lignes.len() > APERCU_LIGNES;
    let mut v: Vec<Vec<String>> = lignes.into_iter().take(APERCU_LIGNES).collect();
    // Colonnes vides à droite retirées, au plus APERCU_COLONNES.
    let largeur = v
        .iter()
        .map(|l| l.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1))
        .max()
        .unwrap_or(0);
    if largeur > APERCU_COLONNES {
        tronque = true;
    }
    let largeur = largeur.min(APERCU_COLONNES);
    for l in &mut v {
        l.resize(largeur, String::new());
    }
    // Lignes vides de fin retirées.
    while v.last().is_some_and(|l| l.iter().all(String::is_empty)) {
        v.pop();
    }
    Feuille {
        nom: String::new(),
        lignes: v,
        tronque,
    }
}

/// Feuilles d'un classeur (xlsx, xls, ods) pour l'aperçu, ou rien s'il ne s'ouvre pas.
pub fn apercu_classeur(octets: &[u8]) -> Option<Vec<Feuille>> {
    let mut classeur = open_workbook_auto_from_rs(Cursor::new(octets.to_vec())).ok()?;
    let mut v = Vec::new();
    for nom in classeur.sheet_names().into_iter().take(FEUILLES_MAX) {
        let Ok(plage) = classeur.worksheet_range(&nom) else {
            continue;
        };
        let mut f = rogner(
            plage
                .rows()
                .take(APERCU_LIGNES + 1)
                .map(|l| l.iter().map(cellule).collect())
                .collect(),
        );
        f.nom = nom;
        v.push(f);
    }
    Some(v)
}

/// Séparateur d'un CSV (« ; », tabulation, « , » ou « | ») deviné sur ses 30 premières lignes
/// non vides : celui qui découpe le plus de lignes en un même nombre de colonnes (au moins 2).
pub(crate) fn separateur_csv(texte: &str) -> char {
    let lignes: Vec<&str> = texte
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(30)
        .collect();
    let mut meilleur = (';', 0usize);
    for sep in [';', '\t', ',', '|'] {
        let mut comptes: std::collections::HashMap<usize, usize> = Default::default();
        for l in &lignes {
            let n = decouper_csv(l, sep).len();
            if n > 1 {
                *comptes.entry(n).or_default() += 1;
            }
        }
        // Lignes au nombre de colonnes le plus fréquent, départagées par ce nombre.
        let score = comptes.iter().map(|(n, k)| k * 1000 + n).max().unwrap_or(0);
        if score > meilleur.1 {
            meilleur = (sep, score);
        }
    }
    meilleur.0
}

/// Un CSV en une feuille : séparateur deviné (« ; », tabulation, « , » ou « | »), guillemets gérés.
pub fn apercu_csv(octets: &[u8]) -> Feuille {
    let (texte, _) = texte_csv(octets);
    let sep = separateur_csv(&texte);
    let lignes = texte
        .lines()
        .take(APERCU_LIGNES + 1)
        .map(|l| decouper_csv(l, sep))
        .collect();
    let mut f = rogner(lignes);
    f.nom = "CSV".into();
    f
}

pub(crate) fn decouper_csv(ligne: &str, sep: char) -> Vec<String> {
    let mut v = Vec::new();
    let mut cur = String::new();
    let mut guillemets = false;
    let mut it = ligne.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if guillemets && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => guillemets = !guillemets,
            c if c == sep && !guillemets => v.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    v.push(cur.trim().to_string());
    v
}

/// Texte d'un CSV : UTF-16 avec BOM (exports Excel « Texte Unicode »), UTF-8, sinon
/// Windows-1252 (exports de logiciels de caisse).
pub(crate) fn decoder_csv(octets: &[u8]) -> String {
    let utf16 = |reste: &[u8], be: bool| -> String {
        let u: Vec<u16> = reste
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if be {
                    u16::from_be_bytes(*c)
                } else {
                    u16::from_le_bytes(*c)
                }
            })
            .collect();
        String::from_utf16_lossy(&u)
    };
    if let Some(reste) = octets.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(reste, false);
    }
    if let Some(reste) = octets.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(reste, true);
    }
    match std::str::from_utf8(octets) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => octets.iter().map(|&b| windows_1252(b)).collect(),
    }
}

/// Texte d'un CSV (`decoder_csv`). Vrai en second : des lignes non vides ont été laissées de
/// côté (au-delà de 2 × `LIGNES_MAX`).
pub fn texte_csv(octets: &[u8]) -> (String, bool) {
    let texte = decoder_csv(octets);
    let coupe = texte
        .lines()
        .skip(LIGNES_MAX * 2)
        .any(|l| !l.trim().trim_matches([';', ',', '\t']).is_empty());
    let t = texte
        .lines()
        .take(LIGNES_MAX * 2)
        .collect::<Vec<_>>()
        .join("\n");
    (t, coupe)
}

fn windows_1252(b: u8) -> char {
    const TABLE: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    if (0x80..0xA0).contains(&b) {
        TABLE[(b - 0x80) as usize]
    } else {
        b as char
    }
}

/// Numéro de série Excel (jours depuis le 30/12/1899) -> AAAA-MM-JJ.
pub(crate) fn date_excel(serie: f64) -> String {
    let base = time::macros::date!(1899 - 12 - 30);
    match base.checked_add(time::Duration::days(serie.floor() as i64)) {
        Some(d) => format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()),
        None => format!("{serie}"),
    }
}

#[cfg(test)]
mod tests_apercu {
    use super::*;

    #[test]
    fn csv() {
        let f = apercu_csv("Réf;Désignation;Qté\n12;\"Bol; bleu\";3\n\n".as_bytes());
        assert_eq!(f.lignes.len(), 2);
        assert_eq!(f.lignes[1], vec!["12", "Bol; bleu", "3"]);
        let f = apercu_csv(b"a,b\n1,2\n");
        assert_eq!(f.lignes[1], vec!["1", "2"]);
        // Titre sans séparateur en première ligne : deviné sur les suivantes.
        let f = apercu_csv(b"Bon de livraison 12, Paris\nref\tqte\tprix\nA\t1\t2,5\nB\t3\t4\n");
        assert_eq!(f.lignes[2], vec!["A", "1", "2,5"]);
    }

    #[test]
    fn csv_utf16() {
        let mut octets = vec![0xFF, 0xFE];
        for u in "Réf;Qté\nA1;3\n".encode_utf16() {
            octets.extend(u.to_le_bytes());
        }
        assert_eq!(decoder_csv(&octets), "Réf;Qté\nA1;3\n");
        let f = apercu_csv(&octets);
        assert_eq!(f.lignes[0], vec!["Réf", "Qté"]);
        let mut octets = vec![0xFE, 0xFF];
        for u in "é".encode_utf16() {
            octets.extend(u.to_be_bytes());
        }
        assert_eq!(decoder_csv(&octets), "é");
    }
}

#[cfg(test)]
mod tests_coupure {
    use super::*;

    #[test]
    fn csv_coupe_signale() {
        let court = "a;b\n1;2\n;;\n".repeat(10);
        assert!(!texte_csv(court.as_bytes()).1);
        let long = "a;b\n".repeat(LIGNES_MAX * 2 + 1);
        assert!(texte_csv(long.as_bytes()).1);
        // Lignes vides au-delà : rien de perdu.
        let vide = format!("{}{}", "a;b\n".repeat(LIGNES_MAX * 2), ";;\n".repeat(50));
        assert!(!texte_csv(vide.as_bytes()).1);
    }
}
