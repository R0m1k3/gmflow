//! Appel de lecture à Claude : prompt, schéma JSON de la réponse et contrôle de la réponse.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::societe::SocieteRef;

pub const TYPES: &[&str] = &["facture", "avoir", "bl", "relance", "devis", "autre"];

/// Longueur maximale du texte d'un document envoyé à Claude.
pub const TEXTE_MAX: usize = 20_000;

/// Lignes articles gardées par document (les suivantes sont ignorées).
pub const LIGNES_MAX: usize = 200;

/// Clé de `document.confiance` : la lecture n'a pas tout pris (texte ou tableur coupé, plus de
/// `LIGNES_MAX` lignes), des lignes articles peuvent manquer. Les totaux d'un BL ne sont alors
/// pas calculés depuis ses lignes, ni les lignes comparées entre facture et BL.
pub const LIGNES_TRONQUEES: &str = "lignes_tronquees";

pub const PROMPT_SYSTEME: &str = "Tu lis des documents reçus par mail par des magasins : factures, \
avoirs, bons de livraison (BL), relances, devis. Tu réponds uniquement avec la sortie structurée \
demandée, selon le schéma. Seuls les factures, avoirs et bons de livraison nous intéressent : \
un relevé, une commande, un devis, une relance, un catalogue, un tarif ou tout autre document est de type « autre » \
(« devis » ou « relance » s'ils le sont clairement). Tu n'inventes rien : un champ absent ou illisible vaut null. Les dates \
sont au format AAAA-MM-JJ, les montants en euros avec un point décimal. La confiance de chaque \
champ va de 0 (deviné) à 1 (lu sans ambiguïté).";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Fournisseur {
    pub nom: Option<String>,
    pub siret: Option<String>,
    pub tva: Option<String>,
    /// Enseigne ou maison mère vue en en-tête ou en logo (« U10 »). Absent des anciennes
    /// réponses : null.
    #[serde(default)]
    pub groupe: Option<String>,
    /// Société, marque, département ou division qui a réellement vendu ou livré, quand le
    /// document la distingue du groupe (« Dép. : L3C »).
    #[serde(default)]
    pub entite: Option<String>,
}

/// Ligne article d'un BL ou d'une facture.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ligne {
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub designation: Option<String>,
    #[serde(default)]
    pub quantite: Option<f64>,
    #[serde(default)]
    pub unite: Option<String>,
    #[serde(default)]
    pub prix_unitaire_ht: Option<f64>,
    #[serde(default)]
    pub montant_ht: Option<f64>,
    /// Code-barres EAN / GTIN de l'article (8, 12, 13 ou 14 chiffres).
    #[serde(default)]
    pub gencod: Option<String>,
    /// Nombre de colis (cartons) livrés.
    #[serde(default)]
    pub colis: Option<f64>,
    /// Unités par colis (PCB, colisage).
    #[serde(default)]
    pub pcb: Option<f64>,
    /// Remise de la ligne en % (« 5 » pour 5 %).
    #[serde(default)]
    pub remise_taux: Option<f64>,
    /// Taux de TVA de la ligne en % quand le document le précise.
    #[serde(default)]
    pub taux_tva: Option<f64>,
    /// Éco-participation DEEE (équipements électriques) HT, par unité.
    #[serde(default)]
    pub deee_ht: Option<f64>,
    /// Éco-mobilier (Ecomaison, éco-participation meubles) HT, par unité.
    #[serde(default)]
    pub ecomob_ht: Option<f64>,
    /// Prix de vente conseillé TTC imprimé par le fournisseur.
    #[serde(default)]
    pub pv_conseille: Option<f64>,
}

impl Ligne {
    /// Ligne sans aucune information utile.
    pub fn vide(&self) -> bool {
        let rien = |s: &Option<String>| s.as_deref().is_none_or(|v| v.trim().is_empty());
        rien(&self.reference)
            && rien(&self.designation)
            && self.quantite.is_none()
            && self.montant_ht.is_none()
    }
}

/// Lignes utiles, au plus `LIGNES_MAX`.
pub fn lignes_utiles(lignes: Vec<Ligne>) -> Vec<Ligne> {
    lignes
        .into_iter()
        .filter(|l| !l.vide())
        .take(LIGNES_MAX)
        .collect()
}

/// Taux de TVA d'une ligne qui ne le précise pas.
pub const TVA_PAR_DEFAUT: f64 = 20.0;

fn centimes(m: f64) -> f64 {
    (m * 100.0).round() / 100.0
}

/// BL qui n'imprime pas ses totaux (tableur : quantité et prix unitaire seulement) : complète
/// le montant des lignes (quantité × prix unitaire, moins la remise de la ligne ; colis × PCB
/// sans quantité), puis le HT, la TVA et le TTC qui manquent. HT = lignes + éco-participations
/// (DEEE, éco-mobilier par unité) ; TVA au taux de la ligne, 20 % sinon. Rien si une ligne
/// reste sans montant (un total partiel tromperait). Rend les montants calculés (« ht »,
/// « tva », « ttc »).
pub fn completer_montants_bl(
    lignes: &mut [Ligne],
    ht: &mut Option<f64>,
    tva: &mut Option<f64>,
    ttc: &mut Option<f64>,
) -> Vec<&'static str> {
    let quantite = |l: &Ligne| {
        l.quantite.or(match (l.colis, l.pcb) {
            (Some(c), Some(p)) => Some(c * p),
            _ => None,
        })
    };
    for l in lignes.iter_mut().filter(|l| l.montant_ht.is_none()) {
        if let (Some(q), Some(pu)) = (quantite(l), l.prix_unitaire_ht) {
            let remise = l
                .remise_taux
                .filter(|r| *r > 0.0 && *r < 100.0)
                .unwrap_or(0.0);
            l.montant_ht = Some(centimes(q * pu * (1.0 - remise / 100.0)));
        }
    }
    if lignes.is_empty()
        || (ht.is_some() && tva.is_some() && ttc.is_some())
        || lignes.iter().any(|l| l.montant_ht.is_none())
    {
        return Vec::new();
    }
    let (mut ht_lignes, mut tva_lignes) = (0.0, 0.0);
    for l in lignes.iter() {
        let eco =
            quantite(l).unwrap_or(0.0) * (l.deee_ht.unwrap_or(0.0) + l.ecomob_ht.unwrap_or(0.0));
        let base = l.montant_ht.unwrap_or(0.0) + eco;
        ht_lignes += base;
        tva_lignes += base * l.taux_tva.unwrap_or(TVA_PAR_DEFAUT) / 100.0;
    }
    let calcules: Vec<&'static str> = [
        ("ht", ht.is_none()),
        ("tva", tva.is_none()),
        ("ttc", ttc.is_none()),
    ]
    .into_iter()
    .filter_map(|(c, manque)| manque.then_some(c))
    .collect();
    let h = *ht.get_or_insert(centimes(ht_lignes));
    let t = *tva.get_or_insert_with(|| {
        // HT imprimé mais pas la TVA : taux moyen des lignes appliqué au HT imprimé.
        if ht_lignes.abs() > 0.005 {
            centimes(h * tva_lignes / ht_lignes)
        } else {
            centimes(h * TVA_PAR_DEFAUT / 100.0)
        }
    });
    ttc.get_or_insert(centimes(h + t));
    calcules
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Confiance {
    pub r#type: f64,
    pub societe: f64,
    pub fournisseur: f64,
    pub numero: f64,
    pub date: f64,
    pub montants: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extraction {
    pub r#type: String,
    pub societe_id: Option<i64>,
    pub fournisseur: Fournisseur,
    pub numero: Option<String>,
    pub date: Option<String>,
    pub numero_commande: Option<String>,
    /// Date d'échéance du paiement (facture), AAAA-MM-JJ. Absente des anciennes réponses.
    #[serde(default)]
    pub echeance: Option<String>,
    pub montant_ht: Option<f64>,
    pub montant_tva: Option<f64>,
    pub montant_ttc: Option<f64>,
    /// Total HT des remises déduites sur le document, en positif (déjà retiré du HT).
    #[serde(default)]
    pub remise_ht: Option<f64>,
    /// Taux de la remise en %, quand il est imprimé.
    #[serde(default)]
    pub remise_taux: Option<f64>,
    /// Total HT des montants annexes compris dans le HT (éco-participation, port…).
    #[serde(default)]
    pub annexes_ht: Option<f64>,
    pub confiance: Confiance,
    /// Lignes articles (obligatoires pour un BL, si lisibles pour une facture).
    #[serde(default)]
    pub lignes: Vec<Ligne>,
    /// Numéros des bons de livraison cités par une facture.
    #[serde(default)]
    pub numeros_bl: Vec<String>,
    /// Transcription d'un scan (absente pour un document déjà en texte).
    #[serde(default)]
    pub texte: Option<String>,
    /// Date lue mais impossible ou mal formée (« 2026-11-31 ») : `date` est alors vide.
    #[serde(skip)]
    pub date_illisible: Option<String>,
    /// Lignes coupées à `LIGNES_MAX` (Claude en rend au plus autant).
    #[serde(skip)]
    pub lignes_tronquees: bool,
}

fn chaine_ou_null() -> Value {
    json!({ "type": ["string", "null"] })
}

fn nombre_ou_null() -> Value {
    json!({ "type": ["number", "null"] })
}

fn confiance() -> Value {
    json!({ "type": "number", "minimum": 0, "maximum": 1 })
}

/// Montants annexes (éco-participation, port…) : souvent imprimés sous le « Total HT » et la
/// « TVA » mais compris dans le TTC. La compta (Dext, Yooz) les veut dans le HT et la TVA.
const MONTANTS_ANNEXES: &str = "montant_ht, montant_tva et montant_ttc décrivent le document \
     entier : HT + TVA = TTC. Quand des montants s'ajoutent après le « Total HT » imprimé \
     (éco-participation, Éco-mobilier, Ecomaison, DEEE, éco-contribution, frais de port, \
     emballage…), avec ou sans leur propre ligne de TVA (« TVA ecomaison »…), ajoute-les : \
     montant_ht = total HT imprimé + ces montants HT, montant_tva = toute la TVA du document, \
     y compris celle de ces montants. Exemple : Total HT 1 677,74, TVA 335,55, Ecomaison 12,47, \
     TVA Ecomaison 2,49, Total TTC 2 028,25 donne montant_ht 1690.21, montant_tva 338.04, \
     montant_ttc 2028.25.\n";

/// Échéance du paiement, recopiée dans Logiflow avec la facture.
const ECHEANCE: &str = "echeance = la date d'échéance du paiement imprimée (« Échéance », « Date \
     d'échéance », « À payer avant le », « Date limite de paiement »), AAAA-MM-JJ ; null si \
     seule une condition est donnée (« 30 jours fin de mois ») ou pour un BL.\n";

/// Remises : le HT reste le net imprimé, la remise est notée à part pour expliquer l'écart
/// avec les bons de livraison (valorisés avant remise).
const REMISES: &str = "remise_ht = le total HT des remises déduites sur le document (remise \
     globale, remise commerciale, remise de pied, rabais, ristourne, escompte), en nombre \
     positif, null s'il n'y en a pas ; remise_taux = son pourcentage s'il est imprimé (20 pour \
     « remise globale 20 % »), null sinon. annexes_ht = le total HT des montants annexes compris \
     dans montant_ht (éco-participation, Éco-mobilier, Ecomaison, DEEE, éco-contribution, frais \
     de port, emballage), qu'ils soient imprimés avant ou après le « Total HT », null s'il n'y en \
     a pas. Les montants restent nets : montant_ht est le total HT après remise, annexes \
     comprises. Exemple : Marchandise HT 1 400,80, Ecomaison 40,65, Remise globale 20 % -280,16, \
     Total HT net 1 161,29 donne montant_ht 1161.29, remise_ht 280.16, remise_taux 20, \
     annexes_ht 40.65.\n";

/// Signe des avoirs : toujours en positif, le type dit que c'est un avoir.
pub const AVOIRS_POSITIFS: &str = "Pour un avoir, tous les montants (montant_ht, montant_tva, \
     montant_ttc, remise_ht, annexes_ht et ceux des lignes) sont donnés en positif, sans signe \
     moins, même s'ils sont imprimés en négatif : le type « avoir » suffit à dire qu'ils sont \
     à rendre.\n";

/// Détail des lignes, surtout pour les BL (fiches produits et commande dans le logiciel du magasin).
const DETAIL_LIGNES: &str = "Pour chaque ligne, recopie aussi ce que le document imprime, \
     null sinon : gencod = le code-barres EAN/GTIN de l'article (8, 12, 13 ou 14 chiffres, \
     colonne EAN, Gencod, Code barre ; jamais la référence fournisseur) ; colis = le nombre de \
     colis ou cartons livrés ; pcb = le nombre d'unités par colis (PCB, colisage, « par », \
     « x 6 ») ; quantite = la quantité imprimée dans la colonne quantité ; prix_unitaire_ht = le \
     prix unitaire brut HT avant remise, tel qu'imprimé ; remise_taux = la remise de la ligne en % \
     (5 pour 5 %) ; taux_tva = le taux de TVA de la ligne en % s'il est précisé ; deee_ht = \
     l'éco-participation DEEE (équipements électriques et électroniques, éco-contribution DEEE) \
     HT par unité ; ecomob_ht = l'éco-mobilier (Ecomaison, éco-participation mobilier) HT par \
     unité ; une éco-participation sans précision va dans ecomob_ht pour un meuble ou de la \
     décoration, dans deee_ht pour un appareil électrique ; si seul le montant TTC est donné, \
     divise-le par 1,2 ; pv_conseille = le prix de vente conseillé TTC.\n";

/// Schéma JSON de la réponse. `avec_texte` : demande aussi la transcription (scans).
pub fn schema(avec_texte: bool) -> Value {
    let mut proprietes = json!({
        "type": { "type": "string", "enum": TYPES },
        "societe_id": { "type": ["integer", "null"] },
        "fournisseur": {
            "type": "object",
            "properties": {
                "nom": chaine_ou_null(), "siret": chaine_ou_null(), "tva": chaine_ou_null(),
                "groupe": chaine_ou_null(), "entite": chaine_ou_null()
            },
            "required": ["nom", "siret", "tva", "groupe", "entite"],
            "additionalProperties": false
        },
        "numero": chaine_ou_null(),
        "date": chaine_ou_null(),
        "numero_commande": chaine_ou_null(),
        "echeance": chaine_ou_null(),
        "montant_ht": nombre_ou_null(),
        "montant_tva": nombre_ou_null(),
        "montant_ttc": nombre_ou_null(),
        "remise_ht": nombre_ou_null(),
        "remise_taux": nombre_ou_null(),
        "annexes_ht": nombre_ou_null(),
        "lignes": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "reference": chaine_ou_null(), "designation": chaine_ou_null(),
                    "quantite": nombre_ou_null(), "unite": chaine_ou_null(),
                    "prix_unitaire_ht": nombre_ou_null(), "montant_ht": nombre_ou_null(),
                    "gencod": chaine_ou_null(), "colis": nombre_ou_null(), "pcb": nombre_ou_null(),
                    "remise_taux": nombre_ou_null(), "taux_tva": nombre_ou_null(),
                    "deee_ht": nombre_ou_null(), "ecomob_ht": nombre_ou_null(),
                    "pv_conseille": nombre_ou_null()
                },
                "required": ["reference", "designation", "quantite", "unite", "prix_unitaire_ht",
                             "montant_ht", "gencod", "colis", "pcb", "remise_taux", "taux_tva",
                             "deee_ht", "ecomob_ht", "pv_conseille"],
                "additionalProperties": false
            }
        },
        "numeros_bl": { "type": "array", "items": { "type": "string" } },
        "confiance": {
            "type": "object",
            "properties": {
                "type": confiance(), "societe": confiance(), "fournisseur": confiance(),
                "numero": confiance(), "date": confiance(), "montants": confiance()
            },
            "required": ["type", "societe", "fournisseur", "numero", "date", "montants"],
            "additionalProperties": false
        }
    });
    let mut requis = vec![
        "type",
        "societe_id",
        "fournisseur",
        "numero",
        "date",
        "numero_commande",
        "echeance",
        "montant_ht",
        "montant_tva",
        "montant_ttc",
        "remise_ht",
        "remise_taux",
        "annexes_ht",
        "lignes",
        "numeros_bl",
        "confiance",
    ];
    if avec_texte {
        proprietes["texte"] = json!({ "type": "string" });
        requis.push("texte");
    }
    json!({
        "type": "object",
        "properties": proprietes,
        "required": requis,
        "additionalProperties": false
    })
}

/// Contexte du mail envoyé avec le document : le strict utile.
pub struct ContexteMail<'a> {
    pub expediteur: &'a str,
    pub objet: &'a str,
    pub corps: &'a str,
    pub nom_fichier: &'a str,
}

pub enum Contenu<'a> {
    /// Texte lu par le code (PDF texte, tableur).
    Texte(&'a str),
    /// Fichier à lire par Claude (scan, image), dans le dossier de travail.
    Fichier { nom: &'a str, pages: usize },
}

fn tronquer(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Texte d'au plus `max` caractères : le début et la fin du document (les totaux sont en bas
/// de la dernière page), le milieu (lignes articles) est coupé s'il le faut.
pub fn debut_et_fin(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let fin = max / 3;
    let debut = max - fin;
    let coupe_fin = s.char_indices().nth(n - fin).map_or(s.len(), |(i, _)| i);
    format!(
        "{}\n[... {} caractères du milieu coupés ...]\n{}",
        tronquer(s, debut),
        n - max,
        &s[coupe_fin..]
    )
}

pub fn prompt(mail: &ContexteMail<'_>, societes: &[SocieteRef], contenu: &Contenu<'_>) -> String {
    let mut p = String::new();
    p.push_str("Structures destinataires possibles (societe_id) :\n");
    for s in societes {
        p.push_str(&format!(
            "- {} : {}{} ; SIRET {} ; TVA {} ; adresses : {}\n",
            s.id,
            s.raison_sociale,
            if s.autres_noms.is_empty() {
                String::new()
            } else {
                format!(" (aussi appelée : {})", s.autres_noms.join(" | "))
            },
            s.siret.as_deref().unwrap_or("?"),
            s.tva.as_deref().unwrap_or("?"),
            if s.adresses.is_empty() {
                "?".to_string()
            } else {
                s.adresses.join(" | ")
            }
        ));
    }
    p.push_str(
        "societe_id = la structure destinataire du document (acheteur, client, adresse de \
         livraison), null si aucune ne correspond. Le fournisseur est l'émetteur du document.\n",
    );
    p.push_str(
        "fournisseur.siret = le SIRET (14 chiffres) de l'émetteur, ou à défaut son SIREN (9 \
         chiffres, souvent après « RCS <ville> » ou « SIREN ») ; fournisseur.tva = son n° de TVA \
         intracommunautaire. Cherche-les partout : en-tête, pied de page, mentions légales, \
         conditions de vente, dernière page. Ils servent à reconnaître ce fournisseur sur ses \
         documents suivants : recopie-les chiffre par chiffre, sans espaces. Jamais ceux d'une \
         structure destinataire listée plus haut ; null si l'émetteur n'en donne pas.\n",
    );
    p.push_str(
        "fournisseur.groupe = l'enseigne ou la maison mère de l'émetteur vue en en-tête ou en \
         logo, null sinon. fournisseur.entite = la société, la marque, le département ou la \
         division qui a réellement vendu ou livré, quand le document la distingue du groupe \
         (« Dép. : X », « Département X », « Division X », « Marque : X », « expédié par X » \
         si X n'est pas le groupe) ; null sinon. Exemple : un BL avec le logo « U10 » et le bloc \
         « Dép. : L3C / EXPEDIE PAR : U10 THIZY, 124 Av. des Granges 69240 THIZY LES BOURGS » \
         donne groupe « U10 » et entite « L3C ». Recopie les noms tels qu'écrits, sans forme \
         juridique ni ville.\n",
    );
    p.push_str(&format!(
        "lignes = les lignes articles (référence, désignation, quantité, unité, prix unitaire HT, \
         montant HT de la ligne) : toutes pour un bon de livraison, et pour une facture ou un avoir \
         quand elles sont lisibles ; au plus {LIGNES_MAX} lignes, sans les lignes de total ni de \
         port ; liste vide sinon. numeros_bl = les numéros de bons de livraison cités par une \
         facture (liste vide sinon).\n"
    ));
    p.push_str(DETAIL_LIGNES);
    p.push_str(MONTANTS_ANNEXES);
    p.push_str(REMISES);
    p.push_str(AVOIRS_POSITIFS);
    p.push_str(ECHEANCE);
    p.push('\n');
    p.push_str(&format!(
        "Mail qui accompagne le document :\nDe : {}\nObjet : {}\n{}\n\nFichier : {}\n\n",
        mail.expediteur,
        mail.objet,
        tronquer(mail.corps.trim(), 1000),
        mail.nom_fichier
    ));
    match contenu {
        Contenu::Texte(t) => {
            p.push_str(
                "Les totaux (total HT, TVA, TTC ou net à payer) sont en général en bas de la \
                 dernière page, parfois dans un tableau récapitulatif dont les chiffres sont \
                 séparés de leurs libellés (Base hors taxe, Taux TVA, Montant TVA, Total HT, Total \
                 TVA, Total à payer) ; sur une facture de plusieurs pages, ignore les « report » \
                 et « à reporter » intermédiaires.\n",
            );
            p.push_str("Contenu du document (texte extrait) :\n<<<\n");
            p.push_str(&debut_et_fin(t, TEXTE_MAX));
            p.push_str("\n>>>\n");
        }
        Contenu::Fichier { nom, pages } => {
            let pages = if *pages > 1 {
                format!(" {}", lire_les_pages(*pages).trim_end())
            } else {
                String::new()
            };
            p.push_str(&format!(
                "Le document est le fichier {nom} du dossier courant : lis-le avec l'outil Read.{pages} \
                 Donne aussi dans « texte » sa transcription fidèle (texte brut, lignes de tableau \
                 séparées par des retours à la ligne).\n"
            ));
        }
    }
    p
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ErreurExtraction {
    #[error("réponse sans sortie structurée")]
    Absente,
    #[error("sortie non conforme au schéma : {0}")]
    Schema(String),
}

fn sans_cloture(s: &str) -> &str {
    let s = s.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

/// Réponse JSON de Claude CLI -> extraction contrôlée. La sortie structurée est dans
/// `structured_output` ; à défaut, `result` peut contenir le JSON en texte.
pub fn depuis_reponse(
    reponse: &Value,
    societes: &[SocieteRef],
) -> Result<Extraction, ErreurExtraction> {
    let valeur = match reponse.get("structured_output") {
        Some(v) if v.is_object() => v.clone(),
        _ => {
            let r = reponse
                .get("result")
                .and_then(Value::as_str)
                .ok_or(ErreurExtraction::Absente)?;
            serde_json::from_str(sans_cloture(r)).map_err(|_| ErreurExtraction::Absente)?
        }
    };
    let mut e: Extraction =
        serde_json::from_value(valeur).map_err(|err| ErreurExtraction::Schema(err.to_string()))?;
    controler(&e, societes)?;
    // Une date impossible ne fait pas échouer la lecture : le champ reste vide, et
    // `anomalies` le signale (nouvel essai pour une facture ou un avoir).
    if let Some(d) = e.date.take() {
        if date(&d).is_some() {
            e.date = Some(d);
        } else {
            e.date_illisible = Some(d);
        }
    }
    // Claude s'arrête à LIGNES_MAX lignes (consigne) : autant de lignes, c'est sans doute coupé.
    e.lignes_tronquees = e.lignes.iter().filter(|l| !l.vide()).count() >= LIGNES_MAX;
    e.lignes = lignes_utiles(std::mem::take(&mut e.lignes));
    e.numeros_bl = numeros_utiles(std::mem::take(&mut e.numeros_bl));
    // Une échéance mal formée ne fait pas échouer la lecture : elle est oubliée.
    if e.echeance.as_deref().is_some_and(|d| date(d).is_none()) {
        e.echeance = None;
    }
    Ok(e)
}

fn controler(e: &Extraction, societes: &[SocieteRef]) -> Result<(), ErreurExtraction> {
    let err = |m: String| Err(ErreurExtraction::Schema(m));
    if !TYPES.contains(&e.r#type.as_str()) {
        return err(format!("type inconnu « {} »", e.r#type));
    }
    if let Some(id) = e.societe_id
        && !societes.iter().any(|s| s.id == id)
    {
        return err(format!("societe_id {id} hors de la liste"));
    }
    let c = &e.confiance;
    for v in [
        c.r#type,
        c.societe,
        c.fournisseur,
        c.numero,
        c.date,
        c.montants,
    ] {
        if !(0.0..=1.0).contains(&v) {
            return err(format!("confiance {v} hors de 0..1"));
        }
    }
    Ok(())
}

/// AAAA-MM-JJ -> date.
pub fn date(s: &str) -> Option<time::Date> {
    time::Date::parse(
        s.trim(),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .ok()
}

fn espace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{a0}' | '\u{202f}')
}

/// Lectures possibles d'un nombre sans espace : le dernier « . » ou « , » est la virgule
/// décimale, les autres séparent les milliers ; suivi de 3 chiffres, il peut aussi séparer les
/// milliers (« 1.535 »).
fn lectures(s: &str) -> Vec<f64> {
    let s: String = s.chars().filter(|c| *c != '\'').collect();
    let Some(pos) = s.rfind(['.', ',']) else {
        return s.parse().ok().into_iter().collect();
    };
    let entier: String = s[..pos].chars().filter(char::is_ascii_digit).collect();
    let dec = &s[pos + 1..];
    let mut v = Vec::new();
    let e = if entier.is_empty() { "0" } else { &entier };
    if let Ok(x) = format!("{e}.{dec}").parse() {
        v.push(x);
    }
    if dec.len() == 3
        && let Ok(x) = format!("{entier}{dec}").parse()
    {
        v.push(x);
    }
    v
}

/// Nombres imprimés dans un texte, sous toutes leurs lectures possibles : « 1 535,40 »,
/// « 1.535,40 », « 1,535.40 », « 1535.4 », « 1'535.40 ». Des groupes de 3 chiffres séparés
/// par des espaces (« 12 345 678,90 ») sont aussi lus d'un seul tenant.
pub fn nombres(texte: &str) -> Vec<f64> {
    // Morceaux de chiffres et de séparateurs ; vrai : séparé du précédent par des espaces seuls.
    let mut morceaux: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let (mut colle, mut apres_morceau) = (false, false);
    for c in texte.chars().chain(std::iter::once('\n')) {
        if c.is_ascii_digit() || (!cur.is_empty() && matches!(c, '.' | ',' | '\'')) {
            if cur.is_empty() {
                colle = apres_morceau;
            }
            cur.push(c);
            continue;
        }
        if !cur.is_empty() {
            morceaux.push((cur.trim_end_matches(['.', ',', '\'']).to_string(), colle));
            cur.clear();
            apres_morceau = espace(c);
        } else if !espace(c) {
            apres_morceau = false;
        }
    }
    let chiffres_en_tete = |m: &str| m.chars().take_while(char::is_ascii_digit).count();
    let mut v = Vec::new();
    for (i, (m, _)) in morceaux.iter().enumerate() {
        v.extend(lectures(m));
        if !(1..=3).contains(&m.len()) || chiffres_en_tete(m) != m.len() {
            continue;
        }
        let mut s = m.clone();
        for (suite, colle) in &morceaux[i + 1..] {
            if !colle || chiffres_en_tete(suite) != 3 {
                break;
            }
            s.push_str(suite);
            v.extend(lectures(&s));
            if suite.len() > 3 {
                break;
            }
        }
    }
    v
}

/// Le montant (au centime, signe ignoré) est-il l'un des nombres du texte ?
fn figure(m: f64, nombres: &[f64]) -> bool {
    nombres.iter().any(|n| (n - m.abs()).abs() < 0.005)
}

/// Ce qui manque ou ne tient pas debout dans une lecture de facture, d'avoir ou de BL (vide si
/// rien). `texte` : texte lu par le code dans le fichier, pour vérifier que le montant TTC y
/// figure bien. Une anomalie déclenche un nouvel essai avec un modèle plus fort.
pub fn anomalies(e: &Extraction, texte: Option<&str>) -> Vec<String> {
    let mut a = Vec::new();
    match e.r#type.as_str() {
        "facture" | "avoir" => {
            if e.numero.as_deref().is_none_or(|n| n.trim().is_empty()) {
                a.push("le numéro du document manque".to_string());
            }
            match (&e.date, &e.date_illisible) {
                (None, Some(d)) => a.push(format!("la date lue « {d} » n'existe pas")),
                (None, None) => a.push("la date du document manque".to_string()),
                _ => {}
            }
            match (e.montant_ht, e.montant_tva, e.montant_ttc) {
                (_, _, None) => a.push("le montant TTC manque".to_string()),
                (Some(ht), Some(tva), Some(ttc))
                    if (ht + tva - ttc).abs() > crate::controles::SEUIL_ECART + 1e-9 =>
                {
                    a.push(format!(
                        "HT + TVA ({ht:.2} + {tva:.2}) ne donne pas le TTC ({ttc:.2})"
                    ))
                }
                _ => {}
            }
            if let Some(t) = texte {
                verifier_dans_le_texte(e, &nombres(t), &mut a);
            }
        }
        "bl" if e.lignes.is_empty() => {
            a.push("aucune ligne article lue sur le bon de livraison".to_string());
        }
        _ => {}
    }
    a
}

/// HT, TVA et TTC lus doivent être des nombres du texte. Le HT peut comprendre des montants
/// annexes ajoutés au total HT imprimé (HT − annexes imprimé), et le HT ou la TVA se déduire des
/// deux autres montants quand ceux-ci sont dans le texte et que HT + TVA = TTC (plusieurs taux,
/// TVA des annexes) : seul un montant qui ne vient de nulle part est signalé.
fn verifier_dans_le_texte(e: &Extraction, n: &[f64], a: &mut Vec<String>) {
    let vu = |m: Option<f64>| m.is_some_and(|m| figure(m, n));
    let (ht, tva, ttc) = (e.montant_ht, e.montant_tva, e.montant_ttc);
    let coherent = matches!((ht, tva, ttc), (Some(h), Some(t), Some(c))
        if (h + t - c).abs() <= crate::controles::SEUIL_ECART + 1e-9);
    let ttc_vu = vu(ttc);
    let ht_vu = vu(ht)
        || ht
            .zip(e.annexes_ht)
            .is_some_and(|(h, x)| vu(Some(h - x.abs())))
        || (coherent && ttc_vu && vu(tva));
    let tva_vu = vu(tva) || (coherent && ttc_vu && ht_vu);
    for (nom, m, ok) in [
        ("TTC", ttc, ttc_vu),
        ("HT", ht, ht_vu),
        ("de TVA", tva, tva_vu),
    ] {
        if let (Some(m), false) = (m, ok) {
            a.push(format!(
                "le montant {nom} {m:.2} n'apparaît pas dans le texte du document"
            ));
        }
    }
}

/// Consigne ajoutée au prompt du nouvel essai : ce que la première lecture a manqué, et le
/// fichier d'origine quand il est joint (PDF texte relu sur pièce).
pub fn consigne_nouvel_essai(
    anomalies: &[String],
    fichier: Option<&str>,
    pages: Option<usize>,
) -> String {
    let mut s = String::from(
        "\nUne première lecture de ce document a été jugée incomplète ou incohérente :\n",
    );
    for a in anomalies {
        s.push_str(&format!("- {a}\n"));
    }
    s.push_str(
        "Relis tout le document avec attention, en particulier le pied de la dernière page \
         (total HT, TVA, net à payer TTC). Recopie les montants tels qu'ils sont imprimés ; \
         null seulement si le document ne les donne vraiment pas.\n",
    );
    s.push_str(MONTANTS_ANNEXES);
    s.push_str(REMISES);
    s.push_str(AVOIRS_POSITIFS);
    s.push_str(ECHEANCE);
    if let Some(f) = fichier {
        s.push_str(&format!(
            "Le fichier d'origine est aussi le fichier {f} du dossier courant : lis-le avec \
             l'outil Read, le texte extrait ci-dessus peut mélanger les colonnes.\n"
        ));
    }
    if let Some(n) = pages.filter(|n| *n > 1) {
        s.push_str(&lire_les_pages(n));
    }
    s
}

/// Consigne de lecture d'un fichier de `n` pages avec l'outil Read : les totaux sont en bas de
/// la dernière page, et Read ne lit pas plus de 10 pages d'un PDF à la fois.
pub fn lire_les_pages(n: usize) -> String {
    if n <= 10 {
        return format!(
            "Le fichier a {n} pages : lis-les toutes, jusqu'au pied de la dernière (totaux).\n"
        );
    }
    let debut_fin = n.saturating_sub(2).max(1);
    format!(
        "Le fichier a {n} pages. L'outil Read lit au plus 10 pages d'un PDF à la fois : passe-lui \
         le paramètre pages. Lis d'abord les pages 1-2 (en-tête, numéro, date, émetteur), puis \
         les pages {debut_fin}-{n} (le récapitulatif des totaux est en bas de la dernière page, \
         souvent un tableau Base hors taxe / Taux TVA / Montant TVA / Total HT / Total TVA / Total \
         à payer), puis les autres par lots de 10 si tu as besoin des lignes.\n"
    )
}

/// Numéros non vides et distincts (au plus 50).
pub fn numeros_utiles(numeros: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for n in numeros {
        let n = n.trim().to_string();
        if !n.is_empty() && !v.contains(&n) && v.len() < 50 {
            v.push(n);
        }
    }
    v
}

#[cfg(test)]
mod tests_montants_bl {
    use super::*;

    fn ligne(q: f64, pu: Option<f64>) -> Ligne {
        Ligne {
            reference: None,
            designation: Some("Article".into()),
            quantite: Some(q),
            unite: None,
            prix_unitaire_ht: pu,
            montant_ht: None,
            gencod: None,
            colis: None,
            pcb: None,
            remise_taux: None,
            taux_tva: None,
            deee_ht: None,
            ecomob_ht: None,
            pv_conseille: None,
        }
    }

    #[test]
    fn bl_sans_totaux() {
        // Lignes du BL LL1261103672 : PARURE 4 × 14,90 ; RIDEAU 6 × 4,50 + éco 0,11 ;
        // COUSSIN 6 × 2,20 + éco 0,30.
        let mut l = vec![
            ligne(4.0, Some(14.9)),
            ligne(6.0, Some(4.5)),
            ligne(6.0, Some(2.2)),
        ];
        l[1].deee_ht = Some(0.11);
        l[2].ecomob_ht = Some(0.3);
        let (mut ht, mut tva, mut ttc) = (None, None, None);
        assert_eq!(
            completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc),
            ["ht", "tva", "ttc"]
        );
        assert_eq!(l[0].montant_ht, Some(59.6));
        assert_eq!(l[1].montant_ht, Some(27.0));
        // 59,60 + 27,00 + 0,66 + 13,20 + 1,80
        assert_eq!(ht, Some(102.26));
        assert_eq!(tva, Some(20.45));
        assert_eq!(ttc, Some(122.71));
    }

    #[test]
    fn remise_taux_et_ht_imprime() {
        let mut l = vec![ligne(10.0, Some(10.0)), ligne(2.0, Some(50.0))];
        l[0].remise_taux = Some(10.0);
        l[1].taux_tva = Some(5.5);
        let (mut ht, mut tva, mut ttc) = (None, None, None);
        assert_eq!(
            completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc).len(),
            3
        );
        assert_eq!(ht, Some(190.0));
        assert_eq!(tva, Some(23.5)); // 90 × 20 % + 100 × 5,5 %
        assert_eq!(ttc, Some(213.5));
        // HT imprimé : gardé, seuls TVA et TTC sont calculés.
        let (mut ht, mut tva, mut ttc) = (Some(100.0), None, None);
        let mut l = vec![ligne(1.0, Some(100.0))];
        assert_eq!(
            completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc),
            ["tva", "ttc"]
        );
        assert_eq!((ht, tva, ttc), (Some(100.0), Some(20.0), Some(120.0)));
    }

    #[test]
    fn rien_si_une_ligne_sans_prix_ou_tout_imprime() {
        let mut l = vec![ligne(4.0, Some(14.9)), ligne(6.0, None)];
        let (mut ht, mut tva, mut ttc) = (None, None, None);
        assert!(completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc).is_empty());
        assert_eq!((ht, tva, ttc), (None, None, None));
        let mut l = vec![ligne(1.0, Some(10.0))];
        let (mut ht, mut tva, mut ttc) = (Some(11.0), Some(2.2), Some(13.2));
        assert!(completer_montants_bl(&mut l, &mut ht, &mut tva, &mut ttc).is_empty());
        assert_eq!(ht, Some(11.0));
        assert!(completer_montants_bl(&mut [], &mut None, &mut None, &mut None).is_empty());
    }
}

#[cfg(test)]
mod tests_pages {
    use super::*;

    #[test]
    fn pages_lues() {
        let c = lire_les_pages(30);
        assert!(c.contains("pages 28-30"), "{c}");
        assert!(lire_les_pages(4).contains("4 pages"));
        let c = consigne_nouvel_essai(&["x".into()], Some("document.pdf"), Some(30));
        assert!(c.contains("document.pdf") && c.contains("pages 28-30"));
        assert!(!consigne_nouvel_essai(&[], None, Some(1)).contains("pages"));
    }
}
