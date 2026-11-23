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

/// Ressemblance par trigrammes à partir de laquelle une fiche est montrée à Claude.
pub const TRIGRAMMES_CANDIDAT: f64 = 0.45;

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

/// Nom collé, sans accents ni ponctuation, sans extension s'il s'agit d'un domaine :
/// « La Foir'Fouille » et « lafoirfouille.fr » donnent tous deux « lafoirfouille ».
pub fn compact(nom: &str) -> String {
    let t = nom.trim().to_lowercase();
    let t = t.strip_prefix("www.").unwrap_or(&t);
    let t = if ressemble_a_un_domaine(t) {
        t.rsplit_once('.').map_or(t, |(a, _)| a)
    } else {
        t
    };
    t.chars()
        .map(sans_accent)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// « lafoirfouille.fr », « glama.fr » : un nom de fiche tiré du domaine faute de mieux.
pub fn ressemble_a_un_domaine(nom: &str) -> bool {
    let t = nom.trim();
    !t.contains(' ')
        && t.rsplit_once('.').is_some_and(|(a, ext)| {
            !a.is_empty()
                && (2..=6).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphabetic())
        })
}

/// Nom lu, sans le complément entre parenthèses que Claude ajoute parfois
/// (« U10 (SAS U10, Lingolsheim) » -> « U10 »).
pub fn nom_propre(nom: &str) -> String {
    let t = nom.trim();
    match t.find(" (") {
        Some(i) if t.ends_with(')') && !t[..i].trim().is_empty() => t[..i].trim().to_string(),
        _ => t.to_string(),
    }
}

/// Ressemblance de deux noms collés par trigrammes (indice de Dice), de 0 à 1 : rattrape
/// « foirfouille » / « lafoirfouille » que les mots ne voient pas.
pub fn trigrammes(a: &str, b: &str) -> f64 {
    fn t(s: &str) -> BTreeSet<[char; 3]> {
        let c: Vec<char> = format!("  {s} ").chars().collect();
        c.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
    }
    if a.len() < 3 || b.len() < 3 {
        return 0.0;
    }
    let (ta, tb) = (t(a), t(b));
    2.0 * ta.intersection(&tb).count() as f64 / (ta.len() + tb.len()) as f64
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
    /// SIRET (14 chiffres) ou SIREN (9) dont la clé est juste : un chiffre mal lu sur un scan
    /// ne devient pas l'identifiant d'une fiche.
    fn siret_net(&self) -> Option<String> {
        self.siret.as_deref().map(chiffres).filter(|c| {
            (c.len() == 14 && crate::controles::siret_valide(c))
                || (c.len() == 9 && crate::controles::siren_valide(c))
        })
    }

    /// N° de TVA lu, s'il est valide (clé vérifiée pour un numéro français).
    fn tva_lue(&self) -> Option<String> {
        self.tva
            .as_deref()
            .map(tva_compacte)
            .filter(|t| crate::controles::tva_valide(t))
    }

    /// N° de TVA lu, sinon celui déduit du SIREN (FR + clé + SIREN) : une fiche dont la
    /// facture ne donnait que le SIREN (« RCS Nancy 123 456 789 ») garde un identifiant.
    fn tva_nette(&self) -> Option<String> {
        self.tva_lue().or_else(|| {
            let siren = self.siret_net()?[..9].to_string();
            let cle = crate::controles::cle_tva_fr(&siren)?;
            Some(format!("FR{cle:02}{siren}"))
        })
    }

    /// SIREN connus (du SIRET, ou d'un n° de TVA français).
    pub fn sirens(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(s) = self.siret_net() {
            v.push(s[..9].to_string());
        }
        if let Some(t) = self.tva_lue()
            && t.starts_with("FR")
            && t.len() == 13
        {
            v.push(t[4..].to_string());
        }
        v
    }

    /// Oublie SIRET et TVA quand ce sont ceux d'une de nos sociétés (Claude a pris
    /// l'acheteur pour l'émetteur) : sinon toutes ses factures iraient à ce fournisseur.
    pub fn sans_identifiants_de(&mut self, sirens_societes: &[String]) {
        if self.sirens().iter().any(|s| sirens_societes.contains(s)) {
            self.siret = None;
            self.tva = None;
        }
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
    let nom = ind.nom_normalise();
    let colle = ind
        .nom
        .as_deref()
        .map(|n| compact(&nom_propre(n)))
        .filter(|c| c.len() >= 3);
    // Le domaine de l'expéditeur sans extension, comparé aux noms des fiches.
    let colle_domaine = ind.domaine.as_deref().map(compact).filter(|c| c.len() >= 3);
    if nom.is_none() && colle.is_none() && colle_domaine.is_none() {
        return Ok(Identification::Aucun);
    }
    let fiches = sqlx::query!(
        "select id, nom, nom_normalise, domaines from fournisseur where fusionne_dans is null order by id"
    )
    .fetch_all(pool)
    .await?;
    let mut exacts: Vec<i64> = Vec::new();
    let mut scores: Vec<(f64, i64)> = Vec::new();
    for f in &fiches {
        let colles_fiche: Vec<String> = std::iter::once(compact(&f.nom))
            .chain(std::iter::once(f.nom_normalise.replace(' ', "")))
            .chain(f.domaines.iter().map(|d| compact(d)))
            .filter(|c| c.len() >= 3)
            .collect();
        let meme_nom = nom.as_deref().is_some_and(|n| *n == f.nom_normalise);
        let meme_colle = colle
            .iter()
            .chain(colle_domaine.iter())
            .any(|c| colles_fiche.contains(c));
        if meme_nom || meme_colle {
            exacts.push(f.id);
            continue;
        }
        let mots = nom
            .as_deref()
            .map_or(0.0, |n| ressemblance(n, &f.nom_normalise));
        let tri = colle
            .iter()
            .chain(colle_domaine.iter())
            .flat_map(|c| colles_fiche.iter().map(move |x| trigrammes(c, x)))
            .fold(0.0, f64::max);
        // Les trigrammes seuls ne suffisent jamais à rattacher : candidate pour Claude.
        let s = if mots >= SCORE_CANDIDAT {
            mots
        } else if tri >= TRIGRAMMES_CANDIDAT {
            SCORE_CANDIDAT.max(tri.min(SCORE_SUR - 0.01))
        } else {
            0.0
        };
        if s > 0.0 {
            scores.push((s, f.id));
        }
    }
    if let Some(id) = exacts.first() {
        return Ok(Identification::Trouve {
            id: *id,
            par: "nom",
        });
    }
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

/// SIREN de nos sociétés (SIRET ou n° de TVA français) : jamais ceux d'un fournisseur.
pub async fn sirens_societes(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    let lignes = sqlx::query!("select siret, tva from societe")
        .fetch_all(pool)
        .await?;
    let mut v = Vec::new();
    for l in lignes {
        let ind = Indices {
            siret: l.siret,
            tva: l.tva,
            ..Default::default()
        };
        v.extend(ind.sirens());
    }
    Ok(v)
}

/// Nouvelle fiche « à confirmer » avec ce qu'on sait. Rend son id, ou rien sans nom ni
/// identifiant.
pub async fn creer(pool: &PgPool, ind: &Indices) -> sqlx::Result<Option<i64>> {
    let nom = ind
        .nom
        .as_deref()
        .map(nom_propre)
        .filter(|n| !n.is_empty())
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
    // Fiche nommée d'après un domaine faute de nom lu (« lafoirfouille.fr ») : prend le
    // nom lu sur ce document, si c'en est un vrai.
    let nom = ind
        .nom
        .as_deref()
        .map(nom_propre)
        .filter(|n| !n.is_empty() && !ressemble_a_un_domaine(n));
    sqlx::query!(
        "update fournisseur set siret = coalesce(siret, $2), tva = coalesce(tva, $3),
            domaines = case when $4::text is null or $4 = any(domaines) then domaines
                            else array_append(domaines, $4) end,
            nom = case when $5::text is not null and nom ~ '^[^ ]+\\.[a-zA-Z]{2,6}$' then $5 else nom end,
            nom_normalise = case when $5::text is not null and nom ~ '^[^ ]+\\.[a-zA-Z]{2,6}$' then $6 else nom_normalise end
         where id = $1",
        id,
        siret,
        ind.tva_nette(),
        domaine,
        nom,
        nom.as_deref().map(normaliser)
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
            code_gestion = coalesce(c.code_gestion, s.code_gestion),
            adresse = coalesce(c.adresse, s.adresse), telephone = coalesce(c.telephone, s.telephone),
            email = coalesce(c.email, s.email), interlocuteur = coalesce(c.interlocuteur, s.interlocuteur),
            conditions_paiement = coalesce(c.conditions_paiement, s.conditions_paiement),
            notes = case when s.notes is null then c.notes when c.notes is null then s.notes
                         else c.notes || E'\n' || s.notes end,
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
                || f.tva.as_deref().is_some_and(|v| {
                    v.len() >= 8 && t.contains(v)
                        || (v.starts_with("FR") && v.len() == 13 && t.contains(&v[4..]))
                })
        })
        .map(|f| f.id)
        .collect();
    Ok(match trouvees.as_slice() {
        [id] => Some(*id),
        _ => None,
    })
}

/// Deux fiches qui désignent probablement le même fournisseur, et pourquoi.
#[derive(Debug, Clone, Serialize)]
pub struct Doublon {
    pub a: i64,
    pub b: i64,
    pub raison: &'static str,
}

/// Paires de fiches actives qui se ressemblent (même SIREN, domaine commun, même nom collé,
/// nom de l'une = domaine de l'autre, noms très proches), hors paires marquées « pas un
/// doublon ». La plus ancienne d'abord dans chaque paire.
pub async fn doublons_probables(pool: &PgPool) -> sqlx::Result<Vec<Doublon>> {
    let fiches = sqlx::query!(
        "select id, nom, nom_normalise, siret, tva, domaines from fournisseur
         where fusionne_dans is null order by id"
    )
    .fetch_all(pool)
    .await?;
    let distincts: BTreeSet<(i64, i64)> = sqlx::query!("select a, b from fournisseur_distinct")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.a, r.b))
        .collect();
    let sirens = |siret: &Option<String>, tva: &Option<String>| {
        Indices {
            siret: siret.clone(),
            tva: tva.clone(),
            ..Default::default()
        }
        .sirens()
    };
    let prep: Vec<_> = fiches
        .iter()
        .map(|f| {
            let colles: BTreeSet<String> = std::iter::once(compact(&f.nom))
                .chain(std::iter::once(f.nom_normalise.replace(' ', "")))
                .chain(f.domaines.iter().map(|d| compact(d)))
                .filter(|c| c.len() >= 3)
                .collect();
            (f, sirens(&f.siret, &f.tva), colles)
        })
        .collect();
    let mut v = Vec::new();
    for (i, (fa, sa, ca)) in prep.iter().enumerate() {
        for (fb, sb, cb) in &prep[i + 1..] {
            if distincts.contains(&(fa.id, fb.id)) {
                continue;
            }
            let raison = if sa.iter().any(|s| sb.contains(s)) {
                "même SIREN"
            } else if fa.domaines.iter().any(|d| fb.domaines.contains(d)) {
                "même domaine mail"
            } else if !fa.nom_normalise.is_empty() && fa.nom_normalise == fb.nom_normalise
                || ca.intersection(cb).next().is_some()
            {
                "même nom"
            } else if ressemblance(&fa.nom_normalise, &fb.nom_normalise) >= SCORE_SUR {
                "noms très proches"
            } else {
                continue;
            };
            v.push(Doublon {
                a: fa.id,
                b: fb.id,
                raison,
            });
        }
    }
    Ok(v)
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
    fn noms_colles() {
        assert_eq!(compact("La Foir'Fouille"), "lafoirfouille");
        assert_eq!(compact("lafoirfouille.fr"), "lafoirfouille");
        assert_eq!(compact("www.Glama.fr"), "glama");
        assert_eq!(compact("Société Générale"), "societegenerale");
        assert!(ressemble_a_un_domaine("glama.fr"));
        assert!(!ressemble_a_un_domaine("La Foir'Fouille"));
        assert!(!ressemble_a_un_domaine("M. Dupont"));
        assert_eq!(nom_propre("U10 (SAS U10, Lingolsheim)"), "U10");
        assert_eq!(nom_propre("(sans nom)"), "(sans nom)");
        assert_eq!(nom_propre("Durand"), "Durand");
        assert!(trigrammes("foirfouille", "lafoirfouille") >= TRIGRAMMES_CANDIDAT);
        assert!(trigrammes("durand", "martin") < TRIGRAMMES_CANDIDAT);
    }

    #[test]
    fn identifiants_lus() {
        let ind = |siret: Option<&str>, tva: Option<&str>| Indices {
            siret: siret.map(str::to_string),
            tva: tva.map(str::to_string),
            ..Default::default()
        };
        // SIREN seul (« RCS Nancy 552 100 554 ») : gardé, n° de TVA déduit.
        let i = ind(Some("552 100 554"), None);
        assert_eq!(i.sirens(), vec!["552100554"]);
        assert_eq!(i.tva_nette().as_deref(), Some("FR96552100554"));
        // Un chiffre mal lu : ni SIREN ni TVA.
        let i = ind(Some("552 100 555"), Some("FR96552100555"));
        assert!(i.sirens().is_empty());
        assert_eq!(i.tva_nette(), None);
        // TVA étrangère gardée telle quelle.
        assert_eq!(
            ind(None, Some("DE 123456789")).tva_nette().as_deref(),
            Some("DE123456789")
        );
        // SIRET ou TVA d'une de nos sociétés : oubliés.
        let mut i = ind(Some("55210055400013"), Some("FR96552100554"));
        i.sans_identifiants_de(&["552100554".to_string()]);
        assert_eq!((i.siret, i.tva), (None, None));
        let mut i = ind(None, Some("FR96552100554"));
        i.sans_identifiants_de(&["123456789".to_string()]);
        assert!(i.tva.is_some());
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
