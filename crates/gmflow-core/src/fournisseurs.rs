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
             order by groupe_id is not null, id",
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
            domaines = array(select distinct unnest(c.domaines || s.domaines) order by 1),
            noms_documents = array(select distinct unnest(c.noms_documents || s.noms_documents) order by 1),
            groupe_id = case when c.groupe_id = s.id then s.groupe_id else coalesce(c.groupe_id, s.groupe_id) end
         from fournisseur s where c.id = $2 and s.id = $1",
        source,
        cible
    )
    .execute(&mut *tx)
    .await?;
    // IBAN connus : repris par la cible (ceux qu'elle connaît déjà restent les siens).
    sqlx::query!(
        "update fournisseur_iban s set fournisseur_id = $2
         where s.fournisseur_id = $1
           and not exists (select 1 from fournisseur_iban c where c.fournisseur_id = $2 and c.iban = s.iban)",
        source,
        cible
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "delete from fournisseur_iban where fournisseur_id = $1",
        source
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
    // Les marques de la fiche fusionnée passent au groupe gardé (jamais à elles-mêmes).
    sqlx::query!(
        "update fournisseur set groupe_id = case when id = $2 then null else $2 end
         where groupe_id = $1",
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

/// Fournisseur d'un groupe (marque) et les noms qui le désignent sur un document.
#[derive(Debug, Clone, Serialize)]
pub struct Marque {
    pub id: i64,
    pub nom: String,
    pub noms_documents: Vec<String>,
    /// Débuts de codes propres à la marque sur les documents du groupe (« LC » pour
    /// « LC54023001 »).
    pub prefixes_codes: Vec<String>,
}

/// Marques actives du groupe `groupe_id` (vide si la fiche n'est pas un groupe).
pub async fn marques(pool: &PgPool, groupe_id: i64) -> sqlx::Result<Vec<Marque>> {
    sqlx::query_as!(
        Marque,
        "select id, nom, noms_documents, prefixes_codes from fournisseur
         where groupe_id = $1 and fusionne_dans is null order by id",
        groupe_id
    )
    .fetch_all(pool)
    .await
}

/// Vrai si `nom` figure dans le texte : mots normalisés qui se suivent (« Decor 10 » dans
/// « DECOR 10 SAS »), ou nom collé d'au moins 5 caractères (« decor10 » dans « Decor10.fr »).
fn nomme(texte_mots: &str, texte_colle: &str, nom: &str) -> bool {
    let n = normaliser(nom);
    let c = compact(nom);
    (!n.is_empty() && format!(" {texte_mots} ").contains(&format!(" {n} ")))
        || (c.chars().count() >= 5 && texte_colle.contains(&c))
}

/// Marques dont un nom (nom de la fiche ou nom sur les documents) figure dans le texte.
pub fn marques_nommees<'a>(marques: &'a [Marque], texte: &str) -> Vec<&'a Marque> {
    let mots = normaliser(texte);
    let colle: String = texte
        .to_lowercase()
        .chars()
        .map(sans_accent)
        .filter(|c| c.is_alphanumeric())
        .collect();
    marques
        .iter()
        .filter(|m| {
            std::iter::once(&m.nom)
                .chain(m.noms_documents.iter())
                .any(|n| nomme(&mots, &colle, n))
        })
        .collect()
}

/// Vrai si deux noms désignent la même fiche : mêmes lettres et chiffres une fois collés
/// (« L3C » et « L 3 C », « Decor10 » et « DECOR 10 ») ou même nom normalisé (« SAS U10 » et
/// « U10 »).
pub fn meme_nom(a: &str, b: &str) -> bool {
    let (a, b) = (nom_propre(a), nom_propre(b));
    let cle = |n: &str| normaliser(n).replace(' ', "");
    let (ca, cb) = (compact(&a), compact(&b));
    (!ca.is_empty() && ca == cb) || {
        let (na, nb) = (cle(&a), cle(&b));
        !na.is_empty() && na == nb
    }
}

/// Vrai si `entite` est une autre fiche que `groupe` : ni le même nom, ni le groupe suivi
/// d'une ville ou d'un complément (« U10 THIZY » est encore U10).
pub fn entite_distincte(groupe: &str, entite: &str) -> bool {
    if meme_nom(groupe, entite) {
        return false;
    }
    let (g, e) = (
        normaliser(&nom_propre(groupe)),
        normaliser(&nom_propre(entite)),
    );
    !(e.is_empty() || e.starts_with(&format!("{g} ")) || g.starts_with(&format!("{e} ")))
}

/// Groupe et entité lus sur un document, nettoyés, s'ils désignent deux fiches distinctes.
pub fn groupe_et_entite(groupe: Option<&str>, entite: Option<&str>) -> Option<(String, String)> {
    let net = |s: Option<&str>| {
        s.map(nom_propre)
            .filter(|n| compact(n).chars().count() >= 2)
    };
    let (g, e) = (net(groupe)?, net(entite)?);
    entite_distincte(&g, &e).then_some((g, e))
}

/// Mots qui suivent souvent le nom dans un bloc d'en-tête de BL : le nom s'arrête avant.
const FIN_DE_MENTION: &[&str] = &[
    "expedie",
    "expediteur",
    "expedition",
    "livre",
    "livraison",
    "adresse",
    "tel",
    "telephone",
    "fax",
    "date",
    "n",
    "no",
    "num",
    "siret",
    "tva",
    "eori",
    "client",
    "commande",
    "page",
];

/// Société, marque ou département nommé dans le texte d'un document par « Dép. : X »,
/// « Dép : X », « Département X », « Division X » ou « Marque : X » (premier trouvé). X
/// s'arrête au bout de la ligne, à « / », « , », « ; », « | », « ( » ou à deux espaces, et
/// compte au plus 4 mots ; il doit contenir une lettre (« Dép. : 54 » est un département
/// administratif).
pub fn mention_entite(texte: &str) -> Option<String> {
    mention(
        texte,
        &[
            ("departement", false),
            ("division", false),
            ("dep", true),
            ("marque", true),
        ],
    )
}

/// Comme [`mention_entite`], mais seulement « Dép. : X » et « Marque : X » : assez sûr pour
/// faire d'un fournisseur un groupe sans que Claude l'ait lu (« Département : Meurthe-et-
/// Moselle » d'une adresse ne doit pas créer de marque).
pub fn mention_marque(texte: &str) -> Option<String> {
    mention(texte, &[("dep", true), ("marque", true)])
}

/// (repère, deux-points obligatoire)
fn mention(texte: &str, reperes: &[(&str, bool)]) -> Option<String> {
    let orig: Vec<char> = texte.chars().collect();
    let bas: Vec<char> = orig
        .iter()
        .map(|c| sans_accent(c.to_lowercase().next().unwrap_or(*c)))
        .collect();
    let commence = |i: usize, mot: &str| {
        let m: Vec<char> = mot.chars().collect();
        bas.len() >= i + m.len() && bas[i..i + m.len()] == m[..]
    };
    for i in 0..bas.len() {
        if i > 0 && bas[i - 1].is_alphanumeric() {
            continue;
        }
        for (mot, deux_points) in reperes {
            if !commence(i, mot) {
                continue;
            }
            let mut j = i + mot.chars().count();
            if bas.get(j).is_some_and(|c| c.is_alphanumeric()) {
                continue;
            }
            let mut vu_deux_points = false;
            while let Some(c) = bas.get(j) {
                match c {
                    ' ' | '\t' | '.' => {}
                    ':' if !vu_deux_points => vu_deux_points = true,
                    _ => break,
                }
                j += 1;
            }
            if *deux_points && !vu_deux_points {
                continue;
            }
            let mut fin = j;
            while let Some(c) = orig.get(fin) {
                let double_espace = *c == ' ' && orig.get(fin + 1) == Some(&' ');
                if matches!(c, '\n' | '\r' | '\t' | '/' | ',' | ';' | '|' | '(' | ':')
                    || double_espace
                {
                    break;
                }
                fin += 1;
            }
            let brut: String = orig[j..fin].iter().collect();
            let mut mots = Vec::new();
            for m in brut.split_whitespace() {
                let cle = normaliser(m);
                if FIN_DE_MENTION.contains(&cle.as_str()) || mots.len() == 4 {
                    break;
                }
                mots.push(m);
            }
            let x = mots
                .join(" ")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != ')' && c != '&')
                .to_string();
            if x.chars().any(char::is_alphabetic) && (2..=40).contains(&x.chars().count()) {
                return Some(x);
            }
        }
    }
    None
}

/// Marque désignée par un nom (nom de la fiche ou un de ses noms sur les documents), au besoin
/// suivi d'un complément : « HOMEA Maison » est la marque Homea, et « HOMEA » la marque
/// « HOMEA Maison ».
fn marque_de_nom<'a>(marques: &'a [Marque], nom: &str) -> Option<&'a Marque> {
    let noms = |m: &'a Marque| std::iter::once(&m.nom).chain(m.noms_documents.iter());
    marques
        .iter()
        .find(|m| noms(m).any(|n| meme_nom(n, nom)))
        .or_else(|| {
            let lu = normaliser(&nom_propre(nom));
            let mut avec_complement = marques.iter().filter(|m| {
                noms(m).any(|n| {
                    let n = normaliser(&nom_propre(n));
                    !n.is_empty()
                        && !lu.is_empty()
                        && (lu.starts_with(&format!("{n} ")) || n.starts_with(&format!("{lu} ")))
                })
            });
            // Une seule marque, sinon le nom est ambigu.
            match (avec_complement.next(), avec_complement.next()) {
                (Some(m), None) => Some(m),
                _ => None,
            }
        })
}

/// Préfixe de code nettoyé (« lc » → « LC ») ; `None` s'il est vide, trop long ou pas
/// seulement fait de lettres et de chiffres.
pub fn prefixe_code(p: &str) -> Option<String> {
    let p = p.trim().to_uppercase();
    (p.chars().all(|c| c.is_ascii_alphanumeric()) && (1..=8).contains(&p.len())).then_some(p)
}

/// Seule marque dont un préfixe commence un code du texte : le préfixe suivi d'au moins 4
/// chiffres, collés ou après une espace (« LC54023001 », « LC 54023001 »). Aucune si aucun
/// code ne correspond ou si des codes de plusieurs marques figurent dans le texte.
pub fn marque_par_code<'a>(marques: &'a [Marque], texte: &str) -> Option<&'a Marque> {
    let mots: Vec<String> = texte
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|m| !m.is_empty())
        .map(str::to_uppercase)
        .collect();
    let chiffres = |s: &str| s.len() >= 4 && s.bytes().all(|b| b.is_ascii_digit());
    let mut trouvees = marques.iter().filter(|m| {
        m.prefixes_codes
            .iter()
            .filter_map(|p| prefixe_code(p))
            .any(|p| {
                mots.iter().enumerate().any(|(i, mot)| {
                    mot.strip_prefix(p.as_str()).is_some_and(|reste| {
                        chiffres(reste)
                            || (reste.is_empty() && mots.get(i + 1).is_some_and(|s| chiffres(s)))
                    })
                })
            })
    });
    match (trouvees.next(), trouvees.next()) {
        (Some(m), None) => Some(m),
        _ => None,
    }
}

/// Marque d'un document d'un groupe, sans Claude : l'entité lue sur le document, sinon un code
/// qui commence par le préfixe d'une seule marque, sinon la mention « Dép. : X » du texte,
/// sinon la seule marque dont un nom figure dans le texte.
pub fn marque_du_document<'a>(
    marques: &'a [Marque],
    texte: &str,
    entite_lue: Option<&str>,
) -> Option<&'a Marque> {
    if let Some(m) = entite_lue.and_then(|e| marque_de_nom(marques, e)) {
        return Some(m);
    }
    if let Some(m) = marque_par_code(marques, texte) {
        return Some(m);
    }
    if let Some(m) = mention_entite(texte).and_then(|x| marque_de_nom(marques, &x)) {
        return Some(m);
    }
    match marques_nommees(marques, texte).as_slice() {
        [m] => Some(m),
        _ => None,
    }
}

/// Verrou des créations de fiches de groupe et de marque (deux lectures en parallèle ne
/// créent pas deux fois U10) : tenu jusqu'à la fin de la transaction rendue.
pub async fn verrouiller(
    pool: &PgPool,
) -> sqlx::Result<sqlx::Transaction<'static, sqlx::Postgres>> {
    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(7426010013)")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Fiche du groupe lu sur un document (`ind.nom` = nom du groupe) : une fiche hors groupe de
/// même nom, sinon celle de son SIREN ou n° de TVA (ou son groupe), sinon une nouvelle fiche
/// « à confirmer ». Rend aussi vrai si elle a été trouvée par un identifiant. Appeler sous
/// `verrouiller`.
pub async fn groupe_lu(pool: &PgPool, ind: &Indices, entite: &str) -> sqlx::Result<(i64, bool)> {
    let groupe = ind.nom.as_deref().unwrap_or_default();
    let fiches = sqlx::query!(
        "select id, nom, groupe_id from fournisseur where fusionne_dans is null order by id"
    )
    .fetch_all(pool)
    .await?;
    if let Some(f) = fiches
        .iter()
        .find(|f| f.groupe_id.is_none() && meme_nom(&f.nom, groupe))
    {
        return Ok((f.id, false));
    }
    let par_identifiant = Indices {
        nom: None,
        domaine: None,
        ..ind.clone()
    };
    if (!par_identifiant.sirens().is_empty() || par_identifiant.tva_nette().is_some())
        && let Identification::Trouve { id, .. } = identifier(pool, &par_identifiant).await?
        && let Some(f) = fiches.iter().find(|f| f.id == id)
    {
        if let Some(g) = f.groupe_id {
            return Ok((g, true));
        }
        // La fiche du SIREN est l'entité elle-même (créée avant de connaître le groupe) :
        // elle ira dans le groupe, qui reste à créer.
        if !meme_nom(&f.nom, entite) {
            return Ok((f.id, true));
        }
    }
    let id = creer(pool, ind).await?.expect("nom du groupe non vide");
    Ok((id, false))
}

/// Ajoute `nom` aux noms sur les documents de la fiche s'il est écrit autrement que son nom.
async fn retenir_nom(pool: &PgPool, id: i64, nom: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "update fournisseur set noms_documents = array_append(noms_documents, $2)
         where id = $1 and nom <> $2 and not ($2 = any(noms_documents))
           and cardinality(noms_documents) < 20",
        id,
        nom.trim()
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Fiche de l'entité `entite` dans le groupe `groupe` : une marque du groupe de ce nom, sinon
/// une fiche hors groupe de ce nom (rattachée au groupe, sauf si elle est elle-même un
/// groupe), sinon une nouvelle marque « à confirmer ». Rend aussi vrai si le groupe a gagné
/// une marque (les documents restés sur le groupe sont alors à répartir). Appeler sous
/// `verrouiller`.
pub async fn marque_lue(pool: &PgPool, groupe: i64, entite: &str) -> sqlx::Result<(i64, bool)> {
    let entite = nom_propre(entite);
    let marques = marques(pool, groupe).await?;
    if let Some(m) = marque_de_nom(&marques, &entite) {
        retenir_nom(pool, m.id, &entite).await?;
        return Ok((m.id, false));
    }
    let fiches = sqlx::query!(
        r#"select f.id, f.nom, f.noms_documents, f.groupe_id,
                  exists (select 1 from fournisseur m where m.groupe_id = f.id and m.fusionne_dans is null) as "est_groupe!"
           from fournisseur f where f.fusionne_dans is null and f.id <> $1 order by f.id"#,
        groupe
    )
    .fetch_all(pool)
    .await?;
    if let Some(f) = fiches.iter().find(|f| {
        std::iter::once(&f.nom)
            .chain(f.noms_documents.iter())
            .any(|n| meme_nom(n, &entite))
    }) {
        if f.groupe_id.is_none() && !f.est_groupe {
            sqlx::query!(
                "update fournisseur set groupe_id = $2 where id = $1",
                f.id,
                groupe
            )
            .execute(pool)
            .await?;
            retenir_nom(pool, f.id, &entite).await?;
            return Ok((f.id, true));
        }
        return Ok((f.id, false));
    }
    let id = sqlx::query_scalar!(
        "insert into fournisseur (nom, nom_normalise, groupe_id) values ($1, $2, $3) returning id",
        entite,
        normaliser(&entite),
        groupe
    )
    .fetch_one(pool)
    .await?;
    Ok((id, true))
}

/// Range chez leur marque les documents restés sur le groupe `groupe` (sans Claude, voir
/// `marque_du_document`), sauf ceux dont le fournisseur a été choisi à la main. Rend les
/// (document, marque) déplacés et le nombre de documents examinés.
pub async fn repartir(pool: &PgPool, groupe: i64) -> sqlx::Result<(Vec<(i64, i64)>, usize)> {
    let marques = marques(pool, groupe).await?;
    if marques.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let docs = sqlx::query!(
        r#"select d.id, d.fournisseur_entite,
                  coalesce(d.fournisseur_nom, '') || E'\n' || m.objet || E'\n' || coalesce(d.texte, '') as "texte!"
           from document d join mail m on m.id = d.mail_id
           where d.fournisseur_id = $1 and not (d.corrections ? 'fournisseur_id')"#,
        groupe
    )
    .fetch_all(pool)
    .await?;
    let deplaces: Vec<(i64, i64)> = docs
        .iter()
        .filter_map(|d| {
            marque_du_document(&marques, &d.texte, d.fournisseur_entite.as_deref())
                .map(|m| (d.id, m.id))
        })
        .collect();
    let mut tx = pool.begin().await?;
    for (doc, marque) in &deplaces {
        sqlx::query!(
            "update document set fournisseur_id = $2 where id = $1 and fournisseur_id = $3",
            doc,
            marque,
            groupe
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((deplaces, docs.len()))
}

/// Deux fiches qui désignent probablement le même fournisseur, et pourquoi.
#[derive(Debug, Clone, Serialize)]
pub struct Doublon {
    pub a: i64,
    pub b: i64,
    pub raison: &'static str,
    /// Identifiant, domaine ou adresse en commun mais noms clairement différents : deux
    /// marques d'un même groupe (ou une marque et son groupe), à regrouper, pas à fusionner.
    pub meme_groupe: bool,
}

/// Paires de fiches actives qui se ressemblent (même SIREN, domaine commun, même nom collé,
/// nom de l'une = domaine de l'autre, noms très proches), hors paires marquées « pas un
/// doublon ». Même SIREN, domaine ou adresse avec des noms clairement différents : « même
/// groupe probable » (L3C et Homea au SIREN de U10). La plus ancienne d'abord dans chaque paire.
pub async fn doublons_probables(pool: &PgPool) -> sqlx::Result<Vec<Doublon>> {
    let fiches = sqlx::query!(
        "select id, nom, nom_normalise, siret, tva, domaines, groupe_id, adresse from fournisseur
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
            let adresse = f
                .adresse
                .as_deref()
                .map(normaliser)
                .filter(|a| a.len() >= 10);
            (f, sirens(&f.siret, &f.tva), colles, adresse)
        })
        .collect();
    let mut v = Vec::new();
    for (i, (fa, sa, ca, ada)) in prep.iter().enumerate() {
        for (fb, sb, cb, adb) in &prep[i + 1..] {
            // Une marque et son groupe (ou deux marques du même groupe) partagent le SIREN.
            let meme_groupe = fa.groupe_id == Some(fb.id)
                || fb.groupe_id == Some(fa.id)
                || (fa.groupe_id.is_some() && fa.groupe_id == fb.groupe_id);
            if meme_groupe || distincts.contains(&(fa.id, fb.id)) {
                continue;
            }
            let meme_siren = sa.iter().any(|s| sb.contains(s));
            let meme_domaine = fa.domaines.iter().any(|d| fb.domaines.contains(d));
            let meme_adresse = ada.is_some() && ada == adb;
            // Noms seuls (sans les domaines, que deux marques d'un groupe partagent).
            let (na, nb) = (compact(&fa.nom), compact(&fb.nom));
            let noms_differents = fa.nom_normalise != fb.nom_normalise
                && na != nb
                && fa.nom_normalise.replace(' ', "") != fb.nom_normalise.replace(' ', "")
                && ressemblance(&fa.nom_normalise, &fb.nom_normalise) < SCORE_CANDIDAT
                && trigrammes(&na, &nb) < TRIGRAMMES_CANDIDAT;
            if (meme_siren || meme_domaine || meme_adresse) && noms_differents {
                v.push(Doublon {
                    a: fa.id,
                    b: fb.id,
                    raison: "même groupe probable",
                    meme_groupe: true,
                });
                continue;
            }
            let raison = if meme_siren {
                "même SIREN"
            } else if meme_domaine {
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
                meme_groupe: false,
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
    fn marque_par_prefixe_de_code() {
        let m = |id, nom: &str, p: &str| Marque {
            id,
            nom: nom.into(),
            noms_documents: Vec::new(),
            prefixes_codes: vec![p.into()],
        };
        let l = [
            m(1, "L3C", "LC"),
            m(2, "Decor10", "ld"),
            m(3, "Homea", "LH"),
        ];
        let id = |t: &str| marque_du_document(&l, t, None).map(|m| m.id);
        assert_eq!(id("U10 SAS\nCode client : : LC54023001\nBL 1234"), Some(1));
        assert_eq!(id("N° BL LD 00451287"), Some(2));
        assert_eq!(id("bl-lh778812.pdf"), Some(3));
        // Codes de deux marques : rien ne tranche.
        assert_eq!(id("LC54023001 LH54023001"), None);
        // Préfixe sans chiffres derrière, ou trop peu : pas un code.
        assert_eq!(id("LCD 12 rue des Lilas LC12"), None);
        assert_eq!(id("CLC54023001"), None);
        // L'entité lue passe avant le code.
        assert_eq!(
            marque_du_document(&l, "LC54023001", Some("Homea")).map(|m| m.id),
            Some(3)
        );
        assert_eq!(prefixe_code(" lc "), Some("LC".into()));
        assert_eq!(prefixe_code("L-C"), None);
        assert_eq!(prefixe_code(""), None);
    }

    #[test]
    fn marques_dans_le_texte() {
        let m = |id, nom: &str, autres: &[&str]| Marque {
            id,
            nom: nom.into(),
            noms_documents: autres.iter().map(|s| s.to_string()).collect(),
            prefixes_codes: Vec::new(),
        };
        let l = [
            m(1, "L3C", &[]),
            m(2, "Homea", &[]),
            m(3, "Decor10", &["Decor 10"]),
        ];
        let ids = |t: &str| {
            marques_nommees(&l, t)
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("SAS U10 - Marque HOMEA - Facture 12"), vec![2]);
        assert_eq!(ids("DECOR 10 collection hiver"), vec![3]);
        assert_eq!(ids("www.decor10.fr"), vec![3]);
        assert_eq!(ids("Bon de livraison L3C n° 44"), vec![1]);
        assert_eq!(ids("Homeland L3CX"), Vec::<i64>::new());
        assert_eq!(ids("Homea et L3C"), vec![1, 2]);
    }

    #[test]
    fn mention_dans_le_texte() {
        let m = |t: &str| mention_entite(t);
        // L'en-tête réel du BL U10.
        assert_eq!(
            m("U10\nDép. : L3C / EXPEDIE PAR : U10 THIZY, 124 Av. des Granges 69240 THIZY LES BOURGS, N° EORI : FR451341465").as_deref(),
            Some("L3C")
        );
        assert_eq!(m("DEP : HOMEA\nBL 12").as_deref(), Some("HOMEA"));
        assert_eq!(m("Dép.: Decor 10  Tél 04 74").as_deref(), Some("Decor 10"));
        assert_eq!(m("Dep. : L3C EXPEDIE PAR U10").as_deref(), Some("L3C"));
        assert_eq!(
            m("Département Maison & Jardin\n").as_deref(),
            Some("Maison & Jardin")
        );
        assert_eq!(m("DIVISION : Textile, Lyon").as_deref(), Some("Textile"));
        assert_eq!(m("Marque : Homea").as_deref(), Some("Homea"));
        // Pas des mentions d'entité.
        assert_eq!(m("Dépôt : Thizy"), None);
        assert_eq!(m("Dép. : 54"), None);
        assert_eq!(m("Dép Lorraine"), None, "deux-points obligatoire après Dép");
        assert_eq!(m("Marque déposée"), None);
        assert_eq!(m("Les marques : toutes"), None);
        assert_eq!(m("Remarque : livrer le matin"), None);
        assert_eq!(m("Aucune mention ici"), None);
        // Repères sûrs seulement.
        assert_eq!(
            mention_marque("Dép. : HOMEA Maison\nEXPEDIE PAR : U10 Strasbourg").as_deref(),
            Some("HOMEA Maison")
        );
        assert_eq!(mention_marque("Département : Meurthe et Moselle"), None);
    }

    #[test]
    fn groupe_entite_et_noms() {
        assert!(meme_nom("L3C", "L 3 C"));
        assert!(meme_nom("l3c", "L3C (Thizy)"));
        assert!(meme_nom("SAS U10", "U10"));
        assert!(meme_nom("Decor10", "DECOR 10"));
        assert!(!meme_nom("L3C", "U10"));
        assert_eq!(
            groupe_et_entite(Some("U10"), Some("L3C")),
            Some(("U10".into(), "L3C".into()))
        );
        assert_eq!(groupe_et_entite(Some("U10"), Some("U 10")), None);
        assert_eq!(groupe_et_entite(Some("U10"), Some("U10 THIZY")), None);
        assert_eq!(groupe_et_entite(Some("U10"), None), None);
        assert_eq!(groupe_et_entite(None, Some("L3C")), None);
        let m = |id, nom: &str, autres: &[&str]| Marque {
            id,
            nom: nom.into(),
            noms_documents: autres.iter().map(|s| s.to_string()).collect(),
            prefixes_codes: Vec::new(),
        };
        let l = [m(1, "L3C", &[]), m(2, "Homea", &["HOMEA DECO"])];
        let id = |t: &str, e: Option<&str>| marque_du_document(&l, t, e).map(|m| m.id);
        assert_eq!(id("BL 44", Some("L 3 C")), Some(1));
        assert_eq!(id("Dép. : HOMEA DECO / EXPEDIE PAR U10", None), Some(2));
        assert_eq!(id("Homea et L3C", None), None);
        assert_eq!(id("Dép. : L3C\nRéassort Homea", None), Some(1));
        // L'en-tête réel d'une facture U10 : la marque suivie d'un complément.
        assert_eq!(id("Facture", Some("HOMEA Maison")), Some(2));
        assert_eq!(id("Facture", Some("Homeaux")), None);
        let l = [m(1, "HOMEA Maison", &[])];
        assert_eq!(
            marque_du_document(&l, "BL", Some("Homea")).map(|m| m.id),
            Some(1)
        );
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
