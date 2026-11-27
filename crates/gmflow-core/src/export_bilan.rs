//! Classeur Excel du « Bilan du mois » (audit U16) : ce que montre la page `/bilan` pour un mois
//! et une société, en trois feuilles :
//! - « Synthèse » : une ligne par société (reçues, envoyées, à contrôler, échecs, doublons,
//!   montant envoyé) et le total ;
//! - « Détail » : une ligne par facture ou avoir compté dans la synthèse ;
//! - « BL en attente » : fournisseurs avec des BL du mois encore sans facture.
//!
//! Montants en vraies cellules numériques (format monétaire, avoirs en négatif comme dans le
//! montant envoyé de la page), dates en vraies dates, en-têtes figés, filtre automatique.
//! Les requêtes sont dans `gmflow-app` (`routes/bilan.rs`) : ici, rien que la mise en forme.

use rust_xlsxwriter::{
    ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError,
};

/// Une ligne de la synthèse (une société, ou les pièces pas encore rangées).
#[derive(Debug, Clone, PartialEq)]
pub struct LigneSynthese {
    pub societe: String,
    pub recues: i64,
    pub envoyees: i64,
    /// À contrôler, validées comprises (« À contrôler » de la page).
    pub a_controler: i64,
    pub en_erreur: i64,
    pub doublons: i64,
    /// TTC envoyé, avoirs en négatif.
    pub montant_envoye: f64,
}

/// Une pièce du détail.
#[derive(Debug, Clone, PartialEq)]
pub struct PieceBilan {
    pub date: Option<time::Date>,
    pub societe: String,
    pub fournisseur: Option<String>,
    /// « Facture », « Avoir ».
    pub type_piece: String,
    pub numero: Option<String>,
    /// Montants signés (avoir en négatif).
    pub ht: Option<f64>,
    pub tva: Option<f64>,
    pub ttc: Option<f64>,
    pub echeance: Option<time::Date>,
    /// « À contrôler », « Validée », « Envoyée à Dext »…
    pub etat: String,
    /// Dernier envoi réussi en comptabilité, heure de Paris.
    pub envoyee_le: Option<time::PrimitiveDateTime>,
}

/// Fournisseur avec des BL du mois sans facture.
#[derive(Debug, Clone, PartialEq)]
pub struct BlAttente {
    pub societe: String,
    pub fournisseur: String,
    pub bl: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Classeur {
    /// « Novembre 2026 ».
    pub periode: String,
    /// « La Foir'Fouille Frouard », « Toutes les sociétés ».
    pub perimetre: String,
    pub synthese: Vec<LigneSynthese>,
    pub pieces: Vec<PieceBilan>,
    pub bl_attente: Vec<BlAttente>,
}

pub const ONGLET_SYNTHESE: &str = "Synthèse";
pub const ONGLET_DETAIL: &str = "Détail";
pub const ONGLET_BL: &str = "BL en attente";

pub const COLONNES_SYNTHESE: [&str; 7] = [
    "Société",
    "Reçues",
    "Envoyées",
    "À contrôler",
    "Échec d'envoi",
    "Doublons",
    "Montant envoyé TTC",
];
pub const COLONNES_DETAIL: [&str; 11] = [
    "Date",
    "Société",
    "Fournisseur",
    "Type",
    "Numéro",
    "HT",
    "TVA",
    "TTC",
    "Échéance",
    "État",
    "Envoyée en compta le",
];
pub const COLONNES_BL: [&str; 3] = ["Société", "Fournisseur", "BL sans facture"];

/// Ligne des en-têtes des tableaux de chaque feuille (titre et sous-titre au-dessus).
pub const LIGNE_ENTETES: u32 = 3;

const MONNAIE: &str = "#,##0.00\\ \"€\";-#,##0.00\\ \"€\"";
const DATE: &str = "dd/mm/yyyy";
const DATE_HEURE: &str = "dd/mm/yyyy hh:mm";

/// Mois AAAA-MM en toutes lettres : « Novembre 2026 ».
pub fn libelle_mois(mois: &str) -> String {
    const NOMS: [&str; 12] = [
        "Janvier",
        "Février",
        "Mars",
        "Avril",
        "Mai",
        "Juin",
        "Juillet",
        "Août",
        "Septembre",
        "Octobre",
        "Novembre",
        "Décembre",
    ];
    match mois.split_once('-') {
        Some((a, m)) => match m.parse::<usize>() {
            Ok(n @ 1..=12) => format!("{} {a}", NOMS[n - 1]),
            _ => mois.to_string(),
        },
        None => mois.to_string(),
    }
}

/// État d'une pièce du bilan, avec les mots de l'interface (« À contrôler → Validée → Envoyée »).
pub fn libelle_etat(statut_depot: &str, validee: bool, outil: Option<&str>) -> String {
    match statut_depot {
        "depose" => match outil {
            Some("dext") => "Envoyée à Dext".into(),
            Some("yooz") => "Envoyée à Yooz".into(),
            _ => "Envoyée".into(),
        },
        "en_envoi" => "Envoi en cours".into(),
        "pret" if validee => "Validée".into(),
        "pret" | "a_verifier" => "À contrôler".into(),
        "erreur" => "Échec d'envoi".into(),
        "doublon" => "Doublon".into(),
        autre => autre.into(),
    }
}

/// Partie du nom de fichier : « la-foir-fouille-frouard », sans accent ni signe.
pub fn segment_nom(texte: &str) -> String {
    let mut s = String::new();
    for c in texte.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() { "societe".into() } else { s }
}

/// `bilan-<société ou toutes>-<AAAA-MM>.xlsx`.
pub fn nom_fichier(societe: Option<&str>, mois: &str) -> String {
    let qui = societe.map_or_else(|| "toutes".to_string(), segment_nom);
    format!("bilan-{qui}-{mois}.xlsx")
}

/// Date et heure d'une cellule Excel (nombre de jours depuis le 30/12/1899) : pour relire un
/// classeur produit ici (tests, contrôle d'un export).
pub fn depuis_serie_excel(v: f64) -> time::PrimitiveDateTime {
    let origine = time::macros::datetime!(1899-12-30 0:00);
    origine + time::Duration::seconds((v * 86_400.0).round() as i64)
}

fn date_excel(d: time::Date) -> Result<ExcelDateTime, XlsxError> {
    ExcelDateTime::from_ymd(d.year() as u16, u8::from(d.month()), d.day())
}

fn date_heure_excel(d: time::PrimitiveDateTime) -> Result<ExcelDateTime, XlsxError> {
    date_excel(d.date())?.and_hms(u16::from(d.hour()), d.minute(), f64::from(d.second()))
}

struct Formats {
    titre: Format,
    sous_titre: Format,
    entete: Format,
    entete_n: Format,
    monnaie: Format,
    date: Format,
    date_heure: Format,
    total: Format,
    total_n: Format,
    total_monnaie: Format,
    note: Format,
}

impl Formats {
    fn new() -> Self {
        let vert = rust_xlsxwriter::Color::RGB(0x2D6A4F);
        let fond = rust_xlsxwriter::Color::RGB(0xE8EFEA);
        let entete = Format::new()
            .set_bold()
            .set_font_color(vert)
            .set_background_color(fond)
            .set_border_bottom(FormatBorder::Thin)
            .set_text_wrap()
            .set_align(FormatAlign::VerticalCenter);
        let total = Format::new().set_bold().set_border_top(FormatBorder::Thin);
        Self {
            titre: Format::new().set_bold().set_font_size(16),
            sous_titre: Format::new().set_font_color(rust_xlsxwriter::Color::RGB(0x46514A)),
            entete_n: entete.clone().set_align(FormatAlign::Right),
            entete,
            monnaie: Format::new().set_num_format(MONNAIE),
            date: Format::new().set_num_format(DATE),
            date_heure: Format::new().set_num_format(DATE_HEURE),
            total_n: total.clone().set_align(FormatAlign::Right),
            total_monnaie: total.clone().set_num_format(MONNAIE),
            total,
            note: Format::new()
                .set_italic()
                .set_font_color(rust_xlsxwriter::Color::RGB(0x5E6962)),
        }
    }
}

/// Titre, sous-titre, en-têtes figés et répétés à l'impression, largeurs.
fn preparer(
    ws: &mut Worksheet,
    f: &Formats,
    titre: &str,
    sous_titre: &str,
    colonnes: &[&str],
    numeriques: &[usize],
    largeurs: &[f64],
) -> Result<(), XlsxError> {
    ws.write_string_with_format(0, 0, titre, &f.titre)?;
    ws.write_string_with_format(1, 0, sous_titre, &f.sous_titre)?;
    ws.set_row_height(LIGNE_ENTETES, 30)?;
    for (i, c) in colonnes.iter().enumerate() {
        let fmt = if numeriques.contains(&i) {
            &f.entete_n
        } else {
            &f.entete
        };
        ws.write_string_with_format(LIGNE_ENTETES, i as u16, *c, fmt)?;
    }
    for (i, l) in largeurs.iter().enumerate() {
        ws.set_column_width(i as u16, *l)?;
    }
    ws.set_freeze_panes(LIGNE_ENTETES + 1, 0)?;
    ws.set_repeat_rows(LIGNE_ENTETES, LIGNE_ENTETES)?;
    ws.set_landscape();
    ws.set_paper_size(9); // A4
    ws.set_print_fit_to_pages(1, 0);
    ws.set_footer("&L&A&R&P / &N");
    Ok(())
}

fn texte_ou_vide(ws: &mut Worksheet, l: u32, c: u16, v: Option<&str>) -> Result<(), XlsxError> {
    if let Some(t) = v.filter(|t| !t.is_empty()) {
        ws.write_string(l, c, t)?;
    }
    Ok(())
}

fn nombre_ou_vide(
    ws: &mut Worksheet,
    l: u32,
    c: u16,
    v: Option<f64>,
    fmt: &Format,
) -> Result<(), XlsxError> {
    if let Some(x) = v {
        ws.write_number_with_format(l, c, x, fmt)?;
    }
    Ok(())
}

fn synthese(wb: &mut Workbook, c: &Classeur, f: &Formats) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(ONGLET_SYNTHESE)?;
    preparer(
        ws,
        f,
        &format!("Bilan du mois : {}", c.periode),
        &format!(
            "{} · factures et avoirs datés du mois (date du document, à défaut du mail)",
            c.perimetre
        ),
        &COLONNES_SYNTHESE,
        &[1, 2, 3, 4, 5, 6],
        &[34.0, 11.0, 11.0, 13.0, 14.0, 11.0, 20.0],
    )?;
    let mut l = LIGNE_ENTETES + 1;
    for s in &c.synthese {
        ws.write_string(l, 0, &s.societe)?;
        for (i, n) in [s.recues, s.envoyees, s.a_controler, s.en_erreur, s.doublons]
            .into_iter()
            .enumerate()
        {
            ws.write_number(l, i as u16 + 1, n as f64)?;
        }
        ws.write_number_with_format(l, 6, s.montant_envoye, &f.monnaie)?;
        l += 1;
    }
    // Total : des nombres (pas des formules), justes même sans recalcul à l'ouverture.
    ws.write_string_with_format(l, 0, "Total", &f.total)?;
    let somme = |g: fn(&LigneSynthese) -> i64| c.synthese.iter().map(g).sum::<i64>() as f64;
    let totaux = [
        somme(|s| s.recues),
        somme(|s| s.envoyees),
        somme(|s| s.a_controler),
        somme(|s| s.en_erreur),
        somme(|s| s.doublons),
    ];
    for (i, n) in totaux.into_iter().enumerate() {
        ws.write_number_with_format(l, i as u16 + 1, n, &f.total_n)?;
    }
    let montant: f64 = c.synthese.iter().map(|s| s.montant_envoye).sum();
    ws.write_number_with_format(l, 6, (montant * 100.0).round() / 100.0, &f.total_monnaie)?;
    l += 2;
    for n in [
        "« Reçues » ne compte pas les doublons. « À contrôler » compte aussi les factures validées qui attendent leur envoi.",
        "Montant envoyé : TTC des pièces envoyées à Dext ou Yooz, avoirs déduits.",
        "Une ligne par pièce dans la feuille « Détail » ; BL du mois sans facture dans « BL en attente ».",
    ] {
        ws.write_string_with_format(l, 0, n, &f.note)?;
        l += 1;
    }
    Ok(())
}

fn detail(wb: &mut Workbook, c: &Classeur, f: &Formats) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(ONGLET_DETAIL)?;
    preparer(
        ws,
        f,
        &format!("Pièces du mois : {}", c.periode),
        &format!(
            "{} · {} pièce{} · avoirs en négatif",
            c.perimetre,
            c.pieces.len(),
            if c.pieces.len() > 1 { "s" } else { "" }
        ),
        &COLONNES_DETAIL,
        &[5, 6, 7],
        &[
            12.0, 28.0, 30.0, 10.0, 18.0, 14.0, 13.0, 14.0, 12.0, 18.0, 18.0,
        ],
    )?;
    let mut l = LIGNE_ENTETES + 1;
    for p in &c.pieces {
        if let Some(d) = p.date {
            ws.write_datetime_with_format(l, 0, date_excel(d)?, &f.date)?;
        }
        ws.write_string(l, 1, &p.societe)?;
        texte_ou_vide(ws, l, 2, p.fournisseur.as_deref())?;
        ws.write_string(l, 3, &p.type_piece)?;
        texte_ou_vide(ws, l, 4, p.numero.as_deref())?;
        nombre_ou_vide(ws, l, 5, p.ht, &f.monnaie)?;
        nombre_ou_vide(ws, l, 6, p.tva, &f.monnaie)?;
        nombre_ou_vide(ws, l, 7, p.ttc, &f.monnaie)?;
        if let Some(d) = p.echeance {
            ws.write_datetime_with_format(l, 8, date_excel(d)?, &f.date)?;
        }
        ws.write_string(l, 9, &p.etat)?;
        if let Some(d) = p.envoyee_le {
            ws.write_datetime_with_format(l, 10, date_heure_excel(d)?, &f.date_heure)?;
        }
        l += 1;
    }
    let derniere = (l - 1).max(LIGNE_ENTETES);
    ws.autofilter(LIGNE_ENTETES, 0, derniere, COLONNES_DETAIL.len() as u16 - 1)?;
    Ok(())
}

fn bl_attente(wb: &mut Workbook, c: &Classeur, f: &Formats) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(ONGLET_BL)?;
    preparer(
        ws,
        f,
        &format!("BL en attente de facture : {}", c.periode),
        &format!("{} · BL datés du mois sans facture rapprochée", c.perimetre),
        &COLONNES_BL,
        &[2],
        &[34.0, 34.0, 16.0],
    )?;
    let mut l = LIGNE_ENTETES + 1;
    for b in &c.bl_attente {
        ws.write_string(l, 0, &b.societe)?;
        ws.write_string(l, 1, &b.fournisseur)?;
        ws.write_number(l, 2, b.bl as f64)?;
        l += 1;
    }
    if c.bl_attente.is_empty() {
        ws.write_string_with_format(l, 0, "Aucun BL du mois sans sa facture.", &f.note)?;
    } else {
        ws.write_string_with_format(l, 0, "Total", &f.total)?;
        ws.write_string_with_format(l, 1, "", &f.total)?;
        let n: i64 = c.bl_attente.iter().map(|b| b.bl).sum();
        ws.write_number_with_format(l, 2, n as f64, &f.total_n)?;
    }
    Ok(())
}

/// Le classeur `.xlsx`, en octets.
pub fn xlsx(c: &Classeur) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let f = Formats::new();
    synthese(&mut wb, c, &f)?;
    detail(&mut wb, c, &f)?;
    bl_attente(&mut wb, c, &f)?;
    let proprietes = rust_xlsxwriter::DocProperties::new()
        .set_title(format!("Bilan du mois : {}", c.periode))
        .set_subject(c.perimetre.clone())
        .set_author("GMFlow");
    wb.set_properties(&proprietes);
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Reader, Xlsx};
    use std::io::Cursor;
    use time::macros::{date, datetime};

    fn classeur() -> Classeur {
        Classeur {
            periode: "Novembre 2026".into(),
            perimetre: "Toutes les sociétés".into(),
            synthese: vec![
                LigneSynthese {
                    societe: "Frouard".into(),
                    recues: 2,
                    envoyees: 1,
                    a_controler: 1,
                    en_erreur: 0,
                    doublons: 0,
                    montant_envoye: 120.5,
                },
                LigneSynthese {
                    societe: "Houdemont".into(),
                    recues: 1,
                    envoyees: 1,
                    a_controler: 0,
                    en_erreur: 0,
                    doublons: 1,
                    montant_envoye: -20.25,
                },
            ],
            pieces: vec![PieceBilan {
                date: Some(date!(2026 - 11 - 03)),
                societe: "Frouard".into(),
                fournisseur: Some("Durand".into()),
                type_piece: "Facture".into(),
                numero: Some("F-1".into()),
                ht: Some(100.42),
                tva: Some(20.08),
                ttc: Some(120.5),
                echeance: Some(date!(2026 - 12 - 31)),
                etat: "Envoyée à Dext".into(),
                envoyee_le: Some(datetime!(2026-11-04 9:30)),
            }],
            bl_attente: vec![],
        }
    }

    #[test]
    fn classeur_lisible_avec_vrais_nombres_et_dates() {
        let octets = xlsx(&classeur()).unwrap();
        let mut x: Xlsx<_> = Xlsx::new(Cursor::new(octets)).unwrap();
        assert_eq!(
            x.sheet_names(),
            vec![ONGLET_SYNTHESE, ONGLET_DETAIL, ONGLET_BL]
        );
        let s = x.worksheet_range(ONGLET_SYNTHESE).unwrap();
        let l = LIGNE_ENTETES as usize;
        assert_eq!(
            s.get_value((l as u32, 0)),
            Some(&Data::String("Société".into()))
        );
        assert_eq!(s.get_value((l as u32 + 1, 1)), Some(&Data::Float(2.0)));
        // Total : 3 reçues, 100,25 € envoyés.
        assert_eq!(
            s.get_value((l as u32 + 3, 0)),
            Some(&Data::String("Total".into()))
        );
        assert_eq!(s.get_value((l as u32 + 3, 1)), Some(&Data::Float(3.0)));
        assert_eq!(s.get_value((l as u32 + 3, 6)), Some(&Data::Float(100.25)));
        let d = x.worksheet_range(ONGLET_DETAIL).unwrap();
        let ligne = l as u32 + 1;
        match d.get_value((ligne, 0)) {
            Some(Data::DateTime(v)) => {
                assert_eq!(depuis_serie_excel(v.as_f64()), datetime!(2026-11-03 0:00))
            }
            v => panic!("date attendue : {v:?}"),
        }
        assert_eq!(d.get_value((ligne, 7)), Some(&Data::Float(120.5)));
        match d.get_value((ligne, 10)) {
            Some(Data::DateTime(v)) => {
                assert_eq!(depuis_serie_excel(v.as_f64()), datetime!(2026-11-04 9:30))
            }
            v => panic!("date et heure attendues : {v:?}"),
        }
    }

    #[test]
    fn noms_et_libelles() {
        assert_eq!(
            nom_fichier(Some("La Foir'Fouille Frouard"), "2026-11"),
            "bilan-la-foir-fouille-frouard-2026-11.xlsx"
        );
        assert_eq!(nom_fichier(None, "2026-01"), "bilan-toutes-2026-01.xlsx");
        assert_eq!(segment_nom("Société Générale ÉTÉ"), "societe-generale-ete");
        assert_eq!(libelle_mois("2026-08"), "Août 2026");
        assert_eq!(libelle_etat("depose", true, Some("dext")), "Envoyée à Dext");
        assert_eq!(libelle_etat("pret", false, None), "À contrôler");
        assert_eq!(libelle_etat("pret", true, None), "Validée");
    }
}
