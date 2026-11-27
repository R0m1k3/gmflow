//! Montants exacts (constat C12) : HT, TVA, TTC, remises, annexes, éco-participations, prix
//! unitaires, quantités et taux sont des [`Decimal`], lus tels quels depuis les colonnes
//! `numeric` de la base. Les flottants n'existent qu'aux frontières :
//! - entrée (réponse de Claude, Apiflow, Logiflow, cellule de tableur, saisie de l'écran) :
//!   conversion une seule fois, par la forme décimale la plus courte du nombre (`0.1` reste
//!   `0.1`), puis arrondi à la précision de la colonne ([`arrondi`], [`borner`]) ;
//! - sortie JSON : un `Decimal` s'écrit en nombre JSON (fonction `serde-float` de
//!   rust_decimal), jamais en chaîne ;
//! - fichiers Excel : [`vers_f64`] au moment d'écrire la cellule.
//!
//! Ce qui n'est pas de l'argent (confiances, scores, durées, coût d'un appel Claude en USD,
//! métriques) reste en `f64`.

use std::str::FromStr;

pub use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;
use rust_decimal::prelude::ToPrimitive;
pub use rust_decimal_macros::dec;
use serde_json::Value;

/// Décimales des colonnes `numeric` : montants `numeric(14, 2)`.
pub const DEC_MONTANT: u32 = 2;
/// Quantités, colis, PCB : `numeric(14, 3)`.
pub const DEC_QUANTITE: u32 = 3;
/// Prix unitaires, éco-participations par unité, prix de vente : `numeric(14, 4)`.
pub const DEC_PRIX: u32 = 4;
/// Taux de TVA et remise d'un document : `numeric(6, 3)`.
pub const DEC_TAUX: u32 = 3;
/// Remise d'une ligne : `numeric(8, 4)`.
pub const DEC_TAUX_LIGNE: u32 = 4;

/// Arrondi commercial à `decimales` chiffres : la moitié s'arrondit en s'éloignant de zéro
/// (0,125 → 0,13 ; -0,125 → -0,13), comme PostgreSQL pour `numeric`.
pub fn arrondi(v: Decimal, decimales: u32) -> Decimal {
    v.round_dp_with_strategy(decimales, RoundingStrategy::MidpointAwayFromZero)
}

/// Arrondi au centime (la seule règle d'arrondi monétaire du code).
pub fn centimes(v: Decimal) -> Decimal {
    arrondi(v, DEC_MONTANT)
}

/// Valeur arrondie à la précision d'une colonne `numeric(chiffres, decimales)`, ou `None` si
/// elle n'y tient pas (une lecture aberrante ne doit pas faire échouer l'écriture en base ni
/// déborder un calcul).
pub fn borner(v: Decimal, chiffres: u32, decimales: u32) -> Option<Decimal> {
    let a = arrondi(v, decimales);
    let max = Decimal::from_i128_with_scale(10i128.pow(chiffres - decimales), 0);
    (a.abs() < max).then_some(a)
}

/// Bornes des colonnes, pour [`borner`].
pub fn montant(v: Decimal) -> Option<Decimal> {
    borner(v, 14, DEC_MONTANT)
}
pub fn quantite(v: Decimal) -> Option<Decimal> {
    borner(v, 14, DEC_QUANTITE)
}
pub fn prix(v: Decimal) -> Option<Decimal> {
    borner(v, 14, DEC_PRIX)
}
pub fn taux(v: Decimal) -> Option<Decimal> {
    borner(v, 6, DEC_TAUX)
}
pub fn taux_ligne(v: Decimal) -> Option<Decimal> {
    borner(v, 8, DEC_TAUX_LIGNE)
}

/// Flottant reçu de l'extérieur (cellule de tableur, `Value::as_f64`) → décimal, par sa forme
/// décimale la plus courte : 0,1 donne exactement 0,1. `None` pour NaN, l'infini, ou un nombre
/// hors des 28 chiffres d'un `Decimal`.
pub fn depuis_f64(v: f64) -> Option<Decimal> {
    if !v.is_finite() {
        return None;
    }
    Decimal::from_str(&v.to_string()).ok()
}

/// Nombre écrit en texte, point décimal (« 1234.50 », « -3 », « 1e3 »).
pub fn depuis_texte(s: &str) -> Option<Decimal> {
    let s = s.trim();
    Decimal::from_str(s)
        .or_else(|_| Decimal::from_scientific(s))
        .ok()
}

/// Nombre JSON → décimal, sans passer par un flottant quand c'est possible (le texte du
/// nombre est relu tel quel).
pub fn depuis_json(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => depuis_texte(&n.to_string()).or_else(|| depuis_f64(n.as_f64()?)),
        _ => None,
    }
}

/// Décimal → flottant, au dernier moment (cellule Excel).
pub fn vers_f64(v: Decimal) -> f64 {
    v.to_f64().unwrap_or_default()
}

/// Répartit `total` au prorata de `poids`, chaque part arrondie à `decimales` chiffres ; le
/// reste d'arrondi va sur la dernière part, si bien que la somme des parts vaut exactement
/// `total` (arrondi). `None` sans poids ou si leur somme n'est pas positive, ou si un poids
/// est négatif.
pub fn repartir(total: Decimal, poids: &[Decimal], decimales: u32) -> Option<Vec<Decimal>> {
    let somme: Decimal = poids.iter().sum();
    if poids.is_empty() || somme <= Decimal::ZERO || poids.iter().any(|p| p.is_sign_negative()) {
        return None;
    }
    let total = arrondi(total, decimales);
    let mut reste = total;
    let mut parts = Vec::with_capacity(poids.len());
    for (i, p) in poids.iter().enumerate() {
        let part = if i + 1 == poids.len() {
            reste
        } else {
            arrondi(total * *p / somme, decimales)
        };
        reste -= part;
        parts.push(part);
    }
    Some(parts)
}

/// Montant à la française : « 1 250,40 € » (espaces insécables), arrondi au centime.
pub fn montant_fr(v: Decimal) -> String {
    let c = centimes(v);
    let neg = c.is_sign_negative() && !c.is_zero();
    let cts = (c.abs() * Decimal::ONE_HUNDRED)
        .to_u128()
        .unwrap_or_default();
    let entier = (cts / 100).to_string();
    let mut groupes = String::new();
    for (i, ch) in entier.chars().enumerate() {
        if i > 0 && (entier.len() - i).is_multiple_of(3) {
            groupes.push('\u{a0}');
        }
        groupes.push(ch);
    }
    format!(
        "{}{groupes},{:02}\u{a0}€",
        if neg { "-" } else { "" },
        cts % 100
    )
}

/// Nombre arrondi à `decimales` chiffres, écrit avec exactement autant de décimales
/// (« 12.30 »), point décimal.
pub fn fixe(v: Decimal, decimales: u32) -> String {
    let mut a = arrondi(v, decimales);
    a.rescale(decimales);
    if a.is_zero() {
        a.set_sign_positive(true);
    }
    a.to_string()
}

/// Montant en euros pour un message : « 12.30 » (deux décimales, point).
pub fn euros(v: Decimal) -> String {
    fixe(v, DEC_MONTANT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sommes_exactes_ou_un_flottant_se_trompe() {
        // 0,1 + 0,2 : 0.30000000000000004 en f64.
        assert!(std::hint::black_box(0.1_f64) + 0.2 != 0.3);
        assert_eq!(dec!(0.1) + dec!(0.2), dec!(0.3));
        // Lus depuis un nombre JSON ou un flottant, 0,1 reste 0,1.
        let a = depuis_json(&serde_json::json!(0.1)).unwrap();
        let b = depuis_f64(0.2).unwrap();
        assert_eq!(a + b, dec!(0.3));
        // Dix lignes de 0,10 font 1,00.
        let s: Decimal = std::iter::repeat_n(dec!(0.10), 10).sum();
        assert_eq!(s, Decimal::ONE);
        assert!(std::iter::repeat_n(std::hint::black_box(0.1_f64), 10).sum::<f64>() != 1.0);
    }

    #[test]
    fn arrondi_commercial_demi_vers_le_haut() {
        // 1,005 vaut 1.00499999999999989... en f64 : (x × 100).round() / 100 donne 1,00.
        assert_eq!(
            (std::hint::black_box(1.005_f64) * 100.0).round() / 100.0,
            1.0
        );
        assert_eq!(centimes(dec!(1.005)), dec!(1.01));
        assert_eq!(centimes(dec!(2.675)), dec!(2.68));
        assert_eq!(centimes(dec!(-2.675)), dec!(-2.68));
        assert_eq!(centimes(dec!(0.125)), dec!(0.13));
        assert_eq!(centimes(dec!(0.124999)), dec!(0.12));
        // TVA 20 % d'un montant en ,x75 : 5,175 × 0,2 = 1,035 → 1,04 (1,03 en f64).
        assert_eq!(centimes(dec!(5.175) * dec!(0.2)), dec!(1.04));
        assert_eq!(
            ((std::hint::black_box(5.175_f64) * 0.2) * 100.0).round() / 100.0,
            1.03
        );
        // TVA 10 % de 10,35 = 1,035 → 1,04 ; TVA 5,5 % de 23,00 = 1,265 → 1,27.
        assert_eq!(centimes(dec!(10.35) * dec!(0.1)), dec!(1.04));
        assert_eq!(
            ((std::hint::black_box(10.35_f64) * 0.1) * 100.0).round() / 100.0,
            1.03
        );
        assert_eq!(centimes(dec!(23) * dec!(0.055)), dec!(1.27));
        assert_eq!(
            ((std::hint::black_box(23.0_f64) * 0.055) * 100.0).round() / 100.0,
            1.26
        );
        // Avoir : même règle en négatif.
        assert_eq!(centimes(dec!(-5.175) * dec!(0.2)), dec!(-1.04));
    }

    #[test]
    fn bornes_des_colonnes() {
        assert_eq!(montant(dec!(12.345)), Some(dec!(12.35)));
        assert_eq!(montant(dec!(999999999999.99)), Some(dec!(999999999999.99)));
        assert_eq!(montant(dec!(1000000000000)), None);
        assert_eq!(prix(dec!(1.23456)), Some(dec!(1.2346)));
        assert_eq!(taux(dec!(5.5)), Some(dec!(5.5)));
        assert_eq!(taux(dec!(1000)), None);
        assert_eq!(depuis_f64(f64::NAN), None);
        assert_eq!(depuis_f64(1e30), None);
        assert_eq!(depuis_texte("1e3"), Some(dec!(1000)));
    }

    #[test]
    fn prorata_qui_retombe_sur_le_total() {
        // 100 € sur trois parts égales : 33,33 + 33,33 + 33,34.
        let p = repartir(dec!(100), &[Decimal::ONE; 3], 2).unwrap();
        assert_eq!(p, [dec!(33.33), dec!(33.33), dec!(33.34)]);
        assert_eq!(p.iter().sum::<Decimal>(), dec!(100));
        // Avoir (négatif) : le reste aussi sur la dernière part.
        let p = repartir(dec!(-100), &[dec!(1), dec!(1), dec!(1)], 2).unwrap();
        assert_eq!(p, [dec!(-33.33), dec!(-33.33), dec!(-33.34)]);
        // Poids inégaux.
        let p = repartir(
            dec!(1161.29),
            &[dec!(700.40), dec!(350.20), dec!(350.20)],
            2,
        )
        .unwrap();
        assert_eq!(p.iter().sum::<Decimal>(), dec!(1161.29));
        assert_eq!(p, [dec!(580.65), dec!(290.32), dec!(290.32)]);
        assert_eq!(repartir(dec!(1), &[], 2), None);
        assert_eq!(repartir(dec!(1), &[dec!(0), dec!(0)], 2), None);
        assert_eq!(repartir(dec!(1), &[dec!(2), dec!(-1)], 2), None);
    }

    #[test]
    fn montant_a_la_francaise() {
        assert_eq!(montant_fr(dec!(1250.4)), "1\u{a0}250,40\u{a0}€");
        assert_eq!(montant_fr(dec!(-0.005)), "-0,01\u{a0}€");
        assert_eq!(montant_fr(dec!(-0.004)), "0,00\u{a0}€");
        assert_eq!(
            montant_fr(dec!(1234567.891)),
            "1\u{a0}234\u{a0}567,89\u{a0}€"
        );
        assert_eq!(euros(dec!(12.3)), "12.30");
        assert_eq!(euros(dec!(-1.005)), "-1.01");
        assert_eq!(euros(dec!(-0.001)), "0.00");
        assert_eq!(fixe(dec!(25), 1), "25.0");
    }

    #[test]
    fn json_en_nombre() {
        // Un montant part en nombre JSON, jamais en chaîne.
        let v = serde_json::to_value(dec!(12.30)).unwrap();
        assert_eq!(v, serde_json::json!(12.3));
        assert!(v.is_number());
        let o: Option<Decimal> = serde_json::from_value(serde_json::json!(0.1)).unwrap();
        assert_eq!(o, Some(dec!(0.1)));
        let o: Option<Decimal> = serde_json::from_value(serde_json::json!(null)).unwrap();
        assert_eq!(o, None);
    }
}
