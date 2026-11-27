//! Lecture par tranches des très gros PDF (suite de L2). Un document qui ne tient pas en un
//! appel (texte de plus de `extraction::TEXTE_MAX` caractères, ou plus de `LIGNES_MAX` lignes)
//! est lu en plusieurs fois : un appel pour l'en-tête et les totaux (premières et dernières
//! pages), puis les lignes d'articles tranche par tranche (pages entières), et la fusion par le
//! code. Le code contrôle ensuite que rien ne manque (somme des lignes = total HT lu, nombre de
//! lignes annoncé) : sinon la pièce garde le marquage `lignes_tronquees`.
//!
//! Lignes à cheval sur deux pages : chaque tranche reçoit la fin de la page précédente et le
//! début de la suivante comme contexte, avec la consigne « une ligne coupée appartient à la
//! page où elle commence » ; le code retire en plus le doublon éventuel à la jonction. Les
//! en-têtes de tableau répétés, les reports et sous-totaux sont exclus par consigne, puis par
//! le code (`parasite`).
//!
//! Ce module ne fait que le calcul (plan, prompts, schémas, fusion, contrôle) ; les appels et
//! l'avancement en base sont dans le worker (`lecture::tranches`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::extraction::{self, Ligne};
use crate::montant::{self, Decimal, dec};

/// Caractères au plus du texte d'une tranche de lignes (hors contexte des pages voisines).
pub const TRANCHE_TEXTE_MAX: usize = 9_000;
/// Lignes de texte au plus par tranche : une ligne de texte vaut à peu près une ligne article,
/// la réponse (environ 80 jetons par ligne) reste loin de la limite de sortie du modèle.
pub const TRANCHE_LIGNES_TEXTE_MAX: usize = 120;
/// Pages au plus par tranche (l'outil Read lit au plus 10 pages, une page scannée coûte cher).
pub const TRANCHE_PAGES_MAX: usize = 4;
/// Tranches de lignes au plus par document : au-delà, les pages restantes ne sont pas lues et
/// la lecture est signalée incomplète.
pub const TRANCHES_MAX: usize = 15;
/// Poids d'une page sans texte (scan que l'OCR n'a pas lu) : comptée comme une page pleine.
const PAGE_SANS_TEXTE_CARACTERES: usize = 3_000;
const PAGE_SANS_TEXTE_LIGNES: usize = 40;
/// Caractères utiles en dessous desquels une page est sans texte (comme `ocr::PAGE_TEXTE_MIN`).
const PAGE_TEXTE_MIN: usize = 40;
/// Lignes de la page voisine données en contexte.
const CONTEXTE_LIGNES: usize = 6;
/// Écart toléré entre la somme des lignes et le total HT lu.
pub const ECART_SOMME: Decimal = dec!(0.05);
/// Écart en dessous duquel deux nombres lus (quantité, prix, montant) sont égaux.
const DEMI_CENTIME: Decimal = dec!(0.005);

/// Clé de `document.confiance` : bilan de la lecture par tranches.
pub const CLE_CONFIANCE: &str = "tranches";

/// Une tranche : pages `debut` à `fin` (numérotées à partir de 1, bornes comprises).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tranche {
    pub rang: usize,
    pub debut: usize,
    pub fin: usize,
}

fn utile(t: &str) -> usize {
    t.chars().filter(|c| c.is_alphanumeric()).count()
}

fn sans_texte(t: &str) -> bool {
    utile(t) < PAGE_TEXTE_MIN
}

/// Poids d'une page : (caractères, lignes de texte non vides).
fn poids(t: &str) -> (usize, usize) {
    if sans_texte(t) {
        return (PAGE_SANS_TEXTE_CARACTERES, PAGE_SANS_TEXTE_LIGNES);
    }
    (
        t.chars().count(),
        t.lines().filter(|l| !l.trim().is_empty()).count(),
    )
}

/// Le document tient-il en un seul appel ? Non (à lire par tranches) quand il a au moins deux
/// pages et que son texte dépasse `TEXTE_MAX` caractères (le milieu serait coupé) ou compte
/// plus de `LIGNES_MAX` lignes contenant des chiffres (Claude s'arrête à `LIGNES_MAX` lignes).
/// Les pages sans texte (scan non lu par l'OCR) comptent comme pleines.
pub fn a_decouper(textes: &[String]) -> bool {
    if textes.len() < 2 {
        return false;
    }
    let caracteres: usize = textes.iter().map(|t| poids(t).0).sum();
    let lignes_chiffrees: usize = textes
        .iter()
        .map(|t| {
            if sans_texte(t) {
                PAGE_SANS_TEXTE_LIGNES
            } else {
                t.lines()
                    .filter(|l| l.chars().any(|c| c.is_ascii_digit()))
                    .count()
            }
        })
        .sum();
    caracteres > extraction::TEXTE_MAX || lignes_chiffrees > extraction::LIGNES_MAX
}

/// Découpe les pages en tranches contiguës : chaque tranche prend des pages entières tant
/// qu'elle reste sous `TRANCHE_TEXTE_MAX` caractères, `TRANCHE_LIGNES_TEXTE_MAX` lignes et
/// `TRANCHE_PAGES_MAX` pages (une page plus grosse forme une tranche à elle seule).
pub fn plan(textes: &[String]) -> Vec<Tranche> {
    let mut v: Vec<Tranche> = Vec::new();
    let (mut car, mut lig, mut debut) = (0usize, 0usize, 1usize);
    for (i, t) in textes.iter().enumerate() {
        let page = i + 1;
        let (c, l) = poids(t);
        let pages = page - debut;
        if pages > 0
            && (car + c > TRANCHE_TEXTE_MAX
                || lig + l > TRANCHE_LIGNES_TEXTE_MAX
                || pages >= TRANCHE_PAGES_MAX)
        {
            v.push(Tranche {
                rang: v.len() + 1,
                debut,
                fin: page - 1,
            });
            (car, lig, debut) = (0, 0, page);
        }
        car += c;
        lig += l;
    }
    if !textes.is_empty() {
        v.push(Tranche {
            rang: v.len() + 1,
            debut,
            fin: textes.len(),
        });
    }
    v
}

/// Pages laissées de côté par le plafond `TRANCHES_MAX` (bornes comprises), s'il y en a.
pub fn hors_plafond(plan: &[Tranche]) -> Option<(usize, usize)> {
    let fin = plan.last()?.fin;
    plan.get(TRANCHES_MAX).map(|t| (t.debut, fin))
}

/// Pages lues pour l'en-tête et les totaux : les deux premières et les deux dernières.
pub fn pages_entete(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = [1, 2, n.saturating_sub(1), n]
        .into_iter()
        .filter(|p| (1..=n).contains(p))
        .collect();
    v.dedup();
    v
}

fn page_en_texte(textes: &[String], p: usize) -> String {
    let t = textes.get(p - 1).map(String::as_str).unwrap_or("");
    if sans_texte(t) {
        format!("--- Page {p} ---\n(page sans texte lisible : lis-la dans le fichier)\n")
    } else {
        format!("--- Page {p} ---\n{}\n", t.trim_end())
    }
}

/// Texte de l'appel d'en-tête : premières et dernières pages, le milieu remplacé par une
/// mention (les lignes y sont lues à part).
pub fn texte_entete(textes: &[String]) -> String {
    let n = textes.len();
    let mut s = String::new();
    let mut prec = 0;
    for p in pages_entete(n) {
        if p > prec + 1 {
            s.push_str(&format!(
                "[... pages {} à {} : lignes articles, lues à part ...]\n",
                prec + 1,
                p - 1
            ));
        }
        s.push_str(&page_en_texte(textes, p));
        prec = p;
    }
    s
}

/// Consigne ajoutée au prompt de l'appel d'en-tête. `fichier` : le PDF est joint (scan, OCR,
/// nouvel essai).
pub fn consigne_entete(pages: usize, fichier: Option<&str>) -> String {
    let mut s = format!(
        "\nCe document de {pages} pages est long : il est lu en plusieurs parties. Tu n'as ici que \
         ses premières et ses dernières pages. Donne l'en-tête et les totaux (type, structure, \
         fournisseur, numéro, dates, commande, montants, remises, IBAN, numéros de BL cités). \
         lignes = liste vide : les lignes articles sont lues à part, page par page. \
         nombre_lignes_annonce = le nombre de lignes ou de références que le document imprime \
         (« Nombre de lignes : 412 », « Nb de références »), null s'il ne le donne pas ; \
         quantite_totale_annoncee = la quantité totale ou le nombre total de colis imprimé en \
         pied (« Total quantité », « Total colis »), null sinon.\n"
    );
    if let Some(f) = fichier {
        let fin = pages.saturating_sub(1).max(1);
        s.push_str(&format!(
            "Le document d'origine est le fichier {f} du dossier courant ; le texte ci-dessus peut \
             venir d'une reconnaissance de caractères (OCR) et contenir des erreurs. Lis avec \
             l'outil Read seulement les pages 1-2 et {fin}-{pages} (paramètre pages) et, en cas \
             de doute, fie-toi au fichier, surtout pour les montants, les numéros et les dates. Ne \
             donne pas de transcription.\n"
        ));
    }
    s
}

/// Schéma de l'appel d'en-tête : celui de la lecture, plus les annonces du document.
pub fn schema_entete() -> Value {
    let mut s = extraction::schema(false);
    s["properties"]["nombre_lignes_annonce"] = json!({ "type": ["integer", "null"] });
    s["properties"]["quantite_totale_annoncee"] = json!({ "type": ["number", "null"] });
    if let Some(r) = s["required"].as_array_mut() {
        r.push(json!("nombre_lignes_annonce"));
        r.push(json!("quantite_totale_annoncee"));
    }
    s
}

/// Ce que le document annonce de lui-même, pour le contrôle de complétude.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Annonces {
    #[serde(default)]
    pub nombre_lignes_annonce: Option<i64>,
    #[serde(default)]
    pub quantite_totale_annoncee: Option<Decimal>,
}

/// Annonces lues dans la réponse de l'appel d'en-tête (vides si absentes ou mal formées).
pub fn annonces(reponse: &Value) -> Annonces {
    crate::claude::sortie_structuree(reponse)
        .and_then(|v| serde_json::from_value(v).ok())
        .map(|a: Annonces| Annonces {
            nombre_lignes_annonce: a.nombre_lignes_annonce.filter(|n| *n > 0),
            quantite_totale_annoncee: a
                .quantite_totale_annoncee
                .and_then(montant::quantite)
                .filter(|q| *q > Decimal::ZERO),
        })
        .unwrap_or_default()
}

/// Schéma de la réponse d'une tranche : les lignes seules.
pub fn schema_lignes() -> Value {
    let lignes = extraction::schema(false)["properties"]["lignes"].clone();
    json!({
        "type": "object",
        "properties": { "lignes": lignes },
        "required": ["lignes"],
        "additionalProperties": false
    })
}

/// Prompt système des tranches de lignes.
pub const PROMPT_SYSTEME: &str = "Tu recopies les lignes articles d'une partie d'un long \
document reçu par un magasin (bon de livraison, facture ou avoir). Tu réponds uniquement avec la \
sortie structurée demandée, selon le schéma. Tu n'inventes rien : un champ absent ou illisible \
vaut null. Les montants sont en euros avec un point décimal.";

fn lignes_non_vides(t: &str) -> Vec<&str> {
    t.lines().filter(|l| !l.trim().is_empty()).collect()
}

/// Dernières lignes d'une page (contexte avant).
fn fin_de_page(t: &str) -> String {
    let l = lignes_non_vides(t);
    l[l.len().saturating_sub(CONTEXTE_LIGNES)..].join("\n")
}

/// Premières lignes d'une page (contexte après), en-tête de tableau répété compris.
fn debut_de_page(t: &str) -> String {
    lignes_non_vides(t)
        .into_iter()
        .take(CONTEXTE_LIGNES + 2)
        .collect::<Vec<_>>()
        .join("\n")
}

fn genre(type_doc: Option<&str>) -> &'static str {
    match type_doc {
        Some("bl") => "un bon de livraison",
        Some("facture") => "une facture",
        Some("avoir") => "un avoir",
        _ => "un document",
    }
}

/// Prompt d'une tranche de lignes. `fichier` : le PDF est joint (scan, OCR, nouvel essai), à
/// lire seulement sur les pages de la tranche.
pub fn prompt_tranche(
    textes: &[String],
    t: &Tranche,
    type_doc: Option<&str>,
    fournisseur: Option<&str>,
    fichier: Option<&str>,
) -> String {
    let n = textes.len();
    let pages = if t.debut == t.fin {
        format!("la page {}", t.debut)
    } else {
        format!("les pages {} à {}", t.debut, t.fin)
    };
    let mut p = format!(
        "Ce document est {}{} de {n} pages, lu en plusieurs parties. Cette partie couvre {pages} \
         ; les autres pages sont lues à part.\n",
        genre(type_doc),
        fournisseur
            .filter(|f| !f.trim().is_empty())
            .map(|f| format!(" de {}", f.trim()))
            .unwrap_or_default()
    );
    p.push_str(&format!(
        "lignes = toutes les lignes articles de {pages}, dans l'ordre du document, sans en sauter, \
         sans en résumer et sans limite de nombre. Ne sont pas des lignes articles : les en-têtes \
         de tableau (souvent répétés en haut de chaque page), les « report » et « à reporter », \
         les sous-totaux et totaux, les frais de port, les mentions et les pieds de page.\n"
    ));
    let avant = (t.debut > 1).then(|| fin_de_page(&textes[t.debut - 2]));
    let apres = (t.fin < n).then(|| debut_de_page(&textes[t.fin]));
    if avant.is_some() {
        p.push_str(&format!(
            "Le texte « AVANT » est la fin de la page {}, lue avec la partie précédente : n'en \
             recopie aucune ligne. Si le haut de la page {} ne fait que terminer une ligne \
             commencée dans ce texte (suite de désignation, montants seuls), ne la recopie pas : \
             elle appartient à la page où elle commence.\n",
            t.debut - 1,
            t.debut
        ));
    }
    if apres.is_some() {
        p.push_str(&format!(
            "Le texte « APRÈS » est le début de la page {}, lue avec la partie suivante : n'en \
             recopie aucune ligne, sauf pour compléter la dernière ligne de la page {} si elle se \
             poursuit là (recopie-la alors une seule fois, en entier).\n",
            t.fin + 1,
            t.fin
        ));
    }
    p.push_str(extraction::DETAIL_LIGNES);
    p.push_str(extraction::AVOIRS_POSITIFS);
    if let Some(f) = fichier {
        let plage = if t.debut == t.fin {
            t.debut.to_string()
        } else {
            format!("{}-{}", t.debut, t.fin)
        };
        p.push_str(&format!(
            "Le document d'origine est le fichier {f} du dossier courant : lis avec l'outil Read \
             seulement les pages {plage} (paramètre pages). Le texte ci-dessous peut venir d'une \
             reconnaissance de caractères (OCR) ou mélanger les colonnes : en cas de doute, \
             fie-toi au fichier. Ne donne pas de transcription.\n"
        ));
    }
    p.push('\n');
    if let Some(a) = avant {
        p.push_str(&format!(
            "AVANT (fin de la page {}, ne pas recopier) :\n<<<\n{a}\n>>>\n",
            t.debut - 1
        ));
    }
    p.push_str(&format!("Texte de {pages} :\n<<<\n"));
    for page in t.debut..=t.fin {
        p.push_str(&page_en_texte(textes, page));
    }
    p.push_str(">>>\n");
    if let Some(a) = apres {
        p.push_str(&format!(
            "APRÈS (début de la page {}, ne pas recopier) :\n<<<\n{a}\n>>>\n",
            t.fin + 1
        ));
    }
    p
}

/// Lignes d'une réponse de tranche (`structured_output`, à défaut le JSON de `result`).
pub fn lignes_de_reponse(reponse: &Value) -> Option<Vec<Ligne>> {
    #[derive(Deserialize)]
    struct R {
        lignes: Vec<Ligne>,
    }
    let v = crate::claude::sortie_structuree(reponse)?;
    let mut lignes = serde_json::from_value::<R>(v).ok()?.lignes;
    lignes.iter_mut().for_each(Ligne::normaliser);
    Some(lignes)
}

/// Libellés d'en-tête de tableau, de report et de total, qui ne sont pas des lignes articles.
const MOTS_ENTETE: &[&str] = &[
    "designation",
    "libelle",
    "article",
    "articles",
    "description",
    "reference",
    "ref",
    "code",
    "quantite",
    "qte",
    "prix",
];
const DEBUTS_REPORT: &[&str] = &[
    "report",
    "a reporter",
    "reporte",
    "sous total",
    "sous-total",
    "total",
    "net a payer",
];

/// Ligne qui n'est pas un article : en-tête de tableau répété (que des libellés, aucun nombre)
/// ou report / sous-total (sans référence, sans code-barres, sans quantité).
pub fn parasite(l: &Ligne) -> bool {
    use super::tableau_articles::normaliser;
    let texte = |s: &Option<String>| s.as_deref().map(normaliser).unwrap_or_default();
    let (r, d) = (texte(&l.reference), texte(&l.designation));
    let sans_nombre = l.quantite.is_none()
        && l.montant_ht.is_none()
        && l.prix_unitaire_ht.is_none()
        && l.colis.is_none();
    let libelles = |s: &str| {
        !s.is_empty()
            && s.split_whitespace()
                .all(|m| MOTS_ENTETE.contains(&m) || m.len() <= 2)
    };
    if sans_nombre && (libelles(&r) || r.is_empty()) && (libelles(&d) || d.is_empty()) {
        return true;
    }
    let sans_article = r.is_empty()
        && l.gencod.as_deref().is_none_or(|g| g.trim().is_empty())
        && l.quantite.is_none()
        && l.colis.is_none();
    sans_article
        && DEBUTS_REPORT
            .iter()
            .any(|m| d == *m || d.starts_with(&format!("{m} ")))
}

fn cle(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(crate::rapprochement::normaliser)
        .filter(|s| !s.is_empty())
}

fn egaux(a: Option<Decimal>, b: Option<Decimal>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => (x - y).abs() < DEMI_CENTIME,
        (None, None) => true,
        _ => false,
    }
}

/// Même ligne lue deux fois à la jonction de deux tranches : mêmes nombres (quantité, prix et
/// montant, au moins un présent) et même référence ou code-barres, ou l'une des deux sans
/// référence (morceau de la ligne coupée), ou, sans référence des deux côtés, des désignations
/// dont l'une prolonge l'autre.
pub fn meme_ligne(a: &Ligne, b: &Ligne) -> bool {
    let nombres = egaux(a.quantite, b.quantite)
        && egaux(a.prix_unitaire_ht, b.prix_unitaire_ht)
        && egaux(a.montant_ht, b.montant_ht)
        && (a.quantite.is_some() || a.montant_ht.is_some());
    if !nombres {
        return false;
    }
    for (x, y) in [(&a.reference, &b.reference), (&a.gencod, &b.gencod)] {
        match (cle(x), cle(y)) {
            (Some(x), Some(y)) => return x == y,
            // L'une a l'identifiant, l'autre non : morceau de la ligne coupée.
            (Some(_), None) | (None, Some(_)) => return true,
            (None, None) => {}
        }
    }
    match (cle(&a.designation), cle(&b.designation)) {
        (Some(x), Some(y)) => x == y || x.ends_with(&y) || y.starts_with(&x) || x.starts_with(&y),
        _ => true,
    }
}

/// Fusion des lignes des tranches, dans l'ordre : lignes vides et parasites retirées, puis le
/// doublon de la jonction (première ligne d'une tranche = dernière de la précédente).
/// Rend les lignes et le nombre de doublons retirés.
pub fn fusionner(parties: &[Vec<Ligne>]) -> (Vec<Ligne>, usize) {
    let mut v: Vec<Ligne> = Vec::new();
    let mut doublons = 0;
    for partie in parties {
        let mut premiere = true;
        for l in partie.iter().filter(|l| !l.vide() && !parasite(l)) {
            if premiere && v.last().is_some_and(|d| meme_ligne(d, l)) {
                doublons += 1;
                premiere = false;
                continue;
            }
            premiere = false;
            v.push(l.clone());
        }
    }
    (v, doublons)
}

/// Pages en plages lisibles : [9, 10, 11, 12, 15] → « pages 9 à 12 et 15 ».
pub fn plages(pages: &[usize]) -> String {
    let mut p = pages.to_vec();
    p.sort_unstable();
    p.dedup();
    let mut morceaux: Vec<String> = Vec::new();
    let mut i = 0;
    while i < p.len() {
        let mut j = i;
        while j + 1 < p.len() && p[j + 1] == p[j] + 1 {
            j += 1;
        }
        morceaux.push(if i == j {
            p[i].to_string()
        } else {
            format!("{} à {}", p[i], p[j])
        });
        i = j + 1;
    }
    let mot = if p.len() > 1 { "pages" } else { "page" };
    match morceaux.len() {
        0 => String::new(),
        1 => format!("{mot} {}", morceaux[0]),
        k => format!(
            "{mot} {} et {}",
            morceaux[..k - 1].join(", "),
            morceaux[k - 1]
        ),
    }
}

/// Montants lus en pied, pour le contrôle de complétude.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totaux {
    pub ht: Option<Decimal>,
    pub annexes_ht: Option<Decimal>,
    pub remise_ht: Option<Decimal>,
}

/// Bilan du contrôle de complétude.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Controle {
    /// Rien ne manque d'après ce qui a pu être vérifié.
    pub complet: bool,
    /// Ce qui ne tombe pas juste ou n'a pas été lu (phrases).
    pub raisons: Vec<String>,
    /// Contrôles réussis : « somme_ht », « nombre_lignes », « quantite_totale ».
    pub verifie: Vec<&'static str>,
}

/// Montant d'une ligne : celui imprimé, sinon quantité (ou colis × PCB) × prix, moins la remise.
fn montant_ligne(l: &Ligne) -> Option<Decimal> {
    l.montant_ht.or_else(|| {
        let q = l.quantite.or(l.colis.zip(l.pcb).map(|(c, p)| c * p))?;
        let r = l
            .remise_taux
            .filter(|r| *r > Decimal::ZERO && *r < Decimal::ONE_HUNDRED)
            .unwrap_or_default();
        Some(q * l.prix_unitaire_ht? * (Decimal::ONE - r / Decimal::ONE_HUNDRED))
    })
}

fn somme(v: impl Iterator<Item = Decimal>) -> Decimal {
    montant::centimes(v.sum())
}

fn euros(m: Decimal) -> String {
    montant::euros(m).replace('.', ",")
}

/// Quantité pour une phrase : « 12 », « 12,5 ».
fn quantite_texte(q: Decimal) -> String {
    q.normalize().to_string().replace('.', ",")
}

/// Contrôle par le code, après fusion : pages non lues, somme des lignes comparée au total HT
/// lu (avec ou sans les montants annexes, les éco-participations des lignes et la remise de
/// pied), nombre de lignes et quantité totale comparés à ce que le document annonce. Complet
/// seulement si toutes les pages sont lues et que rien de ce qui a pu être comparé ne diffère.
pub fn controler(
    lignes: &[Ligne],
    totaux: Totaux,
    annonces: Annonces,
    pages_non_lues: &[usize],
) -> Controle {
    let mut c = Controle::default();
    if !pages_non_lues.is_empty() {
        let s = plages(pages_non_lues);
        let accord = if s.starts_with("pages") {
            "non lues"
        } else {
            "non lue"
        };
        c.raisons.push(format!("{s} {accord}"));
    }
    if lignes.is_empty() {
        c.raisons.push("aucune ligne article lue".into());
    }
    let montants: Option<Vec<Decimal>> = lignes.iter().map(montant_ligne).collect();
    if let (Some(ht), Some(m)) = (totaux.ht, montants.filter(|m| !m.is_empty())) {
        let s = somme(m.into_iter());
        let eco = extraction::eco_lignes(lignes);
        let annexes = totaux.annexes_ht.unwrap_or_default().abs();
        let remise = totaux.remise_ht.unwrap_or_default().abs();
        let tombe_juste = [Decimal::ZERO, eco, annexes]
            .into_iter()
            .flat_map(|plus| [s + plus, s + plus - remise])
            .any(|calcule| (calcule - ht.abs()).abs() <= ECART_SOMME);
        if tombe_juste {
            c.verifie.push("somme_ht");
        } else {
            c.raisons.push(format!(
                "la somme des lignes ({} €) ne donne pas le total HT lu ({} €)",
                euros(s),
                euros(ht.abs())
            ));
        }
    }
    if let Some(n) = annonces.nombre_lignes_annonce {
        if n == lignes.len() as i64 {
            c.verifie.push("nombre_lignes");
        } else {
            c.raisons.push(format!(
                "le document annonce {n} lignes, {} lues",
                lignes.len()
            ));
        }
    }
    if let Some(q) = annonces.quantite_totale_annoncee {
        let quantites = somme(
            lignes
                .iter()
                .filter_map(|l| l.quantite.or(l.colis.zip(l.pcb).map(|(c, p)| c * p))),
        );
        let colis = somme(lignes.iter().filter_map(|l| l.colis));
        if [quantites, colis]
            .iter()
            .any(|x| (x - q).abs() < DEMI_CENTIME)
        {
            c.verifie.push("quantite_totale");
        } else {
            c.raisons.push(format!(
                "le document annonce une quantité totale de {}, les lignes donnent {}",
                quantite_texte(q),
                quantite_texte(quantites)
            ));
        }
    }
    c.complet = c.raisons.is_empty();
    c
}

/// Phrase ajoutée aux anomalies de la lecture (`document.message`, « Lecture à vérifier : … ») :
/// rien si tout est lu et vérifié ; sinon ce qui manque (« lecture incomplète (document lu en 7
/// parties) : pages 9 à 12 non lues ») ou ce qui n'a pas pu être vérifié.
pub fn phrase(parties: usize, c: &Controle) -> Option<String> {
    if !c.complet {
        return Some(format!(
            "lecture incomplète (document lu en {parties} parties) : {}",
            c.raisons.join(" ; ")
        ));
    }
    if c.verifie.is_empty() {
        return Some(format!(
            "document lu en {parties} parties, sans total ni nombre de lignes imprimé pour contrôler \
             qu'aucune ligne ne manque"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: usize, lignes: usize) -> String {
        (0..lignes)
            .map(|i| format!("REF{n:02}{i:03} Article numéro {i} de la page {n}   2   1,50   3,00"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn ligne(r: &str, q: Decimal, m: Decimal) -> Ligne {
        Ligne {
            reference: Some(r.into()),
            designation: Some(format!("Article {r}")),
            quantite: Some(q),
            montant_ht: Some(m),
            ..Default::default()
        }
    }

    #[test]
    fn petit_document_non_decoupe_gros_decoupe_aux_pages() {
        assert!(!a_decouper(&[page(1, 30)]));
        assert!(!a_decouper(&[page(1, 30), page(2, 30)]));
        let gros: Vec<String> = (1..=15).map(|n| page(n, 28)).collect();
        assert!(a_decouper(&gros));
        let p = plan(&gros);
        // 28 lignes d'environ 60 caractères : 4 pages par tranche (plafond de pages).
        assert_eq!(p.len(), 4, "{p:?}");
        assert_eq!(
            p[0],
            Tranche {
                rang: 1,
                debut: 1,
                fin: 4
            }
        );
        assert_eq!(p.last().unwrap().fin, 15);
        // Contiguës, sans trou ni recouvrement.
        for w in p.windows(2) {
            assert_eq!(w[1].debut, w[0].fin + 1);
        }
        // Page très longue : seule dans sa tranche.
        let mut v = vec![page(1, 10), page(2, 300), page(3, 10)];
        v.push(page(4, 10));
        let p = plan(&v);
        assert_eq!(
            p.iter().map(|t| (t.debut, t.fin)).collect::<Vec<_>>(),
            [(1, 1), (2, 2), (3, 4)]
        );
        // Beaucoup de lignes courtes : découpé même sous 20 000 caractères.
        let courtes: Vec<String> = (0..3)
            .map(|_| {
                (0..80)
                    .map(|i| format!("A{i} 1"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        assert!(a_decouper(&courtes));
    }

    #[test]
    fn plafond_et_pages_d_entete() {
        let gros: Vec<String> = (1..=70).map(|n| page(n, 28)).collect();
        let p = plan(&gros);
        assert!(p.len() > TRANCHES_MAX);
        assert_eq!(hors_plafond(&p), Some((p[TRANCHES_MAX].debut, 70)));
        assert_eq!(hors_plafond(&p[..3]), None);
        assert_eq!(pages_entete(15), [1, 2, 14, 15]);
        assert_eq!(pages_entete(3), [1, 2, 3]);
        assert_eq!(pages_entete(1), [1]);
        let t = texte_entete(&gros[..15]);
        assert!(t.contains("--- Page 2 ---") && t.contains("--- Page 14 ---"));
        assert!(t.contains("[... pages 3 à 13"));
        assert!(!t.contains("--- Page 3 ---"));
    }

    #[test]
    fn prompt_d_une_tranche_avec_contexte_des_pages_voisines() {
        let v: Vec<String> = (1..=6).map(|n| page(n, 10)).collect();
        let t = Tranche {
            rang: 2,
            debut: 3,
            fin: 4,
        };
        let p = prompt_tranche(&v, &t, Some("bl"), Some("U10"), None);
        assert!(p.contains("un bon de livraison de U10 de 6 pages"));
        assert!(p.contains("les pages 3 à 4"));
        assert!(p.contains("AVANT (fin de la page 2"));
        assert!(
            p.contains("REF02009") && !p.contains("REF02003"),
            "6 dernières lignes de la page 2"
        );
        assert!(p.contains("APRÈS (début de la page 5"));
        assert!(p.contains("REF05000") && !p.contains("REF06"));
        assert!(p.contains("REF03000") && p.contains("REF04009"));
        assert!(!p.contains("outil Read"));
        let p = prompt_tranche(
            &v,
            &Tranche {
                rang: 1,
                debut: 1,
                fin: 2,
            },
            None,
            None,
            Some("document.pdf"),
        );
        assert!(!p.contains("AVANT") && p.contains("APRÈS"));
        assert!(p.contains("seulement les pages 1-2"));
        let p = prompt_tranche(
            &v,
            &Tranche {
                rang: 3,
                debut: 5,
                fin: 6,
            },
            None,
            None,
            None,
        );
        assert!(p.contains("AVANT") && !p.contains("APRÈS"));
    }

    #[test]
    fn schemas_et_annonces() {
        let s = schema_lignes();
        assert_eq!(s["required"], json!(["lignes"]));
        assert!(s["properties"]["lignes"]["items"]["properties"]["gencod"].is_object());
        let e = schema_entete();
        assert!(
            e["required"]
                .as_array()
                .unwrap()
                .contains(&json!("nombre_lignes_annonce"))
        );
        let r = json!({ "structured_output": { "nombre_lignes_annonce": 412, "quantite_totale_annoncee": null } });
        assert_eq!(annonces(&r).nombre_lignes_annonce, Some(412));
        assert_eq!(annonces(&json!({})), Annonces::default());
        let r = json!({ "structured_output": { "lignes": [{ "reference": "A", "quantite": 2 }] } });
        assert_eq!(lignes_de_reponse(&r).unwrap()[0].quantite, Some(dec!(2.0)));
        assert!(lignes_de_reponse(&json!({ "result": "illisible" })).is_none());
    }

    #[test]
    fn fusion_sans_doublon_ni_en_tete_repete() {
        let entete = Ligne {
            reference: Some("Réf.".into()),
            designation: Some("Désignation".into()),
            ..Default::default()
        };
        let report = Ligne {
            designation: Some("A reporter".into()),
            montant_ht: Some(dec!(120.0)),
            ..Default::default()
        };
        let a = vec![
            entete.clone(),
            ligne("A1", dec!(1.0), dec!(10.0)),
            ligne("A2", dec!(2.0), dec!(20.0)),
            report,
        ];
        // La ligne A2, à cheval, est recopiée par les deux tranches ; la seconde n'en a que la fin.
        let mut morceau = ligne("A2", dec!(2.0), dec!(20.0));
        morceau.reference = None;
        morceau.designation = Some("(suite)".into());
        let b = vec![entete.clone(), morceau, ligne("B1", dec!(1.0), dec!(5.0))];
        let c = vec![
            entete,
            ligne("B1", dec!(3.0), dec!(15.0)),
            ligne("C1", dec!(1.0), dec!(1.0)),
        ];
        let (v, doublons) = fusionner(&[a, b, c]);
        let refs: Vec<_> = v
            .iter()
            .map(|l| l.reference.clone().unwrap_or_default())
            .collect();
        assert_eq!(
            refs,
            ["A1", "A2", "B1", "B1", "C1"],
            "B1 × 3 n'est pas B1 × 1"
        );
        assert_eq!(doublons, 1);
        // Deux tranches qui recopient la même ligne entière.
        let (v, d) = fusionner(&[
            vec![ligne("X", dec!(1.0), dec!(2.0))],
            vec![
                ligne("X", dec!(1.0), dec!(2.0)),
                ligne("Y", dec!(1.0), dec!(2.0)),
            ],
        ]);
        assert_eq!((v.len(), d), (2, 1));
        // Un article à 0 sans nombre n'est pas un en-tête s'il a une vraie désignation.
        let commentaire = Ligne {
            designation: Some("Coussin velours rouge".into()),
            ..Default::default()
        };
        assert!(!parasite(&commentaire));
        assert!(parasite(&Ligne {
            designation: Some("Sous-total page 3".into()),
            montant_ht: Some(dec!(9.0)),
            ..Default::default()
        }));
    }

    #[test]
    fn controle_de_completude() {
        let l = vec![
            ligne("A", dec!(2.0), dec!(10.0)),
            ligne("B", dec!(3.0), dec!(20.5)),
        ];
        let tot = |ht| Totaux {
            ht: Some(ht),
            ..Default::default()
        };
        let c = controler(&l, tot(dec!(30.5)), Annonces::default(), &[]);
        assert!(c.complet && c.verifie == ["somme_ht"], "{c:?}");
        assert_eq!(phrase(3, &c), None);
        // Remise de pied et annexes.
        let c = controler(
            &l,
            Totaux {
                ht: Some(dec!(28.0)),
                annexes_ht: Some(dec!(1.5)),
                remise_ht: Some(dec!(4.0)),
            },
            Annonces::default(),
            &[],
        );
        assert!(c.complet, "{c:?}");
        // Une ligne de moins : la somme ne tombe pas juste.
        let c = controler(&l[..1], tot(dec!(30.5)), Annonces::default(), &[]);
        assert!(!c.complet);
        assert!(
            phrase(3, &c)
                .unwrap()
                .contains("la somme des lignes (10,00 €) ne donne pas le total HT lu (30,50 €)")
        );
        // Nombre de lignes et quantité annoncés.
        let an = Annonces {
            nombre_lignes_annonce: Some(3),
            quantite_totale_annoncee: Some(dec!(5.0)),
        };
        let c = controler(&l, tot(dec!(30.5)), an, &[]);
        assert_eq!(c.verifie, ["somme_ht", "quantite_totale"]);
        assert_eq!(c.raisons, ["le document annonce 3 lignes, 2 lues"]);
        // Pages non lues : jamais complet.
        let c = controler(&l, tot(dec!(30.5)), Annonces::default(), &[9, 10, 11, 12]);
        assert!(!c.complet);
        assert_eq!(
            phrase(7, &c).unwrap(),
            "lecture incomplète (document lu en 7 parties) : pages 9 à 12 non lues"
        );
        // Rien à comparer : pas marqué incomplet, mais signalé.
        let c = controler(&l, Totaux::default(), Annonces::default(), &[]);
        assert!(c.complet && c.verifie.is_empty());
        assert!(phrase(2, &c).unwrap().contains("sans total"));
        assert_eq!(plages(&[3]), "page 3");
        assert_eq!(
            plages(&[9, 10, 11, 12, 15, 17, 18]),
            "pages 9 à 12, 15 et 17 à 18"
        );
        let c = controler(&l, tot(dec!(30.5)), Annonces::default(), &[4]);
        assert_eq!(c.raisons, ["page 4 non lue"]);
    }
}
