//! Tri des listes paginées (`?tri=montant&sens=asc`). La colonne demandée est validée contre la
//! liste blanche de chaque liste : seule une clé connue (`&'static str`) part dans la requête,
//! comme paramètre lié comparé dans des `case when $n = 'cle'` écrits en dur. Rien de ce que le
//! client envoie n'est collé dans le SQL.

use crate::erreur::{ErreurApi, Resultat};

/// Tri choisi : une clé de la liste blanche et son sens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tri {
    pub cle: &'static str,
    pub croissant: bool,
}

/// Une colonne triable et son sens quand on clique dessus la première fois (les dates et les
/// montants : du plus récent ou du plus grand ; les noms : de A à Z).
pub type Colonne = (&'static str, bool);

/// `tri` absent ou vide : la première colonne de `permis` (le tri par défaut de la liste).
/// `sens` absent : le sens par défaut de la colonne ; sinon `asc` ou `desc`. Toute autre valeur
/// est refusée (400, champ `tri` ou `sens`).
pub fn lire(tri: &Option<String>, sens: &Option<String>, permis: &[Colonne]) -> Resultat<Tri> {
    let demande = tri.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (cle, defaut) = match demande {
        None => permis[0],
        Some(t) => *permis
            .iter()
            .find(|(c, _)| *c == t)
            .ok_or_else(|| ErreurApi::Champ {
                champ: "tri",
                message: format!("tri inconnu : {t}"),
            })?,
    };
    let croissant = match sens.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => defaut,
        Some("asc") => true,
        Some("desc") => false,
        Some(s) => {
            return Err(ErreurApi::Champ {
                champ: "sens",
                message: format!("sens inconnu : {s} (asc ou desc)"),
            });
        }
    };
    Ok(Tri { cle, croissant })
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &[Colonne] = &[("recu", false), ("fournisseur", true), ("montant", false)];

    fn t(tri: Option<&str>, sens: Option<&str>) -> Resultat<Tri> {
        lire(&tri.map(String::from), &sens.map(String::from), P)
    }

    #[test]
    fn defaut_puis_sens_de_la_colonne() {
        assert_eq!(
            t(None, None).unwrap(),
            Tri {
                cle: "recu",
                croissant: false
            }
        );
        assert_eq!(
            t(Some(""), None).unwrap(),
            Tri {
                cle: "recu",
                croissant: false
            }
        );
        assert!(t(Some("fournisseur"), None).unwrap().croissant);
        assert!(!t(Some("fournisseur"), Some("desc")).unwrap().croissant);
        assert!(t(Some("montant"), Some("asc")).unwrap().croissant);
    }

    #[test]
    fn hors_liste_blanche_refuse() {
        for (tri, sens, champ) in [
            (Some("d.id; drop table document"), None, "tri"),
            (Some("MONTANT"), None, "tri"),
            (Some("montant"), Some("up"), "sens"),
        ] {
            match t(tri, sens) {
                Err(ErreurApi::Champ { champ: c, .. }) => assert_eq!(c, champ),
                autre => panic!("{autre:?}"),
            }
        }
    }
}
