//! Contrôles d'une facture avant dépôt (cahier, « Fiabilité et contrôles »). Fonctions pures :
//! le doublon, qui demande la base, est contrôlé dans `depot`.

use serde::{Deserialize, Serialize};

/// Écart toléré entre HT + TVA et TTC.
pub const TOLERANCE_MONTANTS: f64 = 0.02;
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
            if ecart > TOLERANCE_MONTANTS + 1e-9 {
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
    }
    e
}

/// Date du jour à Paris (les dates de documents sont des dates locales).
pub fn aujourdhui_paris() -> time::Date {
    // Décalage d'été ou d'hiver : +1 h ou +2 h. Une heure d'écart autour de minuit ne change
    // pas le résultat des contrôles (12 mois, futur).
    (time::OffsetDateTime::now_utc() + time::Duration::hours(1)).date()
}
