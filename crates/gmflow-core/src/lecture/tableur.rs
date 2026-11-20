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

/// Texte d'un classeur : une section par feuille, cellules séparées par « ; ».
pub fn texte_classeur(octets: &[u8]) -> Option<String> {
    let mut classeur = open_workbook_auto_from_rs(Cursor::new(octets.to_vec())).ok()?;
    let mut sortie = String::new();
    for nom in classeur.sheet_names().into_iter().take(FEUILLES_MAX) {
        let Ok(plage) = classeur.worksheet_range(&nom) else {
            continue;
        };
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
    Some(sortie)
}

/// Texte d'un CSV : UTF-8, sinon Windows-1252 (exports de logiciels de caisse).
pub fn texte_csv(octets: &[u8]) -> String {
    let texte = match std::str::from_utf8(octets) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => octets.iter().map(|&b| windows_1252(b)).collect(),
    };
    texte
        .lines()
        .take(LIGNES_MAX * 2)
        .collect::<Vec<_>>()
        .join("\n")
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
fn date_excel(serie: f64) -> String {
    let base = time::macros::date!(1899 - 12 - 30);
    match base.checked_add(time::Duration::days(serie.floor() as i64)) {
        Some(d) => format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()),
        None => format!("{serie}"),
    }
}
