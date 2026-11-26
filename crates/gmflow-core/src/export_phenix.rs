//! Export d'un BL vers le logiciel du magasin (BO Phénix) :
//! - la **matrice d'intégration** (commande : CODEIN, GENCOD, REFERENCE, QTECDE, PA, REMISE ;
//!   le fournisseur se choisit dans Phénix à l'import) ;
//! - la **trame d'import** des fiches produits (le plus d'informations possible, code
//!   fournisseur BO renseigné).
//!
//! Tout vient du BL (prix, quantités, TVA, éco-participations), sauf pour les lots : une ligne
//! de lot est remplacée par ses composants tels que le magasin les connaît, et le prix
//! d'achat des composants est rééquilibré pour que leur total redonne le prix du lot sur le BL.
//! Dans la trame, un lot est un « lot de vente » : ligne mère puis composants (« NUM LOT
//! VENTE » = n° de la ligne mère, « QTE LOT VENTE » = quantité dans le lot).

use rust_xlsxwriter::{Workbook, XlsxError};
use serde::Serialize;

use crate::articles_magasin::{ArticleMagasin, Composant};

/// En-têtes de la matrice, à l'identique du modèle (« REMISE » suivi d'une espace).
pub const COLONNES_MATRICE: [&str; 6] =
    ["CODEIN", "GENCOD", "REFERENCE", "QTECDE", "PA", "REMISE "];

/// En-têtes de l'onglet « TRAME MODELE » du modèle BO Phénix, dans son ordre.
pub const COLONNES_TRAME: [&str; 105] = [
    "NUMERO LIGNE",
    "NUMERO DE LOT",
    "NUM LOT VENTE",
    "Codein",
    "Fournisseur",
    "Ref Fournisseur",
    "Gencod",
    "LIBELLE",
    "Colisage",
    "LibTicket",
    "PrixAchat",
    "Prix de Vente",
    "CodeFamille",
    "Qté à commander",
    "PV_Conseille",
    "TauxTva",
    "DEEE HT",
    "Ecomob HT",
    "DEEE TTC",
    "Ecomob TTC",
    "Garantie légale",
    "Unite de Vente",
    "NBR UNITE VENTE",
    "Décl. Caisse",
    "Texte Caisse",
    "Gamme",
    "Saison",
    "Marque",
    "StockMini",
    "InterditAchat",
    "InterditVente",
    "Preference",
    "SUSPENDU (REF)",
    "Décimal en caisse",
    "CodePub",
    "LIBELLE PUB",
    "DATE DEBUT PUB",
    "DATE FIN PUB",
    "PVPUB",
    "Remise 1",
    "Remise 2",
    "Remise 3",
    "PromoDeb",
    "PromoFin",
    "NBUV_ACHAT",
    "QTE LOT VENTE",
    "% LOT VENTE",
    "Ticket restaurant",
    "Interdit publicité",
    "OnWeb",
    "ZoneResponsable",
    "NOMENDOUANIERE",
    "NOMENDOUANIERECOM",
    "CODEECOMOB",
    "CODE ECOTAXE",
    "COMMENTAIRE",
    "COMMENTAIRE2",
    "COMMENTAIRE4",
    "Libelle2",
    "POIDS",
    "Libre1",
    "Libre2",
    "Libre3",
    "Libre4",
    "Libre5",
    "Libre6",
    "Libre7",
    "Libre8",
    "Libre9",
    "Libre10",
    "Libre11",
    "OnDivers1",
    "OnDivers2",
    "OnDivers3",
    "OnDivers4",
    "OnDivers6",
    "OnDivers8",
    "OnDivers10",
    "OnDivers11",
    "OnDivers12",
    "OnDivers14",
    "OnDivers16",
    "OnDivers17",
    "OnDivers18",
    "OnDivers19",
    "OnDivers20",
    "Artchamp1",
    "Artchamp2",
    "Artchamp3",
    "Artchamp4",
    "Artchamp5",
    "Artchamp6",
    "Artchamp7",
    "Artchamp8",
    "Artchamp9",
    "Artchamp11",
    "Artchamp12",
    "Artchamp13",
    "Artchamp14",
    "Artchamp15",
    "Artchamp16",
    "Artchamp17",
    "Artchamp18",
    "Artchamp19",
    "Artchamp20",
];

/// Colonnes d'identification remplies depuis le BL, réglées par fournisseur (fiche, sinon son
/// groupe ; les deux par défaut) : certains fournisseurs impriment une référence ou un gencod
/// que le magasin ne veut pas reprendre. Les composants d'un lot gardent ceux du magasin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Colonnes {
    #[default]
    LesDeux,
    Reference,
    Gencod,
}

impl Colonnes {
    /// Valeur de `fournisseur.colonnes_export` (`ref`, `gencod`, null = les deux).
    pub fn depuis(v: Option<&str>) -> Self {
        match v {
            Some("ref") => Self::Reference,
            Some("gencod") => Self::Gencod,
            _ => Self::LesDeux,
        }
    }

    fn reference(self) -> bool {
        self != Self::Gencod
    }

    fn gencod(self) -> bool {
        self != Self::Reference
    }
}

/// Ligne de BL telle que lue, avec l'article retrouvé au magasin.
#[derive(Debug, Clone, Default)]
pub struct LigneBl {
    pub rang: i32,
    pub reference: Option<String>,
    pub designation: Option<String>,
    pub quantite: Option<f64>,
    pub prix_unitaire_ht: Option<f64>,
    pub montant_ht: Option<f64>,
    pub gencod: Option<String>,
    pub colis: Option<f64>,
    pub pcb: Option<f64>,
    pub remise_taux: Option<f64>,
    pub taux_tva: Option<f64>,
    pub deee_ht: Option<f64>,
    pub ecomob_ht: Option<f64>,
    pub pv_conseille: Option<f64>,
    pub magasin: Option<ArticleMagasin>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cellule {
    Texte(String),
    Nombre(f64),
}

/// Ligne d'un fichier : (colonne, valeur), les colonnes absentes restent vides.
pub type Rangee = Vec<(&'static str, Cellule)>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Alerte {
    /// Rang de la ligne du BL (0 = première), absent pour une alerte générale.
    pub rang: Option<i32>,
    /// Désignation (ou référence) de la ligne, pour la retrouver.
    pub article: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Export {
    pub rangees: Vec<Rangee>,
    pub alertes: Vec<Alerte>,
}

fn arrondi(x: f64, decimales: i32) -> f64 {
    let f = 10f64.powi(decimales);
    (x * f).round() / f
}

fn texte(v: Option<&str>) -> Option<Cellule> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Cellule::Texte(s.to_string()))
}

fn nombre(v: Option<f64>) -> Option<Cellule> {
    v.filter(|x| x.is_finite()).map(Cellule::Nombre)
}

fn pousser(r: &mut Rangee, col: &'static str, c: Option<Cellule>) {
    if let Some(c) = c {
        r.push((col, c));
    }
}

/// Quantité livrée en unités : colis × PCB quand le BL donne les deux, sinon la quantité.
pub fn unites(l: &LigneBl) -> Option<f64> {
    match (l.colis, l.pcb) {
        (Some(c), Some(p)) if c > 0.0 && p > 0.0 => Some(arrondi(c * p, 3)),
        _ => l.quantite.filter(|q| *q > 0.0),
    }
}

/// Prix d'achat brut HT par unité et remise de la ligne (%). Un prix imprimé par colis est
/// ramené à l'unité (le montant de la ligne dit lequel des deux colle). Sans prix unitaire,
/// le montant divisé par la quantité (déjà net : pas de remise en plus).
pub fn prix_achat(l: &LigneBl) -> (Option<f64>, Option<f64>) {
    let remise = l.remise_taux.filter(|r| *r > 0.0 && *r < 100.0);
    let u = unites(l);
    match l.prix_unitaire_ht.filter(|p| *p > 0.0) {
        Some(pu) => {
            if let (Some(c), Some(p), Some(m), Some(u)) = (l.colis, l.pcb, l.montant_ht, u)
                && p > 1.0
                && c > 0.0
            {
                let net = 1.0 - remise.unwrap_or(0.0) / 100.0;
                let ecart_unite = (pu * u * net - m).abs();
                let ecart_colis = (pu * c * net - m).abs();
                if ecart_colis < ecart_unite {
                    return (Some(arrondi(pu / p, 4)), remise);
                }
            }
            (Some(pu), remise)
        }
        None => match (l.montant_ht, u) {
            (Some(m), Some(u)) if m > 0.0 => (Some(arrondi(m / u, 4)), None),
            _ => (None, None),
        },
    }
}

/// Prix d'achat de chaque composant pour que Σ prix × quantité = `prix_lot`. On garde la
/// répartition du magasin (prix d'achat, sinon prix de vente, sinon à parts égales) ; si le
/// total colle déjà, les prix du magasin restent tels quels. Sans prix du lot : ceux du magasin.
pub fn prix_composants(prix_lot: Option<f64>, composants: &[Composant]) -> Vec<Option<f64>> {
    let Some(p) = prix_lot.filter(|p| *p > 0.0) else {
        return composants.iter().map(|c| c.pa).collect();
    };
    let poids = |f: fn(&Composant) -> Option<f64>| -> Option<Vec<f64>> {
        composants
            .iter()
            .map(|c| f(c).filter(|x| *x > 0.0))
            .collect()
    };
    let (base, du_magasin) = match poids(|c| c.pa) {
        Some(v) => (v, true),
        None => match poids(|c| c.pv) {
            Some(v) => (v, false),
            None => (vec![1.0; composants.len()], false),
        },
    };
    let total: f64 = base
        .iter()
        .zip(composants)
        .map(|(b, c)| b * c.quantite)
        .sum();
    if total <= 0.0 {
        return vec![None; composants.len()];
    }
    if du_magasin && (total - p).abs() < 0.005 {
        return base.into_iter().map(Some).collect();
    }
    base.iter()
        .map(|b| Some(arrondi(b * p / total, 4)))
        .collect()
}

fn lot(l: &LigneBl) -> Option<&ArticleMagasin> {
    l.magasin.as_ref().filter(|a| !a.composants.is_empty())
}

fn alerte(a: &mut Vec<Alerte>, l: &LigneBl, m: impl Into<String>) {
    a.push(Alerte {
        rang: Some(l.rang),
        article: l
            .designation
            .clone()
            .or_else(|| l.reference.clone())
            .filter(|x| !x.trim().is_empty()),
        message: m.into(),
    });
}

/// Gencod du BL repris dans les fichiers : seulement si sa clé de contrôle GS1 est juste.
fn gencod_bl(l: &LigneBl) -> Option<&str> {
    l.gencod
        .as_deref()
        .filter(|g| crate::lecture::xml::gencod_valide(g))
}

/// Contrôles communs aux deux fichiers.
fn controler(l: &LigneBl, colonnes: Colonnes, a: &mut Vec<Alerte>) {
    match (unites(l), l.quantite, l.colis, l.pcb) {
        (None, ..) => alerte(a, l, "quantité absente"),
        // Colis ou PCB mal lu : le stock serait multiplié (12 unités, 12 colis lus → 72).
        (Some(u), Some(q), Some(c), Some(p)) if c > 0.0 && p > 0.0 && (u - q).abs() > 0.001 => {
            alerte(
                a,
                l,
                format!(
                    "quantité imprimée {q} mais colis × PCB = {c} × {p} = {u} : {u} exportés, \
                     vérifier le colisage"
                ),
            )
        }
        _ => {}
    }
    if colonnes.gencod()
        && let Some(g) = l.gencod.as_deref().filter(|g| !g.trim().is_empty())
        && gencod_bl(l).is_none()
    {
        alerte(
            a,
            l,
            format!("gencod « {g} » invalide (clé de contrôle fausse) : laissé vide"),
        );
    }
    if prix_achat(l).0.is_none() {
        alerte(
            a,
            l,
            if lot(l).is_some() {
                "prix du lot absent du BL : prix des composants du magasin"
            } else {
                "prix d'achat absent du BL"
            },
        );
    }
}

/// Matrice d'intégration (commande) : une rangée par article, les lots remplacés par leurs
/// composants.
pub fn matrice(lignes: &[LigneBl], colonnes: Colonnes) -> Export {
    let mut e = Export::default();
    for l in lignes {
        controler(l, colonnes, &mut e.alertes);
        let u = unites(l);
        let (pa, remise) = prix_achat(l);
        if let Some(a) = lot(l) {
            let prix = prix_composants(pa, &a.composants);
            for (c, p) in a.composants.iter().zip(prix) {
                let mut r = Rangee::new();
                pousser(&mut r, "CODEIN", texte(c.codein.as_deref()));
                pousser(&mut r, "GENCOD", texte(c.gencod.as_deref()));
                pousser(&mut r, "REFERENCE", texte(c.reference.as_deref()));
                pousser(
                    &mut r,
                    "QTECDE",
                    nombre(u.map(|u| arrondi(u * c.quantite, 3))),
                );
                pousser(&mut r, "PA", nombre(p));
                pousser(&mut r, "REMISE ", nombre(remise));
                e.rangees.push(r);
            }
            continue;
        }
        // Hors lot : gencod et référence du BL seulement (selon le réglage du fournisseur),
        // pas de code article.
        let gencod = gencod_bl(l).filter(|_| colonnes.gencod());
        let reference = l.reference.as_deref().filter(|_| colonnes.reference());
        if gencod.is_none() && reference.is_none() {
            alerte(
                &mut e.alertes,
                l,
                match colonnes {
                    Colonnes::LesDeux => "ni gencod ni référence sur le BL : à saisir dans Phénix",
                    Colonnes::Reference => "pas de référence sur le BL : à saisir dans Phénix",
                    Colonnes::Gencod => "pas de gencod sur le BL : à saisir dans Phénix",
                },
            );
        }
        let mut r = Rangee::new();
        pousser(&mut r, "GENCOD", texte(gencod));
        pousser(&mut r, "REFERENCE", texte(reference));
        pousser(&mut r, "QTECDE", nombre(u));
        pousser(&mut r, "PA", nombre(pa));
        pousser(&mut r, "REMISE ", nombre(remise));
        e.rangees.push(r);
    }
    e
}

/// Colonne d'éco-participation d'un composant : celle que le BL donne pour le lot.
fn colonne_eco(l: &LigneBl) -> Option<&'static str> {
    match (
        l.deee_ht.is_some_and(|x| x > 0.0),
        l.ecomob_ht.is_some_and(|x| x > 0.0),
    ) {
        (true, false) => Some("DEEE HT"),
        (false, true) => Some("Ecomob HT"),
        _ => None,
    }
}

/// Éco-participation de la trame : 0 quand le BL n'en donne pas (colonne jamais vide).
fn eco(v: Option<f64>) -> Option<Cellule> {
    nombre(Some(v.filter(|x| *x > 0.0).unwrap_or(0.0)))
}

/// Trame d'import BO Phénix (fiches produits). `fournisseur` : code BO Phénix.
pub fn trame(lignes: &[LigneBl], fournisseur: Option<&str>, colonnes: Colonnes) -> Export {
    let mut e = Export::default();
    if fournisseur.is_none() {
        e.alertes.push(Alerte {
            rang: None,
            article: None,
            message: "code BO Phénix absent de la fiche fournisseur : colonne Fournisseur vide"
                .into(),
        });
    }
    let fou = || texte(fournisseur);
    let mut numero = 0;
    for l in lignes {
        controler(l, colonnes, &mut e.alertes);
        let (pa, _) = prix_achat(l);
        // N° de ligne et TVA laissés vides : le magasin les saisit dans Phénix. Le numéro sert
        // seulement à rattacher les composants d'un lot à leur ligne mère (NUM LOT VENTE).
        numero += 1;
        let mut r = Rangee::new();
        // Codein et données du magasin pour un lot seulement ; sinon tout vient du BL.
        let mere = lot(l);
        pousser(
            &mut r,
            "Codein",
            texte(mere.and_then(|a| a.codein.as_deref())),
        );
        pousser(&mut r, "Fournisseur", fou());
        pousser(
            &mut r,
            "Ref Fournisseur",
            texte(l.reference.as_deref().filter(|_| colonnes.reference())),
        );
        pousser(
            &mut r,
            "Gencod",
            texte(
                gencod_bl(l)
                    .or_else(|| mere.and_then(|a| a.gencod.as_deref()))
                    .filter(|_| colonnes.gencod()),
            ),
        );
        pousser(
            &mut r,
            "LIBELLE",
            texte(
                l.designation
                    .as_deref()
                    .or_else(|| mere.and_then(|a| a.libelle.as_deref())),
            ),
        );
        pousser(&mut r, "Colisage", nombre(l.pcb.filter(|p| *p > 0.0)));
        pousser(&mut r, "PV_Conseille", nombre(l.pv_conseille));
        let Some(a) = mere else {
            if l.magasin.is_none() && crate::articles_magasin::semble_lot(l.designation.as_deref())
            {
                alerte(
                    &mut e.alertes,
                    l,
                    "lot présumé inconnu du magasin : composants non détaillés, prix de vente et famille à fixer",
                );
            }
            pousser(&mut r, "PrixAchat", nombre(pa));
            pousser(&mut r, "DEEE HT", eco(l.deee_ht));
            pousser(&mut r, "Ecomob HT", eco(l.ecomob_ht));
            e.rangees.push(r);
            continue;
        };
        // Lot de vente : ligne mère, puis ses composants.
        let num_mere = numero;
        e.rangees.push(r);
        let colonne = colonne_eco(l);
        if colonne.is_none() && a.composants.iter().any(|c| c.eco_ht.is_some()) {
            alerte(
                &mut e.alertes,
                l,
                "lot : le BL ne dit pas si l'éco-participation est DEEE ou Ecomob, composants sans éco-participation",
            );
        }
        for (c, p) in a.composants.iter().zip(prix_composants(pa, &a.composants)) {
            numero += 1;
            let mut r = Rangee::new();
            pousser(&mut r, "NUM LOT VENTE", nombre(Some(num_mere as f64)));
            pousser(&mut r, "Codein", texte(c.codein.as_deref()));
            pousser(&mut r, "Fournisseur", fou());
            pousser(&mut r, "Ref Fournisseur", texte(c.reference.as_deref()));
            pousser(&mut r, "Gencod", texte(c.gencod.as_deref()));
            pousser(&mut r, "LIBELLE", texte(c.libelle.as_deref()));
            pousser(&mut r, "LibTicket", texte(c.lib_ticket.as_deref()));
            pousser(&mut r, "Colisage", nombre(Some(1.0)));
            pousser(&mut r, "PrixAchat", nombre(p));
            pousser(&mut r, "Prix de Vente", nombre(c.pv));
            pousser(&mut r, "CodeFamille", texte(c.famille.as_deref()));
            pousser(&mut r, "PV_Conseille", nombre(c.pv_conseille));
            // Colonne que le BL donne pour le lot ; l'autre (ou les deux si le BL ne dit
            // rien) à 0.
            for col in ["DEEE HT", "Ecomob HT"] {
                pousser(
                    &mut r,
                    col,
                    eco((colonne == Some(col)).then_some(c.eco_ht).flatten()),
                );
            }
            pousser(&mut r, "QTE LOT VENTE", nombre(Some(c.quantite)));
            pousser(
                &mut r,
                "% LOT VENTE",
                nombre(c.pourc_marge.filter(|x| *x > 0.0)),
            );
            e.rangees.push(r);
        }
    }
    e
}

fn classeur(onglet: &str, colonnes: &[&str], rangees: &[Rangee]) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(onglet)?;
    for (i, c) in colonnes.iter().enumerate() {
        ws.write_string(0, i as u16, *c)?;
    }
    for (n, r) in rangees.iter().enumerate() {
        let ligne = n as u32 + 1;
        for (col, v) in r {
            let Some(i) = colonnes.iter().position(|c| c == col) else {
                continue;
            };
            match v {
                Cellule::Texte(t) => ws.write_string(ligne, i as u16, t)?,
                Cellule::Nombre(x) => ws.write_number(ligne, i as u16, *x)?,
            };
        }
    }
    wb.save_to_buffer()
}

/// Fichier de la matrice (onglet « Feuil1 », comme le modèle).
pub fn xlsx_matrice(e: &Export) -> Result<Vec<u8>, XlsxError> {
    classeur("Feuil1", &COLONNES_MATRICE, &e.rangees)
}

/// Fichier de la trame (onglet « TRAME MODELE » en premier, comme l'exige l'import).
pub fn xlsx_trame(e: &Export) -> Result<Vec<u8>, XlsxError> {
    classeur("TRAME MODELE", &COLONNES_TRAME, &e.rangees)
}

/// Lignes d'un document pour l'export.
pub async fn lignes(pool: &sqlx::PgPool, document_id: i64) -> sqlx::Result<Vec<LigneBl>> {
    let r = sqlx::query!(
        r#"select rang, reference, designation, quantite::float8 as quantite,
                  prix_unitaire_ht::float8 as prix, montant_ht::float8 as montant, gencod,
                  colis::float8 as colis, pcb::float8 as pcb, remise_taux::float8 as remise,
                  taux_tva::float8 as tva, deee_ht::float8 as deee, ecomob_ht::float8 as ecomob,
                  pv_conseille::float8 as pvc, magasin
           from ligne_document where document_id = $1 order by rang"#,
        document_id
    )
    .fetch_all(pool)
    .await?;
    Ok(r.into_iter()
        .map(|l| LigneBl {
            rang: l.rang,
            reference: l.reference,
            designation: l.designation,
            quantite: l.quantite,
            prix_unitaire_ht: l.prix,
            montant_ht: l.montant,
            gencod: l.gencod,
            colis: l.colis,
            pcb: l.pcb,
            remise_taux: l.remise,
            taux_tva: l.tva,
            deee_ht: l.deee,
            ecomob_ht: l.ecomob,
            pv_conseille: l.pvc,
            magasin: l.magasin.and_then(|v| serde_json::from_value(v).ok()),
        })
        .collect())
}
