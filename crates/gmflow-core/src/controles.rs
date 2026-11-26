//! Contrôles d'une facture avant dépôt (cahier, « Fiabilité et contrôles »). Fonctions pures :
//! le doublon, qui demande la base, est contrôlé dans `depot`.

use serde::{Deserialize, Serialize};

/// Écart de montant toléré partout (arrondis) : HT + TVA = TTC au dépôt et à la lecture,
/// somme des lignes = HT, facture = BL au rapprochement.
pub const SEUIL_ECART: f64 = 0.05;
/// Taux de TVA le plus haut en France (%).
pub const TAUX_TVA_MAX: f64 = 20.0;
/// Confiance minimale sur la société destinataire.
pub const CONFIANCE_SOCIETE_MIN: f64 = 0.9;
/// Âge maximal d'une facture, en jours (12 mois).
pub const AGE_MAX_JOURS: i64 = 366;

/// Contrôle en échec. `message` : phrase pour l'écran, dans le vocabulaire de l'interface
/// (société, envoi à Dext/Yooz, montants « 1 250,40 € »), sans point final.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Echec {
    pub code: String,
    pub message: String,
    /// Champs du document visés (noms de l'API : `montant_ttc`, `societe_id`...), pour les
    /// surligner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub champs: Vec<String>,
    /// Correction proposée en un clic (montants : HT + TVA = TTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposition: Option<Proposition>,
}

/// Valeur proposée pour un champ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposition {
    pub champ: String,
    pub valeur: f64,
}

fn echec(code: &str, message: impl Into<String>, champs: &[&str]) -> Echec {
    Echec {
        code: code.into(),
        message: message.into(),
        champs: champs.iter().map(|c| c.to_string()).collect(),
        proposition: None,
    }
}

fn arrondi(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Montant à la française : « 1 250,40 € » (espaces insécables).
pub fn montant_fr(v: f64) -> String {
    let neg = v < 0.0;
    let centimes = (v.abs() * 100.0).round() as u64;
    let entier = (centimes / 100).to_string();
    let mut groupes = String::new();
    for (i, c) in entier.chars().enumerate() {
        if i > 0 && (entier.len() - i).is_multiple_of(3) {
            groupes.push('\u{a0}');
        }
        groupes.push(c);
    }
    format!(
        "{}{groupes},{:02}\u{a0}€",
        if neg { "-" } else { "" },
        centimes % 100
    )
}

/// Date à la française : « 18/11/2026 ».
pub fn date_fr(d: time::Date) -> String {
    format!("{:02}/{:02}/{}", d.day(), u8::from(d.month()), d.year())
}

pub fn chiffres(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// TVA en majuscules sans espaces ni ponctuation.
pub fn tva_compacte(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn luhn(c: &str) -> bool {
    let mut somme = 0u32;
    for (i, ch) in c.chars().rev().enumerate() {
        let Some(mut d) = ch.to_digit(10) else {
            return false;
        };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        somme += d;
    }
    somme.is_multiple_of(10)
}

pub fn siren_valide(s: &str) -> bool {
    let c = s.replace([' ', '.'], "");
    c.len() == 9 && c.chars().all(|ch| ch.is_ascii_digit()) && luhn(&c)
}

/// SIRET : 14 chiffres, clé de Luhn. Exception connue : les établissements de La Poste
/// (SIREN 356000000), dont la somme des chiffres est un multiple de 5.
pub fn siret_valide(s: &str) -> bool {
    let brut: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .collect();
    let c = chiffres(&brut);
    if c.len() != 14 || c != brut {
        return false;
    }
    if c.starts_with("356000000") {
        return c
            .chars()
            .filter_map(|d| d.to_digit(10))
            .sum::<u32>()
            .is_multiple_of(5);
    }
    luhn(&c)
}

/// Clé de TVA française d'un SIREN : (12 + 3 × (SIREN mod 97)) mod 97.
pub fn cle_tva_fr(siren: &str) -> Option<u64> {
    let n: u64 = siren.parse().ok()?;
    Some((12 + 3 * (n % 97)) % 97)
}

/// N° de TVA intracommunautaire au bon format. France : FR + clé (2 chiffres, vérifiée, ou
/// 2 caractères pour les anciens numéros) + SIREN. Autres pays : 2 lettres + 2 à 13 caractères.
pub fn tva_valide(s: &str) -> bool {
    let t = tva_compacte(s);
    if t.len() < 4 || !t[..2].chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    if let Some(reste) = t.strip_prefix("FR") {
        if reste.len() != 11 {
            return false;
        }
        let (cle, siren) = reste.split_at(2);
        if !siren.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        return match cle.parse::<u64>() {
            Ok(k) => cle_tva_fr(siren) == Some(k),
            // Clé alphanumérique (numéros anciens) : format seul.
            Err(_) => true,
        };
    }
    (2..=13).contains(&(t.len() - 2))
}

/// Ce qui est contrôlé sur une facture ou un avoir lu.
#[derive(Debug, Clone, Default)]
pub struct Facture<'a> {
    pub societe_id: Option<i64>,
    pub confiance_societe: f64,
    /// Statut de la fiche fournisseur (`valide`, `a_confirmer`), si le fournisseur est rattaché.
    pub fournisseur_statut: Option<&'a str>,
    pub fournisseur_siret: Option<&'a str>,
    pub fournisseur_tva: Option<&'a str>,
    pub numero: Option<&'a str>,
    pub date: Option<time::Date>,
    pub ht: Option<f64>,
    pub tva: Option<f64>,
    pub ttc: Option<f64>,
    /// La structure a une adresse de dépôt.
    pub adresse_depot: bool,
    /// Échéance du paiement lue.
    pub echeance: Option<time::Date>,
    /// SIRET (ou SIREN) et n° de TVA lus sur le document, sans ceux de la fiche.
    pub siret_lu: Option<&'a str>,
    pub tva_lue: Option<&'a str>,
    /// SIRET et n° de TVA de la fiche fournisseur et de son groupe.
    pub sirets_fiche: Vec<&'a str>,
    pub tvas_fiche: Vec<&'a str>,
    /// Somme des montants HT des lignes, quand toutes en ont un et qu'aucune ne manque (facture
    /// lue en entier) ; `eco_lignes` : éco-participations par unité × quantité des lignes.
    pub somme_lignes: Option<f64>,
    pub eco_lignes: f64,
    /// Remise et montants annexes lus à part (`document.remise_ht`, `remise_taux`, `annexes_ht`).
    pub remise_ht: Option<f64>,
    pub remise_taux: Option<f64>,
    pub annexes_ht: Option<f64>,
    /// Devise des montants quand le document la donne (facture électronique).
    pub devise: Option<&'a str>,
}

/// La somme des lignes redonne-t-elle le HT, d'une des façons dont une facture le compose :
/// remise de pied déduite ou non, montants annexes (port, éco-participation) et
/// éco-participations des lignes ajoutés ou non (règle de lecture : ils vont dans le HT) ?
pub fn lignes_font_le_ht(
    somme: f64,
    eco_lignes: f64,
    ht: f64,
    remise_ht: Option<f64>,
    remise_taux: Option<f64>,
    annexes_ht: Option<f64>,
) -> bool {
    let remise = remise_ht
        .map(f64::abs)
        .or_else(|| {
            remise_taux
                .filter(|t| *t > 0.0 && *t < 100.0)
                .map(|t| somme * t / 100.0)
        })
        .unwrap_or(0.0);
    let annexes = annexes_ht.map(f64::abs).unwrap_or(0.0);
    let tolerance = SEUIL_ECART.max(ht.abs() * 0.001) + 1e-9;
    [0.0, remise].iter().any(|r| {
        [0.0, annexes].iter().any(|a| {
            [0.0, eco_lignes]
                .iter()
                .any(|e| (somme - r + a + e - ht).abs() <= tolerance)
        })
    })
}

/// SIREN d'un SIRET ou d'un SIREN valide.
fn siren_de(s: &str) -> Option<String> {
    let c = chiffres(s);
    (siret_valide(s) || siren_valide(s)).then(|| c[..9].to_string())
}

/// Pourcentage à la française : « 25,3 % ».
fn pourcentage_fr(v: f64) -> String {
    format!("{v:.1}\u{a0}%").replace('.', ",")
}

/// Contrôles en échec (vide : la facture peut partir). `aujourdhui` : date du jour (Paris).
pub fn controler(f: &Facture<'_>, aujourdhui: time::Date) -> Vec<Echec> {
    let mut e = Vec::new();
    let manquants: Vec<(&str, &str)> = [
        (
            "numéro",
            "numero",
            f.numero.is_some_and(|n| !n.trim().is_empty()),
        ),
        ("date", "date_document", f.date.is_some()),
        ("montant TTC", "montant_ttc", f.ttc.is_some()),
    ]
    .into_iter()
    .filter(|(_, _, ok)| !ok)
    .map(|(n, c, _)| (n, c))
    .collect();
    if !manquants.is_empty() {
        let noms: Vec<&str> = manquants.iter().map(|(n, _)| *n).collect();
        let champs: Vec<&str> = manquants.iter().map(|(_, c)| *c).collect();
        e.push(echec(
            "incomplet",
            format!(
                "{} : {}",
                if noms.len() > 1 {
                    "Champs manquants"
                } else {
                    "Champ manquant"
                },
                noms.join(", ")
            ),
            &champs,
        ));
    }
    match (f.ht, f.tva, f.ttc) {
        (Some(ht), Some(tva), Some(ttc)) => {
            let ecart = (ht + tva - ttc).abs();
            if ecart > SEUIL_ECART + 1e-9 {
                let mut x = echec(
                    "montants",
                    format!(
                        "HT + TVA donnent {}, le TTC lu est {}{}",
                        montant_fr(ht + tva),
                        montant_fr(ttc),
                        // Éco-participation ou port imprimés après le Total HT, oubliés.
                        if ht + tva < ttc {
                            " (éco-participation ou port à ajouter au HT et à la TVA ?)"
                        } else {
                            ""
                        }
                    ),
                    &["montant_ttc"],
                );
                // TTC plus grand : il est le plus sûr (retrouvé dans le texte), pas de baisse
                // proposée ; c'est le HT et la TVA qui ont oublié un montant.
                if ht + tva > ttc {
                    x.proposition = Some(Proposition {
                        champ: "montant_ttc".into(),
                        valeur: arrondi(ht + tva),
                    });
                }
                e.push(x);
            }
        }
        (None, Some(tva), Some(ttc)) => {
            let mut x = echec("montants", "Montant HT manquant", &["montant_ht"]);
            x.proposition = Some(Proposition {
                champ: "montant_ht".into(),
                valeur: arrondi(ttc - tva),
            });
            e.push(x);
        }
        (Some(ht), None, Some(ttc)) => {
            let mut x = echec("montants", "Montant de TVA manquant", &["montant_tva"]);
            x.proposition = Some(Proposition {
                champ: "montant_tva".into(),
                valeur: arrondi(ttc - ht),
            });
            e.push(x);
        }
        (None, None, Some(_)) => e.push(echec(
            "montants",
            "Montants HT et TVA manquants",
            &["montant_ht", "montant_tva"],
        )),
        _ => {}
    }
    match (f.fournisseur_siret, f.fournisseur_tva) {
        (None, None) => e.push(echec(
            "identifiants",
            "Ni SIRET ni n° de TVA du fournisseur",
            &["fournisseur_id"],
        )),
        (s, t) => {
            // SIREN seul (9 chiffres) : demandé à la lecture quand le SIRET n'est pas imprimé.
            if let Some(s) = s
                && !siret_valide(s)
                && !siren_valide(s)
            {
                e.push(echec(
                    "identifiants",
                    format!("SIRET du fournisseur « {s} » invalide"),
                    &["fournisseur_id"],
                ));
            }
            if let Some(t) = t
                && !tva_valide(t)
            {
                e.push(echec(
                    "identifiants",
                    format!("N° de TVA du fournisseur « {t} » invalide"),
                    &["fournisseur_id"],
                ));
            }
        }
    }
    // Identifiants lus ≠ fiche fournisseur (fiche mal rattachée, ou RIB et coordonnées d'un
    // autre) : comparés seulement quand les deux côtés sont valides. SIREN : un autre
    // établissement de la même société n'est pas signalé.
    if let Some(lu) = f.siret_lu.and_then(siren_de) {
        let fiche: Vec<String> = f.sirets_fiche.iter().filter_map(|s| siren_de(s)).collect();
        if !fiche.is_empty() && !fiche.contains(&lu) {
            e.push(echec(
                "identite",
                format!(
                    "SIRET lu « {} » différent de celui de la fiche fournisseur « {} »",
                    f.siret_lu.unwrap_or_default().trim(),
                    f.sirets_fiche
                        .iter()
                        .find(|s| siren_de(s).is_some())
                        .copied()
                        .unwrap_or_default()
                ),
                &["fournisseur_id"],
            ));
        }
    }
    if let Some(lu) = f.tva_lue.filter(|t| tva_valide(t)) {
        let fiche: Vec<&str> = f
            .tvas_fiche
            .iter()
            .copied()
            .filter(|t| tva_valide(t))
            .collect();
        if !fiche.is_empty() && !fiche.iter().any(|t| tva_compacte(t) == tva_compacte(lu)) {
            e.push(echec(
                "identite",
                format!(
                    "N° de TVA lu « {} » différent de celui de la fiche fournisseur « {} »",
                    lu.trim(),
                    fiche[0]
                ),
                &["fournisseur_id"],
            ));
        }
    }
    if let (Some(s), Some(ht)) = (f.somme_lignes, f.ht)
        && !lignes_font_le_ht(
            s,
            f.eco_lignes,
            ht,
            f.remise_ht,
            f.remise_taux,
            f.annexes_ht,
        )
    {
        e.push(echec(
            "lignes",
            format!(
                "La somme des lignes ({}) ne fait pas le HT ({}) : lignes ou HT mal lus",
                montant_fr(s),
                montant_fr(ht)
            ),
            &["montant_ht"],
        ));
    }
    if let (Some(ht), Some(tva)) = (f.ht, f.tva)
        && ht > 0.0
        && tva > ht * TAUX_TVA_MAX / 100.0 + SEUIL_ECART.max(ht * 0.002)
    {
        e.push(echec(
            "tva",
            format!(
                "TVA de {} du HT : plus que le taux normal de 20\u{a0}%",
                pourcentage_fr(tva / ht * 100.0)
            ),
            &["montant_tva"],
        ));
    }
    if let Some(d) = f
        .devise
        .map(str::trim)
        .filter(|d| !d.is_empty() && !d.eq_ignore_ascii_case("EUR"))
    {
        e.push(echec(
            "devise",
            format!("Montants en {d}, pas en euros"),
            &["montant_ttc"],
        ));
    }
    match f.societe_id {
        None => e.push(echec(
            "societe",
            "Société destinataire non reconnue",
            &["societe_id"],
        )),
        Some(_) if f.confiance_societe < CONFIANCE_SOCIETE_MIN => e.push(echec(
            "societe",
            format!(
                "Société à vérifier : reconnue avec une confiance de {:.0}\u{a0}%",
                f.confiance_societe * 100.0
            ),
            &["societe_id"],
        )),
        Some(_) if !f.adresse_depot => e.push(echec(
            "depot",
            "La société n'a pas d'adresse d'envoi vers Dext ou Yooz",
            &["societe_id"],
        )),
        _ => {}
    }
    match f.fournisseur_statut {
        Some("valide") => {}
        Some(_) => e.push(echec(
            "fournisseur",
            "Nouveau fournisseur (à confirmer)",
            &["fournisseur_id"],
        )),
        None => e.push(echec(
            "fournisseur",
            "Fournisseur non identifié",
            &["fournisseur_id"],
        )),
    }
    if let Some(d) = f.date {
        let jours = (aujourdhui - d).whole_days();
        if jours < 0 {
            e.push(echec(
                "date",
                format!("Date du {} dans le futur", date_fr(d)),
                &["date_document"],
            ));
        } else if jours > AGE_MAX_JOURS {
            e.push(echec(
                "date",
                format!("Date du {} : plus de 12 mois", date_fr(d)),
                &["date_document"],
            ));
        }
        if let Some(ech) = f.echeance
            && ech < d
        {
            e.push(echec(
                "echeance",
                format!(
                    "Échéance du {} avant la date du document ({})",
                    date_fr(ech),
                    date_fr(d)
                ),
                &[],
            ));
        }
    }
    e
}

/// Date du jour à Paris (les dates de documents sont des dates locales).
pub fn aujourdhui_paris() -> time::Date {
    // Décalage d'été ou d'hiver : +1 h ou +2 h. Une heure d'écart autour de minuit ne change
    // pas le résultat des contrôles (12 mois, futur).
    (time::OffsetDateTime::now_utc() + time::Duration::hours(1)).date()
}
