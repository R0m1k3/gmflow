//! Appel de lecture à Claude : prompt, schéma JSON de la réponse et contrôle de la réponse.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::societe::SocieteRef;

pub const TYPES: &[&str] = &["facture", "avoir", "bl", "relance", "devis", "autre"];

/// Longueur maximale du texte d'un document envoyé à Claude.
pub const TEXTE_MAX: usize = 20_000;

/// Lignes articles gardées par document (les suivantes sont ignorées).
pub const LIGNES_MAX: usize = 200;

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
    pub montant_ht: Option<f64>,
    pub montant_tva: Option<f64>,
    pub montant_ttc: Option<f64>,
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
        "montant_ht": nombre_ou_null(),
        "montant_tva": nombre_ou_null(),
        "montant_ttc": nombre_ou_null(),
        "lignes": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "reference": chaine_ou_null(), "designation": chaine_ou_null(),
                    "quantite": nombre_ou_null(), "unite": chaine_ou_null(),
                    "prix_unitaire_ht": nombre_ou_null(), "montant_ht": nombre_ou_null()
                },
                "required": ["reference", "designation", "quantite", "unite", "prix_unitaire_ht", "montant_ht"],
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
        "montant_ht",
        "montant_tva",
        "montant_ttc",
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
         facture (liste vide sinon).\n\n"
    ));
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
    e.lignes = lignes_utiles(std::mem::take(&mut e.lignes));
    e.numeros_bl = numeros_utiles(std::mem::take(&mut e.numeros_bl));
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
    if let Some(d) = &e.date
        && date(d).is_none()
    {
        return err(format!("date « {d} » hors format AAAA-MM-JJ"));
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

/// Chiffres seuls d'un montant au centime (1535.4 -> « 153540 »).
fn chiffres_montant(m: f64) -> String {
    format!("{:.2}", m.abs()).replace('.', "")
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
            if e.date.is_none() {
                a.push("la date du document manque".to_string());
            }
            match (e.montant_ht, e.montant_tva, e.montant_ttc) {
                (_, _, None) => a.push("le montant TTC manque".to_string()),
                (Some(ht), Some(tva), Some(ttc)) if (ht + tva - ttc).abs() > 0.05 => a.push(
                    format!("HT + TVA ({ht:.2} + {tva:.2}) ne donne pas le TTC ({ttc:.2})"),
                ),
                _ => {}
            }
            if let (Some(ttc), Some(t)) = (e.montant_ttc, texte) {
                // Chiffres seuls des deux côtés : 1 535,40 / 1.535,40 / 1535.40 se retrouvent.
                let chiffres: String = t.chars().filter(char::is_ascii_digit).collect();
                // Tableurs : 1535.4 sans le zéro final.
                let court: String = ttc
                    .abs()
                    .to_string()
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect();
                if !chiffres.contains(&chiffres_montant(ttc)) && !chiffres.contains(&court) {
                    a.push(format!(
                        "le montant TTC {ttc:.2} n'apparaît pas dans le texte du document"
                    ));
                }
            }
        }
        "bl" if e.lignes.is_empty() => {
            a.push("aucune ligne article lue sur le bon de livraison".to_string());
        }
        _ => {}
    }
    a
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
