//! Société destinataire reconnue par le code : SIRET, SIREN ou n° de TVA de la structure
//! présents dans le texte, ou identifiants de l'acheteur d'une facture électronique.

use serde::Serialize;

use super::xml::Partie;

#[derive(Debug, Clone, Serialize)]
pub struct SocieteRef {
    pub id: i64,
    pub raison_sociale: String,
    pub siret: Option<String>,
    pub tva: Option<String>,
    /// Adresses en texte libre (pour le prompt).
    pub adresses: Vec<String>,
}

fn chiffres(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn nom_normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Identifiants cherchés pour une société : SIRET (14), SIREN (9), TVA (FRxx + SIREN).
fn identifiants(s: &SocieteRef) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(siret) = &s.siret {
        let c = chiffres(siret);
        if c.len() == 14 {
            ids.push(c[..9].to_string());
            ids.push(c);
        } else if c.len() == 9 {
            ids.push(c);
        }
    }
    if let Some(tva) = &s.tva {
        let c = compact(tva);
        if c.len() >= 8 {
            ids.push(c);
        }
    }
    ids
}

/// La société dont un identifiant apparaît dans le texte, s'il y en a une et une seule.
pub fn dans_texte(texte: &str, societes: &[SocieteRef]) -> Option<i64> {
    let t = compact(texte);
    let trouvees: Vec<i64> = societes
        .iter()
        .filter(|s| identifiants(s).iter().any(|id| contient_isole(&t, id)))
        .map(|s| s.id)
        .collect();
    match trouvees.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

/// `id` présent dans `t` sans être collé à d'autres chiffres (un SIREN n'est pas trouvé au
/// milieu d'un numéro de facture plus long).
fn contient_isole(t: &str, id: &str) -> bool {
    let octets = t.as_bytes();
    let mut depart = 0;
    while let Some(pos) = t[depart..].find(id) {
        let debut = depart + pos;
        let fin = debut + id.len();
        let avant = debut == 0 || !octets[debut - 1].is_ascii_digit();
        // Un SIREN suivi de 5 chiffres est un SIRET : il compte aussi.
        let apres = fin == octets.len()
            || !octets[fin].is_ascii_digit()
            || (id.len() == 9
                && octets.len() >= fin + 5
                && octets[fin..fin + 5].iter().all(u8::is_ascii_digit)
                && (fin + 5 == octets.len() || !octets[fin + 5].is_ascii_digit()));
        if avant && apres {
            return true;
        }
        depart = debut + 1;
    }
    false
}

/// La société qui correspond à l'acheteur d'une facture électronique.
pub fn pour_acheteur(acheteur: &Partie, societes: &[SocieteRef]) -> Option<i64> {
    let mut ids_acheteur = Vec::new();
    if let Some(s) = &acheteur.siret {
        let c = chiffres(s);
        if c.len() >= 9 {
            ids_acheteur.push(c[..9].to_string());
        }
    }
    if let Some(t) = &acheteur.tva {
        ids_acheteur.push(compact(t));
    }
    let par_id: Vec<i64> = societes
        .iter()
        .filter(|s| {
            identifiants(s).iter().any(|id| {
                ids_acheteur.iter().any(|a| {
                    a == id || (id.len() == 14 && a.len() == 9 && id.starts_with(a.as_str()))
                })
            })
        })
        .map(|s| s.id)
        .collect();
    if let [id] = par_id.as_slice() {
        return Some(*id);
    }
    let nom = nom_normalise(acheteur.nom.as_deref()?);
    let par_nom: Vec<i64> = societes
        .iter()
        .filter(|s| !nom.is_empty() && nom_normalise(&s.raison_sociale) == nom)
        .map(|s| s.id)
        .collect();
    match par_nom.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}
