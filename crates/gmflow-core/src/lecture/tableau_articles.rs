//! Lignes d'articles d'un tableur (xlsx, xls, ods, csv) lues dans les cellules par le code, en
//! nombres exacts et sans plafond de lignes. Claude ne lit plus que l'en-tête du document (type,
//! numéro, date, fournisseur, totaux) sur un extrait court : le haut de la feuille et la zone
//! des totaux.
//!
//! Le tableau est repéré par sa ligne d'en-tête (intitulés connus : `SYNONYMES`). Si elle n'est
//! pas reconnue avec assez de certitude, le worker demande une fois à Claude la correspondance
//! colonnes → champs (`prompt_correspondance`), la valide (`appliquer`) et la garde sur la fiche
//! fournisseur, sous l'empreinte des intitulés (`empreinte`).

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use calamine::{Data, Reader, SheetVisible, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::extraction::Ligne;
use super::tableur;

/// Clé de `document.confiance` : d'où viennent les lignes (« cellules » : lues par le code).
pub const LIGNES_SOURCE: &str = "lignes_source";
/// Clé de `document.resultat` : correspondance donnée par Claude, à garder sur la fiche
/// fournisseur une fois le document rattaché.
pub const COLONNES_APPRISES: &str = "colonnes_tableur";

/// Lignes du haut d'une feuille où chercher l'en-tête.
const ENTETE_CHERCHEE: usize = 60;
/// Lignes non vides montrées à Claude pour qu'il désigne les colonnes.
pub const LIGNES_POUR_CLAUDE: usize = 15;
/// Lignes gardées avant l'en-tête et après le tableau dans l'extrait donné à Claude.
const EXTRAIT_HAUT: usize = 40;
const EXTRAIT_BAS: usize = 40;
/// Longueur maximale de l'extrait (le prompt le coupe de toute façon à `TEXTE_MAX`).
const EXTRAIT_MAX: usize = 12_000;

/// Valeur d'une cellule.
#[derive(Debug, Clone, PartialEq)]
pub enum Valeur {
    Vide,
    Texte(String),
    Nombre(f64),
    /// Cellule en erreur (`#REF!`, `#N/A`…) : valeur absente, signalée.
    Erreur(String),
}

impl Valeur {
    fn depuis(d: &Data) -> Self {
        match d {
            Data::Empty => Valeur::Vide,
            Data::String(s) => texte_ou_erreur(s),
            Data::Float(f) => Valeur::Nombre(*f),
            Data::Int(i) => Valeur::Nombre(*i as f64),
            Data::Bool(b) => Valeur::Texte(b.to_string()),
            Data::DateTime(dt) => Valeur::Texte(tableur::date_excel(dt.as_f64())),
            Data::DateTimeIso(s) | Data::DurationIso(s) => texte_ou_erreur(s),
            Data::Error(e) => Valeur::Erreur(e.to_string()),
        }
    }

    /// Texte affiché (extrait pour Claude, texte du document).
    pub fn texte(&self) -> String {
        match self {
            Valeur::Vide => String::new(),
            Valeur::Texte(s) => s.replace(['\n', '\r', ';'], " "),
            Valeur::Nombre(f) => nombre_affiche(*f),
            Valeur::Erreur(e) => e.clone(),
        }
    }

    fn est_vide(&self) -> bool {
        matches!(self, Valeur::Vide)
    }

    /// Nombre de la cellule : nombre du tableur, ou texte « 1 234,56 », « 12,5 € », « 20 % ».
    fn nombre(&self) -> Option<f64> {
        match self {
            Valeur::Nombre(f) => Some(*f),
            Valeur::Texte(s) => nombre_texte(s).map(|(v, _)| v),
            _ => None,
        }
    }
}

const ERREURS: &[&str] = &[
    "#REF!", "#N/A", "#VALUE!", "#DIV/0!", "#NAME?", "#NUM!", "#NULL!", "#DATA!",
];

fn texte_ou_erreur(s: &str) -> Valeur {
    let t = s.trim();
    if t.is_empty() {
        Valeur::Vide
    } else if ERREURS.contains(&t.to_uppercase().as_str()) {
        Valeur::Erreur(t.to_uppercase())
    } else {
        Valeur::Texte(t.to_string())
    }
}

fn nombre_affiche(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{}", arrondi(f))
    }
}

/// Arrondi à 6 décimales : efface le bruit des formules (14,900000000000002).
fn arrondi(f: f64) -> f64 {
    (f * 1e6).round() / 1e6
}

/// Nombre écrit en texte : « 1 234,56 », « 1.234,56 », « 1,234.56 », « 12,5 € », « -3 »,
/// « (12,00) », « 12,00- », « 20 % ». Vrai en second : suivi de « % ».
pub fn nombre_texte(s: &str) -> Option<(f64, bool)> {
    let mut t: String = s
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'' | '€'))
        .collect();
    for suffixe in ["EUR", "eur", "HT", "ht"] {
        if let Some(r) = t.strip_suffix(suffixe) {
            t = r.to_string();
        }
    }
    let pourcent = t.ends_with('%');
    let t = t.trim_end_matches('%');
    let (negatif, t) = if let Some(r) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        (true, r)
    } else if let Some(r) = t.strip_suffix('-') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('-') {
        (true, r)
    } else {
        (false, t.strip_prefix('+').unwrap_or(t))
    };
    if t.is_empty()
        || !t
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        || !t.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    // Le dernier séparateur est la virgule décimale, les autres séparent les milliers.
    let v: f64 = match t.rfind(['.', ',']) {
        None => t.parse().ok()?,
        Some(pos) => {
            let entier: String = t[..pos].chars().filter(char::is_ascii_digit).collect();
            let dec = &t[pos + 1..];
            if dec.contains(['.', ',']) {
                return None;
            }
            let e = if entier.is_empty() { "0" } else { &entier };
            format!("{e}.{dec}").parse().ok()?
        }
    };
    Some((if negatif { -v } else { v }, pourcent))
}

/// Une feuille : nom et lignes de cellules.
#[derive(Debug, Clone, PartialEq)]
pub struct Grille {
    pub nom: String,
    pub lignes: Vec<Vec<Valeur>>,
}

impl Grille {
    fn cellule(&self, l: usize, c: usize) -> &Valeur {
        self.lignes
            .get(l)
            .and_then(|r| r.get(c))
            .unwrap_or(&Valeur::Vide)
    }

    fn largeur(&self) -> usize {
        self.lignes.iter().map(Vec::len).max().unwrap_or(0)
    }

    fn ligne_texte(&self, l: usize) -> String {
        let Some(r) = self.lignes.get(l) else {
            return String::new();
        };
        let fin = r.iter().rposition(|v| !v.est_vide()).map_or(0, |i| i + 1);
        r[..fin]
            .iter()
            .map(Valeur::texte)
            .collect::<Vec<_>>()
            .join(";")
    }

    fn ligne_vide(&self, l: usize) -> bool {
        self.lignes
            .get(l)
            .is_none_or(|r| r.iter().all(Valeur::est_vide))
    }
}

/// Feuilles visibles d'un classeur (les feuilles masquées sont ignorées), ou rien s'il ne
/// s'ouvre pas.
pub fn grilles_classeur(octets: &[u8]) -> Option<Vec<Grille>> {
    let mut classeur = open_workbook_auto_from_rs(Cursor::new(octets.to_vec())).ok()?;
    let visibles: Vec<String> = classeur
        .sheets_metadata()
        .iter()
        .filter(|s| s.visible == SheetVisible::Visible)
        .map(|s| s.name.clone())
        .collect();
    let mut v = Vec::new();
    for nom in visibles {
        let Ok(plage) = classeur.worksheet_range(&nom) else {
            continue;
        };
        let lignes = plage
            .rows()
            .map(|r| r.iter().map(Valeur::depuis).collect())
            .collect();
        v.push(Grille { nom, lignes });
    }
    Some(v)
}

/// Un CSV en une grille (encodage et séparateur devinés).
pub fn grille_csv(octets: &[u8]) -> Grille {
    let texte = tableur::decoder_csv(octets);
    let sep = tableur::separateur_csv(&texte);
    Grille {
        nom: "CSV".into(),
        lignes: texte
            .lines()
            .map(|l| {
                tableur::decouper_csv(l, sep)
                    .iter()
                    .map(|c| texte_ou_erreur(c))
                    .collect()
            })
            .collect(),
    }
}

/// Champs d'une ligne d'article (noms de `extraction::Ligne`).
pub const CHAMPS: &[&str] = &[
    "reference",
    "gencod",
    "designation",
    "quantite",
    "unite",
    "colis",
    "pcb",
    "prix_unitaire_ht",
    "montant_ht",
    "remise_taux",
    "taux_tva",
    "deee_ht",
    "ecomob_ht",
    "pv_conseille",
];

/// Intitulés de colonne reconnus, normalisés (`normaliser`), par champ.
const SYNONYMES: &[(&str, &[&str])] = &[
    (
        "reference",
        &[
            "ref",
            "reference",
            "ref article",
            "reference article",
            "ref fournisseur",
            "reference fournisseur",
            "ref fourn",
            "ref fou",
            "code article",
            "code produit",
            "ref produit",
            "reference produit",
            "n article",
            "no article",
            "num article",
            "sku",
            "code",
            "article code",
            "ref frs",
            "ref interne",
        ],
    ),
    (
        "gencod",
        &[
            "gencod",
            "gencode",
            "ean",
            "ean13",
            "ean 13",
            "code ean",
            "code ean13",
            "code barre",
            "code barres",
            "codebarre",
            "codebarres",
            "gtin",
            "upc",
            "ean ucv",
            "ean uvc",
            "gencod uvc",
            "ean article",
        ],
    ),
    (
        "designation",
        &[
            "designation",
            "libelle",
            "description",
            "produit",
            "nom produit",
            "intitule",
            "libelle article",
            "designation article",
            "nom article",
            "libelle produit",
            "designation produit",
            "article",
        ],
    ),
    (
        "quantite",
        &[
            "quantite",
            "qte",
            "qty",
            "qt",
            "quantite livree",
            "qte livree",
            "qte liv",
            "quantite expediee",
            "qte expediee",
            "qte uvc",
            "quantite uvc",
            "nb unites",
            "unites",
            "nombre",
            "quantite facturee",
            "qte facturee",
            "qte fact",
            "quantites",
            "nb uvc",
            "uvc",
            "qte totale",
        ],
    ),
    (
        "unite",
        &[
            "unite",
            "u",
            "unite de vente",
            "uv",
            "conditionnement unite",
        ],
    ),
    (
        "colis",
        &[
            "colis",
            "nb colis",
            "nombre de colis",
            "qte colis",
            "cartons",
            "nb cartons",
            "carton",
            "nb col",
            "nbre colis",
            "quantite colis",
            "col",
        ],
    ),
    (
        "pcb",
        &[
            "pcb",
            "colisage",
            "par",
            "uvc par colis",
            "unites par colis",
            "qte par colis",
            "spcb",
            "conditionnement",
            "cond",
            "uvc colis",
            "par colis",
            "colis de",
        ],
    ),
    (
        "prix_unitaire_ht",
        &[
            "pu",
            "pu ht",
            "prix unitaire",
            "prix unitaire ht",
            "prix ht",
            "prix",
            "p u",
            "p u ht",
            "prix net",
            "pu net",
            "prix brut",
            "pu brut",
            "tarif",
            "prix achat",
            "pa",
            "pa ht",
            "prix u ht",
            "prix unit",
            "prix unit ht",
            "pu net ht",
            "prix unitaire net",
            "prix unitaire brut",
            "prix achat ht",
            "pab",
            "pan",
        ],
    ),
    (
        "montant_ht",
        &[
            "montant",
            "montant ht",
            "total ht",
            "total",
            "montant net",
            "montant net ht",
            "total ligne",
            "valeur",
            "montant ligne",
            "total net ht",
            "mt ht",
            "mt",
            "net ht",
            "total net",
            "montant total",
            "montant total ht",
            "valeur ht",
        ],
    ),
    (
        "remise_taux",
        &[
            "remise",
            "rem",
            "taux remise",
            "taux de remise",
            "remise ligne",
            "tx remise",
            "remise 1",
            "rem 1",
            "remise taux",
        ],
    ),
    (
        "taux_tva",
        &["tva", "taux tva", "tx tva", "taux de tva", "t v a"],
    ),
    (
        "deee_ht",
        &[
            "deee",
            "d3e",
            "eco deee",
            "eco part deee",
            "ecotaxe",
            "eco contribution",
            "eco taxe",
            "eco participation deee",
            "ecotaxe deee",
            "eco contribution deee",
        ],
    ),
    (
        "ecomob_ht",
        &[
            "eco mobilier",
            "ecomobilier",
            "ecomob",
            "eco maison",
            "ecomaison",
            "eco part",
            "eco participation",
            "eco mob",
            "dea",
            "eco part mobilier",
            "eco participation mobilier",
            "eco mobilier ht",
            "ecopart",
        ],
    ),
    (
        "pv_conseille",
        &[
            "pv",
            "pvc",
            "pv ttc",
            "pvc ttc",
            "prix de vente",
            "prix de vente conseille",
            "pv conseille",
            "prix public",
            "prix vente",
            "prix de vente ttc",
        ],
    ),
];

/// Minuscules, sans accents ni ponctuation, espaces simples : « Qté. livrée » → « qte livree ».
pub fn normaliser(s: &str) -> String {
    let mut sortie = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let r = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => "a",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'î' | 'ï' | 'í' => "i",
            'ô' | 'ö' | 'ó' | 'õ' => "o",
            'ù' | 'û' | 'ü' | 'ú' => "u",
            'ç' => "c",
            'œ' => "oe",
            'æ' => "ae",
            '°' => " ",
            c if c.is_alphanumeric() => {
                sortie.push(c);
                continue;
            }
            _ => " ",
        };
        sortie.push_str(r);
    }
    sortie.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contient_mot(h: &str, mot: &str) -> bool {
    h == mot
        || h.starts_with(&format!("{mot} "))
        || h.ends_with(&format!(" {mot}"))
        || h.contains(&format!(" {mot} "))
}

/// Champ le plus probable d'un intitulé, avec un score (0 : aucun).
fn champ_intitule(h: &str) -> Option<(&'static str, i32)> {
    if h.is_empty() || h.len() > 50 || h.chars().all(|c| c.is_ascii_digit() || c == ' ') {
        return None;
    }
    let mut meilleur: Option<(&'static str, i32)> = None;
    for (champ, syns) in SYNONYMES {
        // Intitulés qui ressemblent à un champ mais n'en sont pas.
        let exclu = match *champ {
            "taux_tva" => ["montant", "total", "code", "mt", "base"]
                .iter()
                .any(|m| contient_mot(h, m)),
            "montant_ht" | "prix_unitaire_ht" => ["tva", "ttc", "vente", "public", "conseille"]
                .iter()
                .any(|m| contient_mot(h, m)),
            "reference" => [
                "ean", "gencod", "barre", "barres", "client", "commande", "bl",
            ]
            .iter()
            .any(|m| contient_mot(h, m)),
            "quantite" => ["colis", "carton", "cartons", "par"]
                .iter()
                .any(|m| contient_mot(h, m)),
            _ => false,
        };
        if exclu {
            continue;
        }
        for syn in *syns {
            let score = if h == *syn {
                1000 + syn.len() as i32
            } else if syn.len() >= 2 && contient_mot(h, syn) {
                100 + 10 * syn.len() as i32 - (h.len() as i32 - syn.len() as i32)
            } else {
                continue;
            };
            let mut score = score;
            if *champ == "quantite" {
                if contient_mot(h, "livree")
                    || contient_mot(h, "liv")
                    || contient_mot(h, "expediee")
                {
                    score += 50;
                }
                if ["commandee", "cde", "cdee", "commande"]
                    .iter()
                    .any(|m| contient_mot(h, m))
                {
                    score -= 60;
                }
            }
            if meilleur.is_none_or(|(_, s)| score > s) {
                meilleur = Some((champ, score));
            }
        }
    }
    meilleur
}

/// Correspondance colonnes → champs d'un tableau.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correspondance {
    /// Index (dans la grille) de la ligne d'en-tête.
    pub ligne_entete: usize,
    /// Colonne (index à partir de 0) de chaque champ trouvé.
    pub colonnes: BTreeMap<String, usize>,
}

impl Correspondance {
    fn col(&self, champ: &str) -> Option<usize> {
        self.colonnes.get(champ).copied()
    }

    fn a(&self, champ: &str) -> bool {
        self.colonnes.contains_key(champ)
    }
}

/// Champs reconnus d'une ligne prise pour en-tête (chaque champ et chaque colonne une fois).
fn reconnaitre(grille: &Grille, l: usize) -> BTreeMap<String, usize> {
    let Some(r) = grille.lignes.get(l) else {
        return BTreeMap::new();
    };
    let mut candidats: Vec<(i32, usize, &'static str)> = r
        .iter()
        .enumerate()
        .filter_map(|(c, v)| match v {
            Valeur::Texte(t) => champ_intitule(&normaliser(t)).map(|(ch, s)| (s, c, ch)),
            _ => None,
        })
        .collect();
    candidats.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut colonnes = BTreeMap::new();
    let mut prises = Vec::new();
    for (_, c, ch) in candidats {
        if colonnes.contains_key(ch) || prises.contains(&c) {
            continue;
        }
        colonnes.insert(ch.to_string(), c);
        prises.push(c);
    }
    colonnes
}

/// Une correspondance est utilisable : un champ qui nomme l'article et un qui le compte.
fn suffisante(c: &BTreeMap<String, usize>) -> bool {
    let nomme = ["reference", "designation", "gencod"]
        .iter()
        .any(|k| c.contains_key(*k));
    let compte = ["quantite", "colis"].iter().any(|k| c.contains_key(*k));
    nomme && compte
}

/// Ligne d'en-tête reconnue par les intitulés, si elle l'est avec assez de certitude (au moins
/// trois champs, dont un qui nomme l'article et un qui le compte).
pub fn reperer(grille: &Grille) -> Option<Correspondance> {
    let mut meilleure: Option<Correspondance> = None;
    for l in 0..grille.lignes.len().min(ENTETE_CHERCHEE) {
        let colonnes = reconnaitre(grille, l);
        if colonnes.len() >= 3
            && suffisante(&colonnes)
            && meilleure
                .as_ref()
                .is_none_or(|m| colonnes.len() > m.colonnes.len())
        {
            meilleure = Some(Correspondance {
                ligne_entete: l,
                colonnes,
            });
        }
    }
    meilleure
}

/// Empreinte des intitulés d'une ligne (normalisés, à leur place) : clé d'une correspondance
/// apprise. Rien pour une ligne de moins de deux intitulés.
pub fn empreinte(grille: &Grille, l: usize) -> Option<String> {
    let r = grille.lignes.get(l)?;
    let fin = r.iter().rposition(|v| !v.est_vide())? + 1;
    let intitules: Vec<String> = r[..fin]
        .iter()
        .map(|v| match v {
            Valeur::Texte(t) => normaliser(t),
            Valeur::Vide => String::new(),
            _ => "#".into(),
        })
        .collect();
    if intitules
        .iter()
        .filter(|i| !i.is_empty() && i.as_str() != "#")
        .count()
        < 2
    {
        return None;
    }
    let h = Sha256::digest(intitules.join("|").as_bytes());
    Some(hex::encode(&h[..12]))
}

/// Empreintes des lignes du haut de chaque feuille, pour chercher des correspondances apprises.
pub fn empreintes(grilles: &[Grille]) -> Vec<String> {
    let mut v = Vec::new();
    for g in grilles {
        for l in 0..g.lignes.len().min(ENTETE_CHERCHEE) {
            if let Some(e) = empreinte(g, l)
                && !v.contains(&e)
            {
                v.push(e);
            }
        }
    }
    v
}

/// Correspondance apprise (clé : empreinte de l'en-tête) : seules les colonnes comptent, la
/// ligne d'en-tête est celle qui a cette empreinte dans la feuille.
pub type Apprises = HashMap<String, BTreeMap<String, usize>>;

/// Intitulés de début de ligne qui ne sont pas des articles : totaux, reports, taxes, port.
const HORS_ARTICLES: &[&str] = &[
    "total",
    "totaux",
    "sous total",
    "s total",
    "montant total",
    "net a payer",
    "report",
    "a reporter",
    "base ht",
    "base tva",
    "montant tva",
    "tva",
    "total ttc",
    "net ht",
    "net ttc",
    "frais de port",
    "port",
    "frais de transport",
    "transport",
    "remise globale",
    "remise sur facture",
    "escompte",
    "acompte",
    "reste a payer",
    "montant ht",
    "total ht",
    "montant ttc",
];

fn hors_article(t: &str) -> bool {
    let n = normaliser(t);
    HORS_ARTICLES
        .iter()
        .any(|h| n == *h || n.starts_with(&format!("{h} ")))
}

/// Taux (remise, TVA) : 0,2 pour 20 % (cellule au format pourcentage) devient 20.
fn taux(v: &Valeur) -> Option<f64> {
    let (x, pourcent) = match v {
        Valeur::Nombre(f) => (*f, false),
        Valeur::Texte(s) => nombre_texte(s)?,
        _ => return None,
    };
    if !pourcent && x > 0.0 && x < 1.0 {
        Some(arrondi(x * 100.0))
    } else {
        Some(arrondi(x))
    }
}

fn code(v: &Valeur) -> Option<String> {
    match v {
        Valeur::Nombre(f) => Some(nombre_affiche(*f)),
        Valeur::Texte(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn gencod(v: &Valeur) -> Option<String> {
    let s: String = code(v)?.chars().filter(|c| !c.is_whitespace()).collect();
    (s.chars().all(|c| c.is_ascii_digit()) && matches!(s.len(), 8 | 12 | 13 | 14)).then_some(s)
}

fn lettre_colonne(mut c: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

/// Lignes d'articles d'une feuille selon une correspondance. Rend aussi les anomalies (cellules
/// en erreur) et les index (dans la grille) des lignes prises.
fn extraire(grille: &Grille, c: &Correspondance) -> (Vec<Ligne>, Vec<String>, Vec<usize>) {
    let (mut lignes, mut anomalies, mut rangs) = (Vec::new(), Vec::new(), Vec::new());
    let cols_nombres: Vec<usize> = c
        .colonnes
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "reference" | "designation" | "unite" | "gencod"))
        .map(|(_, v)| *v)
        .collect();
    for l in c.ligne_entete + 1..grille.lignes.len() {
        if grille.ligne_vide(l) {
            continue;
        }
        let cell = |champ: &str| c.col(champ).map_or(&Valeur::Vide, |k| grille.cellule(l, k));
        // Libellés de la ligne (hors colonnes de nombres) : total, report, port…
        let r = &grille.lignes[l];
        if r.iter().enumerate().any(|(k, v)| {
            !cols_nombres.contains(&k) && matches!(v, Valeur::Texte(t) if hors_article(t))
        }) {
            continue;
        }
        let reference = code(cell("reference"));
        let designation = code(cell("designation"));
        let gencod_lu = gencod(cell("gencod"));
        if reference.is_none() && designation.is_none() && gencod_lu.is_none() {
            continue;
        }
        let quantite = cell("quantite").nombre();
        let colis = cell("colis").nombre();
        let montant = cell("montant_ht").nombre();
        if quantite.is_none() && colis.is_none() && montant.is_none() {
            // Titre de section, note, en-tête répété : pas un article.
            continue;
        }
        for (champ, k) in &c.colonnes {
            if let Valeur::Erreur(e) = grille.cellule(l, *k) {
                anomalies.push(format!(
                    "cellule {}{} ({champ}) en erreur « {e} » dans la feuille {}",
                    lettre_colonne(*k),
                    l + 1,
                    grille.nom
                ));
            }
        }
        let n = |champ: &str| cell(champ).nombre().map(arrondi);
        lignes.push(Ligne {
            reference,
            designation,
            quantite: quantite.map(arrondi),
            unite: code(cell("unite")),
            prix_unitaire_ht: n("prix_unitaire_ht"),
            montant_ht: montant.map(arrondi),
            gencod: gencod_lu,
            colis: colis.map(arrondi),
            pcb: n("pcb"),
            remise_taux: taux(cell("remise_taux")),
            taux_tva: taux(cell("taux_tva")),
            deee_ht: n("deee_ht"),
            ecomob_ht: n("ecomob_ht"),
            pv_conseille: n("pv_conseille"),
        });
        rangs.push(l);
    }
    (lignes, anomalies, rangs)
}

/// Contrôle d'une correspondance (venue des intitulés, apprise ou donnée par Claude) : colonnes
/// existantes et distinctes, au moins une ligne, quantités (et prix) numériques sur un
/// échantillon des lignes qui nomment un article.
pub fn valider(grille: &Grille, c: &Correspondance) -> bool {
    let largeur = grille.largeur();
    if c.ligne_entete >= grille.lignes.len()
        || c.colonnes.values().any(|k| *k >= largeur)
        || c.colonnes.keys().any(|k| !CHAMPS.contains(&k.as_str()))
        || !suffisante(&c.colonnes)
    {
        return false;
    }
    let mut vues: Vec<usize> = c.colonnes.values().copied().collect();
    vues.sort_unstable();
    vues.dedup();
    if vues.len() != c.colonnes.len() {
        return false;
    }
    // Échantillon : les 30 premières lignes qui nomment un article.
    let nomme = |l: usize| {
        ["reference", "designation", "gencod"].iter().any(|k| {
            c.col(k).is_some_and(|col| {
                matches!(grille.cellule(l, col), Valeur::Texte(_) | Valeur::Nombre(_))
            })
        })
    };
    let echantillon: Vec<usize> = (c.ligne_entete + 1..grille.lignes.len())
        .filter(|l| nomme(*l))
        .filter(|l| {
            !grille.lignes[*l]
                .iter()
                .any(|v| matches!(v, Valeur::Texte(t) if hors_article(t)))
        })
        .take(30)
        .collect();
    if echantillon.is_empty() {
        return false;
    }
    let part_numerique = |champ: &str| -> Option<f64> {
        let col = c.col(champ)?;
        let pleines: Vec<&Valeur> = echantillon
            .iter()
            .map(|l| grille.cellule(*l, col))
            .filter(|v| !matches!(v, Valeur::Vide | Valeur::Erreur(_)))
            .collect();
        if pleines.is_empty() {
            return Some(0.0);
        }
        Some(pleines.iter().filter(|v| v.nombre().is_some()).count() as f64 / pleines.len() as f64)
    };
    let compte = if c.a("quantite") { "quantite" } else { "colis" };
    if part_numerique(compte).is_none_or(|p| p < 0.7) {
        return false;
    }
    for champ in ["prix_unitaire_ht", "montant_ht"] {
        if part_numerique(champ).is_some_and(|p| p < 0.7) {
            return false;
        }
    }
    !extraire(grille, c).0.is_empty()
}

/// D'où vient la correspondance des colonnes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origine {
    /// Intitulés reconnus par le code.
    Entete,
    /// Correspondance gardée sur une fiche fournisseur.
    Apprise,
    /// Donnée par Claude pour ce document.
    Claude,
}

impl Origine {
    pub fn code(self) -> &'static str {
        match self {
            Origine::Entete => "entete",
            Origine::Apprise => "apprise",
            Origine::Claude => "claude",
        }
    }
}

/// Tableau d'articles lu dans les cellules.
#[derive(Debug, Clone, PartialEq)]
pub struct TableauLu {
    pub lignes: Vec<Ligne>,
    /// Cellules en erreur dans les colonnes lues.
    pub anomalies: Vec<String>,
    /// Extrait court pour Claude : haut de chaque feuille, en-tête, quelques lignes, zone des
    /// totaux.
    pub extrait: String,
    pub origine: Origine,
    /// Correspondance donnée par Claude, à garder (empreinte, colonnes).
    pub a_garder: Option<(String, BTreeMap<String, usize>)>,
}

impl TableauLu {
    /// Marques de `document.confiance` : lignes lues dans les cellules, origine des colonnes.
    pub fn confiance(&self) -> Value {
        json!({
            LIGNES_SOURCE: "cellules",
            "colonnes_source": self.origine.code(),
            "cellules_en_erreur": self.anomalies.len(),
        })
    }

    /// Consigne ajoutée au prompt de lecture : les lignes sont déjà lues.
    pub fn consigne(&self) -> String {
        format!(
            "\nCe document est un tableur. Ses {} lignes articles ont été lues dans les cellules \
             par le code : le contenu ci-dessus n'en est qu'un extrait (haut de la feuille, \
             en-tête du tableau, premières lignes, zone des totaux). Ne recopie pas les lignes : \
             donne lignes = []. Lis seulement le reste (type, numéro, date, fournisseur, société, \
             numéros de BL, totaux imprimés ; null pour un total absent, sans le calculer).\n",
            self.lignes.len()
        )
    }
}

/// Correspondance trouvée pour une feuille.
fn correspondance(grille: &Grille, apprises: &Apprises) -> Option<(Correspondance, Origine)> {
    for l in 0..grille.lignes.len().min(ENTETE_CHERCHEE) {
        if let Some(cols) = empreinte(grille, l).and_then(|e| apprises.get(&e)) {
            let c = Correspondance {
                ligne_entete: l,
                colonnes: cols.clone(),
            };
            if valider(grille, &c) {
                return Some((c, Origine::Apprise));
            }
        }
    }
    reperer(grille)
        .filter(|c| valider(grille, c))
        .map(|c| (c, Origine::Entete))
}

/// Lit les tableaux d'articles des feuilles (correspondances apprises d'abord, puis intitulés).
/// Rien si aucune feuille n'a de tableau reconnu : Claude peut alors désigner les colonnes
/// (`feuille_a_faire_designer`, `avec_correspondance`), sinon la voie texte reprend.
pub fn lire(grilles: &[Grille], apprises: &Apprises) -> Option<TableauLu> {
    let trouvees: Vec<(usize, Correspondance, Origine)> = grilles
        .iter()
        .enumerate()
        .filter_map(|(i, g)| correspondance(g, apprises).map(|(c, o)| (i, c, o)))
        .collect();
    if trouvees.is_empty() {
        return None;
    }
    let origine = if trouvees.iter().any(|(_, _, o)| *o == Origine::Apprise) {
        Origine::Apprise
    } else {
        Origine::Entete
    };
    Some(assembler(grilles, &trouvees, origine, None))
}

fn assembler(
    grilles: &[Grille],
    trouvees: &[(usize, Correspondance, Origine)],
    origine: Origine,
    a_garder: Option<(String, BTreeMap<String, usize>)>,
) -> TableauLu {
    let (mut lignes, mut anomalies) = (Vec::new(), Vec::new());
    let mut extrait = String::new();
    for (i, g) in grilles.iter().enumerate() {
        extrait.push_str(&format!("## Feuille {}\n", g.nom));
        match trouvees.iter().find(|(k, _, _)| *k == i) {
            Some((_, c, _)) => {
                let (l, a, rangs) = extraire(g, c);
                extrait.push_str(&extrait_feuille(g, c.ligne_entete, &rangs));
                lignes.extend(l);
                anomalies.extend(a);
            }
            None => {
                // Feuille sans tableau d'articles (conditions, récapitulatif) : son début.
                for l in (0..g.lignes.len()).filter(|l| !g.ligne_vide(*l)).take(20) {
                    extrait.push_str(&g.ligne_texte(l));
                    extrait.push('\n');
                }
            }
        }
    }
    let extrait = super::extraction::debut_et_fin(&extrait, EXTRAIT_MAX);
    TableauLu {
        lignes,
        anomalies,
        extrait,
        origine,
        a_garder,
    }
}

fn extrait_feuille(g: &Grille, entete: usize, rangs: &[usize]) -> String {
    let mut s = String::new();
    let pousser = |s: &mut String, l: usize| {
        if !g.ligne_vide(l) {
            s.push_str(&g.ligne_texte(l));
            s.push('\n');
        }
    };
    let debut = entete.saturating_sub(EXTRAIT_HAUT);
    if debut > 0 {
        s.push_str("[... lignes du haut coupées ...]\n");
    }
    for l in debut..=entete {
        pousser(&mut s, l);
    }
    for &l in rangs.iter().take(3) {
        pousser(&mut s, l);
    }
    let derniere = rangs.last().copied().unwrap_or(entete);
    // Lignes non prises au milieu du tableau (sous-totaux, titres) : gardées, elles peuvent
    // porter des montants utiles ; les lignes d'articles, non.
    let milieu: Vec<usize> = (entete + 1..derniere)
        .filter(|l| !rangs.contains(l) && !g.ligne_vide(*l))
        .collect();
    s.push_str(&format!(
        "[... {} lignes articles lues par le code, non recopiées ici ...]\n",
        rangs.len().saturating_sub(3)
    ));
    for l in milieu.into_iter().take(10) {
        pousser(&mut s, l);
    }
    for l in (derniere + 1..g.lignes.len()).take(EXTRAIT_BAS) {
        pousser(&mut s, l);
    }
    s
}

/// Feuille à faire désigner par Claude quand aucune n'a de tableau reconnu : celle qui a le
/// plus de lignes d'au moins trois cellules.
pub fn feuille_a_faire_designer(grilles: &[Grille]) -> Option<usize> {
    grilles
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let n = g
                .lignes
                .iter()
                .filter(|r| r.iter().filter(|v| !v.est_vide()).count() >= 3)
                .count();
            (i, n)
        })
        .filter(|(_, n)| *n >= 2)
        .max_by_key(|(i, n)| (*n, std::cmp::Reverse(*i)))
        .map(|(i, _)| i)
}

/// Les `LIGNES_POUR_CLAUDE` premières lignes non vides d'une feuille, avec leur index.
fn lignes_montrees(g: &Grille) -> Vec<usize> {
    (0..g.lignes.len())
        .filter(|l| !g.ligne_vide(*l))
        .take(LIGNES_POUR_CLAUDE)
        .collect()
}

/// Prompt de la correspondance des colonnes : les premières lignes non vides de la feuille,
/// chaque cellule précédée de sa colonne.
pub fn prompt_correspondance(g: &Grille) -> String {
    let mut p = String::from(
        "Voici les premières lignes non vides d'une feuille de tableur reçue d'un fournisseur \
         (bon de livraison, facture ou avoir). Chaque ligne commence par son numéro, chaque \
         cellule par le numéro de sa colonne entre crochets.\n\n",
    );
    for l in lignes_montrees(g) {
        let r = &g.lignes[l];
        let cellules: Vec<String> = r
            .iter()
            .enumerate()
            .take(40)
            .filter(|(_, v)| !v.est_vide())
            .map(|(c, v)| {
                let t: String = v.texte().chars().take(40).collect();
                format!("[{c}] {t}")
            })
            .collect();
        p.push_str(&format!("ligne {l} : {}\n", cellules.join(" | ")));
    }
    p.push_str(
        "\nDonne ligne_entete = le numéro de la ligne d'en-tête du tableau des articles (celle \
         des intitulés de colonnes), null s'il n'y a pas de tableau d'articles. Pour chaque champ, \
         le numéro de la colonne qui le contient, null si aucune : reference = référence \
         fournisseur de l'article ; gencod = code-barres EAN/GTIN ; designation = libellé ; \
         quantite = quantité livrée ou facturée en unités ; unite ; colis = nombre de colis ou \
         cartons ; pcb = unités par colis ; prix_unitaire_ht = prix unitaire HT ; montant_ht = \
         montant HT de la ligne ; remise_taux = remise de la ligne en % ; taux_tva = taux de \
         TVA ; deee_ht = éco-participation DEEE par unité ; ecomob_ht = éco-mobilier par unité ; \
         pv_conseille = prix de vente conseillé TTC. Une colonne ne sert qu'à un champ.\n",
    );
    p
}

/// Schéma JSON de la réponse de correspondance.
pub fn schema_correspondance() -> Value {
    let colonne = json!({ "type": ["integer", "null"], "minimum": 0 });
    let proprietes: serde_json::Map<String, Value> = CHAMPS
        .iter()
        .map(|c| (c.to_string(), colonne.clone()))
        .collect();
    json!({
        "type": "object",
        "properties": {
            "ligne_entete": { "type": ["integer", "null"], "minimum": 0 },
            "colonnes": {
                "type": "object",
                "properties": proprietes,
                "required": CHAMPS,
                "additionalProperties": false
            }
        },
        "required": ["ligne_entete", "colonnes"],
        "additionalProperties": false
    })
}

/// Correspondance lue dans la réponse de Claude (sortie structurée), si elle désigne une des
/// lignes montrées.
pub fn depuis_reponse(g: &Grille, valeur: &Value) -> Option<Correspondance> {
    let ligne_entete = usize::try_from(valeur.get("ligne_entete")?.as_u64()?).ok()?;
    if !lignes_montrees(g).contains(&ligne_entete) {
        return None;
    }
    let cols = valeur.get("colonnes")?.as_object()?;
    let colonnes = CHAMPS
        .iter()
        .filter_map(|c| {
            let k = cols.get(*c)?.as_u64()?;
            Some((c.to_string(), usize::try_from(k).ok()?))
        })
        .collect();
    Some(Correspondance {
        ligne_entete,
        colonnes,
    })
}

/// Lecture avec la correspondance donnée par Claude pour la feuille `i`, si elle est valide
/// (les autres feuilles reconnues par leurs intitulés sont lues aussi).
pub fn avec_correspondance(
    grilles: &[Grille],
    i: usize,
    c: Correspondance,
    apprises: &Apprises,
) -> Option<TableauLu> {
    let g = grilles.get(i)?;
    if !valider(g, &c) {
        return None;
    }
    let cle = empreinte(g, c.ligne_entete)?;
    let a_garder = Some((cle, c.colonnes.clone()));
    let mut trouvees: Vec<(usize, Correspondance, Origine)> = grilles
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .filter_map(|(k, g)| correspondance(g, apprises).map(|(c, o)| (k, c, o)))
        .collect();
    trouvees.push((i, c, Origine::Claude));
    trouvees.sort_by_key(|(k, _, _)| *k);
    Some(assembler(grilles, &trouvees, Origine::Claude, a_garder))
}

/// Texte complet des feuilles (toutes les lignes) : texte gardé du document et contrôle des
/// montants lus par Claude.
pub fn texte_complet(grilles: &[Grille]) -> String {
    let mut s = String::new();
    for g in grilles {
        s.push_str(&format!("## Feuille {}\n", g.nom));
        for l in 0..g.lignes.len() {
            if !g.ligne_vide(l) {
                s.push_str(&g.ligne_texte(l));
                s.push('\n');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Valeur {
        texte_ou_erreur(s)
    }

    fn grille(lignes: Vec<Vec<Valeur>>) -> Grille {
        Grille {
            nom: "BL".into(),
            lignes,
        }
    }

    #[test]
    fn nombres_en_texte() {
        assert_eq!(nombre_texte("1 234,56"), Some((1234.56, false)));
        assert_eq!(nombre_texte("1.234,56 €"), Some((1234.56, false)));
        assert_eq!(nombre_texte("1,234.56"), Some((1234.56, false)));
        assert_eq!(nombre_texte("20 %"), Some((20.0, true)));
        assert_eq!(nombre_texte("(12,00)"), Some((-12.0, false)));
        assert_eq!(nombre_texte("12,00-"), Some((-12.0, false)));
        assert_eq!(nombre_texte("A12"), None);
        assert_eq!(nombre_texte("2026-11-12"), None);
        assert_eq!(nombre_texte("-"), None);
    }

    #[test]
    fn intitules_reconnus() {
        assert_eq!(normaliser(" Qté. livrée "), "qte livree");
        assert_eq!(champ_intitule("qte livree").unwrap().0, "quantite");
        assert_eq!(champ_intitule("p u ht").unwrap().0, "prix_unitaire_ht");
        assert_eq!(champ_intitule("montant tva"), None);
        assert_eq!(champ_intitule("taux tva").unwrap().0, "taux_tva");
        assert_eq!(champ_intitule("code ean").unwrap().0, "gencod");
        assert_eq!(champ_intitule("nb colis").unwrap().0, "colis");
        assert_eq!(
            champ_intitule("prix de vente conseille").unwrap().0,
            "pv_conseille"
        );
        let g = grille(vec![vec![
            t("Réf."),
            t("Libellé"),
            t("Qté cdée"),
            t("Qté livrée"),
            t("P.U. HT"),
        ]]);
        let c = reconnaitre(&g, 0);
        assert_eq!(c.get("quantite"), Some(&3));
        assert_eq!(c.get("prix_unitaire_ht"), Some(&4));
    }

    #[test]
    fn erreurs_pourcentages_et_totaux() {
        let g = grille(vec![
            vec![t("Fournisseur Durand"), Valeur::Vide],
            vec![
                t("Code"),
                t("Désignation"),
                t("Qté"),
                t("PU HT"),
                t("Remise"),
                t("TVA"),
            ],
            vec![
                t("A1"),
                t("Vase"),
                Valeur::Nombre(2.0),
                Valeur::Nombre(10.5),
                Valeur::Nombre(0.1),
                Valeur::Nombre(0.2),
            ],
            vec![Valeur::Vide, t("ARTS DE LA TABLE")],
            vec![
                t("A2"),
                t("Bol"),
                Valeur::Nombre(3.0),
                Valeur::Erreur("#REF!".into()),
                t("5 %"),
                t("20"),
            ],
            vec![Valeur::Vide, t("Sous-total rayon"), Valeur::Nombre(5.0)],
            vec![
                t("Total HT"),
                Valeur::Vide,
                Valeur::Vide,
                Valeur::Nombre(36.0),
            ],
        ]);
        let lu = lire(std::slice::from_ref(&g), &Apprises::new()).unwrap();
        assert_eq!(lu.origine, Origine::Entete);
        assert_eq!(lu.lignes.len(), 2);
        assert_eq!(lu.lignes[0].remise_taux, Some(10.0));
        assert_eq!(lu.lignes[0].taux_tva, Some(20.0));
        assert_eq!(lu.lignes[1].prix_unitaire_ht, None);
        assert_eq!(lu.lignes[1].remise_taux, Some(5.0));
        assert_eq!(lu.anomalies.len(), 1);
        assert!(lu.anomalies[0].contains("D5"), "{:?}", lu.anomalies);
        assert!(lu.extrait.contains("Total HT"), "{}", lu.extrait);
    }

    #[test]
    fn sans_entete_reconnue_rien() {
        let g = grille(vec![
            vec![t("X1"), t("Colonne bizarre"), t("Truc")],
            vec![t("A1"), t("Vase"), Valeur::Nombre(2.0)],
        ]);
        assert!(lire(std::slice::from_ref(&g), &Apprises::new()).is_none());
        assert_eq!(feuille_a_faire_designer(std::slice::from_ref(&g)), Some(0));
        let c = Correspondance {
            ligne_entete: 0,
            colonnes: [
                ("reference".to_string(), 0),
                ("designation".into(), 1),
                ("quantite".into(), 2),
            ]
            .into_iter()
            .collect(),
        };
        let lu =
            avec_correspondance(std::slice::from_ref(&g), 0, c.clone(), &Apprises::new()).unwrap();
        assert_eq!(lu.lignes.len(), 1);
        let (cle, cols) = lu.a_garder.unwrap();
        // La correspondance apprise sert ensuite sans Claude.
        let apprises: Apprises = [(cle, cols)].into_iter().collect();
        let lu = lire(std::slice::from_ref(&g), &apprises).unwrap();
        assert_eq!(lu.origine, Origine::Apprise);
        // Une correspondance qui prend du texte pour une quantité est refusée.
        let mauvaise = Correspondance {
            ligne_entete: 0,
            colonnes: [("reference".to_string(), 0), ("quantite".into(), 1)]
                .into_iter()
                .collect(),
        };
        assert!(!valider(&g, &mauvaise));
    }

    #[test]
    fn schema_et_reponse() {
        let g = grille(vec![
            vec![t("a"), t("b")],
            vec![t("A1"), Valeur::Nombre(2.0)],
        ]);
        let s = schema_correspondance();
        assert_eq!(
            s["properties"]["colonnes"]["required"]
                .as_array()
                .unwrap()
                .len(),
            CHAMPS.len()
        );
        let r = json!({ "ligne_entete": 0, "colonnes": { "reference": 0, "quantite": 1, "designation": null } });
        let c = depuis_reponse(&g, &r).unwrap();
        assert_eq!(c.colonnes.len(), 2);
        assert!(depuis_reponse(&g, &json!({ "ligne_entete": 7, "colonnes": {} })).is_none());
        assert!(prompt_correspondance(&g).contains("ligne 1 : [0] A1 | [1] 2"));
    }
}
