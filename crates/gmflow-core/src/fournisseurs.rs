//! Référentiel des fournisseurs, construit au fil de l'eau. Identification dans l'ordre du
//! cahier : SIRET ou n° de TVA, puis domaine de l'expéditeur, puis nom rapproché des fiches.

use std::collections::BTreeSet;

use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::controles::{chiffres, tva_compacte};

/// Domaines de messageries grand public : ils ne désignent pas un fournisseur.
const DOMAINES_GENERIQUES: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "hotmail.com",
    "hotmail.fr",
    "outlook.com",
    "outlook.fr",
    "live.com",
    "live.fr",
    "msn.com",
    "yahoo.com",
    "yahoo.fr",
    "orange.fr",
    "wanadoo.fr",
    "free.fr",
    "sfr.fr",
    "neuf.fr",
    "laposte.net",
    "icloud.com",
    "me.com",
    "gmx.fr",
    "gmx.com",
    "gmx.net",
    "aol.com",
    "bbox.fr",
    "numericable.fr",
    "proton.me",
    "protonmail.com",
];

/// Mots sans valeur pour rapprocher deux noms (formes juridiques, liaisons).
const MOTS_VIDES: &[&str] = &[
    "sa", "sas", "sasu", "sarl", "eurl", "snc", "sci", "scop", "sca", "ste", "societe", "et",
    "cie", "co", "de", "du", "des", "la", "le", "les", "l", "d", "france", "group", "groupe",
];

/// Score de nom au-delà duquel une fiche est rattachée sans Claude.
pub const SCORE_SUR: f64 = 0.8;
/// Score à partir duquel une fiche est une candidate (Claude tranche).
pub const SCORE_CANDIDAT: f64 = 0.5;

fn sans_accent(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' | 'í' => 'i',
        'ô' | 'ö' | 'ó' | 'õ' => 'o',
        'ù' | 'û' | 'ü' | 'ú' => 'u',
        'ç' => 'c',
        'ÿ' => 'y',
        'ñ' => 'n',
        _ => c,
    }
}

/// Nom en minuscules, sans accents, ponctuation ni formes juridiques : « Grossiste Durand &
/// Fils SAS » -> « grossiste durand fils ».
pub fn normaliser(nom: &str) -> String {
    let t: String = nom
        .to_lowercase()
        .chars()
        .map(sans_accent)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    t.split_whitespace()
        .filter(|m| !MOTS_VIDES.contains(m))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ressemblance de deux noms normalisés, de 0 à 1 : le plus grand de l'indice de Jaccard et
/// de 0,7 × l'inclusion (« metro » est inclus dans « metro cash carry » : candidate, pas sûre).
pub fn ressemblance(a: &str, b: &str) -> f64 {
    let ma: BTreeSet<&str> = a.split_whitespace().collect();
    let mb: BTreeSet<&str> = b.split_whitespace().collect();
    if ma.is_empty() || mb.is_empty() {
        return 0.0;
    }
    let commun = ma.intersection(&mb).count() as f64;
    let jaccard = commun / ma.union(&mb).count() as f64;
    let inclusion = commun / ma.len().min(mb.len()) as f64;
    jaccard.max(0.7 * inclusion)
}

/// Domaine d'une adresse d'expédition (« Durand <factures@Durand.fr> » -> « durand.fr »).
pub fn domaine(expediteur: &str) -> Option<String> {
    let adresse = match (expediteur.rfind('<'), expediteur.rfind('>')) {
        (Some(a), Some(b)) if a < b => &expediteur[a + 1..b],
        _ => expediteur,
    };
    let (_, d) = adresse.trim().rsplit_once('@')?;
    let d = d.trim().trim_end_matches('.').to_lowercase();
    (d.contains('.') && !d.contains(' ')).then_some(d)
}

pub fn domaine_generique(d: &str) -> bool {
    DOMAINES_GENERIQUES.contains(&d)
}

/// Ce qu'on sait du fournisseur d'un document.
#[derive(Debug, Clone, Default)]
pub struct Indices {
    pub nom: Option<String>,
    pub siret: Option<String>,
    pub tva: Option<String>,
    /// Domaine utilisable de l'expéditeur (ni grand public, ni celui d'une boîte surveillée).
    pub domaine: Option<String>,
}

impl Indices {
    fn siret_net(&self) -> Option<String> {
        self.siret
            .as_deref()
            .map(chiffres)
            .filter(|c| c.len() == 14 || c.len() == 9)
    }

    fn tva_nette(&self) -> Option<String> {
        self.tva
            .as_deref()
            .map(tva_compacte)
            .filter(|t| t.len() >= 4)
    }

    /// SIREN connus (du SIRET, ou d'un n° de TVA français).
    fn sirens(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(s) = self.siret_net() {
            v.push(s[..9].to_string());
        }
        if let Some(t) = self.tva_nette()
            && t.starts_with("FR")
            && t.len() == 13
        {
            v.push(t[4..].to_string());
        }
        v
    }

    fn nom_normalise(&self) -> Option<String> {
        self.nom
            .as_deref()
            .map(normaliser)
            .filter(|n| !n.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct Candidat {
    pub id: i64,
    pub nom: String,
    pub siret: Option<String>,
    pub tva: Option<String>,
    pub domaines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Identification {
    /// Fiche trouvée sans ambiguïté, et par quel moyen (`identifiant`, `domaine`, `nom`).
    Trouve {
        id: i64,
        par: &'static str,
    },
    /// Plusieurs fiches possibles : à départager (Claude, ou un humain).
    Candidats(Vec<Candidat>),
    Aucun,
}

/// Domaines des boîtes surveillées : un mail transféré par un magasin ne désigne pas le
/// fournisseur.
pub async fn domaines_exclus(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        r#"select distinct lower(split_part(identifiant, '@', 2)) as "d!" from boite_mail
           where identifiant like '%@%'"#
    )
    .fetch_all(pool)
    .await
}

/// Domaine utilisable d'un expéditeur, ou rien.
pub fn domaine_utile(expediteur: &str, exclus: &[String]) -> Option<String> {
    domaine(expediteur).filter(|d| !domaine_generique(d) && !exclus.iter().any(|e| e == d))
}

pub async fn identifier(pool: &PgPool, ind: &Indices) -> sqlx::Result<Identification> {
    let sirens = ind.sirens();
    let tvas: Vec<String> = ind.tva_nette().into_iter().collect();
    if !sirens.is_empty() || !tvas.is_empty() {
        let ids = sqlx::query_scalar!(
            "select id from fournisseur where fusionne_dans is null
               and (left(siret, 9) = any($1) or tva = any($2)
                    or (tva like 'FR%' and length(tva) = 13 and substr(tva, 5, 9) = any($1)))
             order by id",
            &sirens,
            &tvas
        )
        .fetch_all(pool)
        .await?;
        if let Some(id) = ids.first() {
            // Deux fiches au même SIREN : la plus ancienne (l'autre est un doublon à fusionner).
            return Ok(Identification::Trouve {
                id: *id,
                par: "identifiant",
            });
        }
    }
    if let Some(d) = &ind.domaine {
        let ids = sqlx::query_scalar!(
            "select id from fournisseur where fusionne_dans is null and $1 = any(domaines) order by id",
            d
        )
        .fetch_all(pool)
        .await?;
        if let [id] = ids.as_slice() {
            return Ok(Identification::Trouve {
                id: *id,
                par: "domaine",
            });
        }
        if ids.len() > 1 {
            return Ok(Identification::Candidats(candidats(pool, &ids).await?));
        }
    }
    let Some(nom) = ind.nom_normalise() else {
        return Ok(Identification::Aucun);
    };
    let fiches = sqlx::query!(
        "select id, nom_normalise from fournisseur where fusionne_dans is null order by id"
    )
    .fetch_all(pool)
    .await?;
    let exacts: Vec<i64> = fiches
        .iter()
        .filter(|f| f.nom_normalise == nom)
        .map(|f| f.id)
        .collect();
    if let Some(id) = exacts.first() {
        return Ok(Identification::Trouve {
            id: *id,
            par: "nom",
        });
    }
    let mut scores: Vec<(f64, i64)> = fiches
        .iter()
        .map(|f| (ressemblance(&nom, &f.nom_normalise), f.id))
        .filter(|(s, _)| *s >= SCORE_CANDIDAT)
        .collect();
    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    match scores.as_slice() {
        [] => Ok(Identification::Aucun),
        [(s, id)] if *s >= SCORE_SUR => Ok(Identification::Trouve {
            id: *id,
            par: "nom",
        }),
        [(s1, id), (s2, _), ..] if *s1 >= SCORE_SUR && s1 - s2 >= 0.2 => {
            Ok(Identification::Trouve {
                id: *id,
                par: "nom",
            })
        }
        _ => {
            let ids: Vec<i64> = scores.iter().take(8).map(|(_, id)| *id).collect();
            Ok(Identification::Candidats(candidats(pool, &ids).await?))
        }
    }
}

pub async fn candidats(pool: &PgPool, ids: &[i64]) -> sqlx::Result<Vec<Candidat>> {
    sqlx::query_as!(
        Candidat,
        "select id, nom, siret, tva, domaines from fournisseur where id = any($1) order by id",
        ids
    )
    .fetch_all(pool)
    .await
}

/// Nouvelle fiche « à confirmer » avec ce qu'on sait. Rend son id, ou rien sans nom ni
/// identifiant.
pub async fn creer(pool: &PgPool, ind: &Indices) -> sqlx::Result<Option<i64>> {
    let nom = ind
        .nom
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| ind.domaine.clone());
    let Some(nom) = nom else {
        return Ok(None);
    };
    let siret = ind.siret_net().filter(|s| s.len() == 14);
    let domaines: Vec<String> = ind.domaine.iter().cloned().collect();
    let id = sqlx::query_scalar!(
        "insert into fournisseur (nom, nom_normalise, siret, tva, domaines)
         values ($1, $2, $3, $4, $5) returning id",
        nom,
        normaliser(&nom),
        siret,
        ind.tva_nette(),
        &domaines
    )
    .fetch_one(pool)
    .await?;
    Ok(Some(id))
}

/// Complète une fiche avec ce qui lui manque : SIRET, TVA (si vides) et domaine. Le domaine
/// n'est appris que si la fiche a été trouvée par un identifiant sûr.
pub async fn apprendre(
    pool: &PgPool,
    id: i64,
    ind: &Indices,
    avec_domaine: bool,
) -> sqlx::Result<()> {
    let siret = ind.siret_net().filter(|s| s.len() == 14);
    let domaine = if avec_domaine {
        ind.domaine.clone()
    } else {
        None
    };
    sqlx::query!(
        "update fournisseur set siret = coalesce(siret, $2), tva = coalesce(tva, $3),
            domaines = case when $4::text is null or $4 = any(domaines) then domaines
                            else array_append(domaines, $4) end
         where id = $1",
        id,
        siret,
        ind.tva_nette(),
        domaine
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Fusionne la fiche `source` dans `cible` : documents rattachés à la cible, domaines et
/// identifiants manquants repris, source marquée comme fusionnée.
pub async fn fusionner(tx: &mut PgConnection, source: i64, cible: i64) -> sqlx::Result<u64> {
    let n = sqlx::query!(
        "update document set fournisseur_id = $2 where fournisseur_id = $1",
        source,
        cible
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query!(
        "update fournisseur c set
            siret = coalesce(c.siret, s.siret), tva = coalesce(c.tva, s.tva),
            societe_defaut_id = coalesce(c.societe_defaut_id, s.societe_defaut_id),
            format_bl = coalesce(c.format_bl, s.format_bl),
            domaines = array(select distinct unnest(c.domaines || s.domaines) order by 1)
         from fournisseur s where c.id = $2 and s.id = $1",
        source,
        cible
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update fournisseur set fusionne_dans = $2 where id = $1 or fusionne_dans = $1",
        source,
        cible
    )
    .execute(&mut *tx)
    .await?;
    Ok(n)
}

/// Fiche dont un SIREN ou un n° de TVA apparaît dans le texte d'un document (avant l'appel à
/// Claude, pour lui donner les exemples de ce fournisseur), s'il y en a une seule.
pub async fn dans_texte(pool: &PgPool, texte: &str) -> sqlx::Result<Option<i64>> {
    let t = tva_compacte(texte);
    let fiches = sqlx::query!(
        "select id, siret, tva from fournisseur
         where fusionne_dans is null and (siret is not null or tva is not null)"
    )
    .fetch_all(pool)
    .await?;
    let trouvees: Vec<i64> = fiches
        .into_iter()
        .filter(|f| {
            f.siret
                .as_deref()
                .is_some_and(|s| s.len() >= 9 && t.contains(&s[..9]))
                || f.tva
                    .as_deref()
                    .is_some_and(|v| v.len() >= 8 && t.contains(v))
        })
        .map(|f| f.id)
        .collect();
    Ok(match trouvees.as_slice() {
        [id] => Some(*id),
        _ => None,
    })
}

/// Fiche connue par le domaine de l'expéditeur, s'il y en a une seule.
pub async fn par_domaine(pool: &PgPool, d: &str) -> sqlx::Result<Option<i64>> {
    let ids = sqlx::query_scalar!(
        "select id from fournisseur where fusionne_dans is null and $1 = any(domaines)",
        d
    )
    .fetch_all(pool)
    .await?;
    Ok(match ids.as_slice() {
        [id] => Some(*id),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms() {
        assert_eq!(
            normaliser("Grossiste Durand & Fils SAS"),
            "grossiste durand fils"
        );
        assert_eq!(
            normaliser("Société Générale d'Électricité"),
            "generale electricite"
        );
        assert!(
            ressemblance(&normaliser("METRO"), &normaliser("Metro Cash & Carry")) >= SCORE_CANDIDAT
        );
        assert!(ressemblance(&normaliser("METRO"), &normaliser("Metro Cash & Carry")) < SCORE_SUR);
        assert_eq!(ressemblance("durand", "martin"), 0.0);
        assert!(ressemblance("grossiste durand fils", "grossiste durand fils") > 0.99);
    }

    #[test]
    fn domaines() {
        assert_eq!(
            domaine("Durand <Factures@Durand.FR>").as_deref(),
            Some("durand.fr")
        );
        assert_eq!(domaine("compta@metro.fr").as_deref(), Some("metro.fr"));
        assert_eq!(domaine("sans adresse"), None);
        let exclus = vec!["magasin.test".to_string()];
        assert_eq!(domaine_utile("x@gmail.com", &exclus), None);
        assert_eq!(domaine_utile("compta@magasin.test", &exclus), None);
        assert_eq!(
            domaine_utile("a@durand.fr", &exclus).as_deref(),
            Some("durand.fr")
        );
    }
}
