//! Serveur du magasin (Apiflow, `https://api.ffnancy.fr`) : adresse (en clair) et clé d'API
//! (chiffrée), essai de connexion, copie des fournisseurs du magasin et codes BO Phénix
//! proposés pour une fiche. Les appels réseau partent toujours du worker ; le jeton n'est
//! jamais renvoyé par l'API de GMFlow.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::crypto::Chiffreur;
use crate::fournisseurs::{
    compact, domaine_generique, meme_nom, normaliser, ressemblance, trigrammes,
};
use crate::parametre;

/// `valeur` = [`ServeurApi`], `valeur_chiffree` = jeton.
pub const CLE: &str = "serveur_api";
/// Dernier essai : `{"quand", "ok", "message"}`.
pub const CLE_TEST: &str = "serveur_api_test";
/// Dernière copie des fournisseurs : `{"quand", "ok", "message", "nombre"}`.
pub const CLE_SYNCHRO: &str = "serveur_api_synchro";
/// Les données du magasin changent une fois par nuit (6 h 45) : une copie toutes les 12 h suffit.
pub const SYNCHRO_HEURES: i32 = 12;
/// Taille de page demandée (maximum d'Apiflow).
const PAGE: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServeurApi {
    pub adresse: String,
    /// 4 derniers caractères du jeton, pour le reconnaître.
    #[serde(default)]
    pub fin: Option<String>,
}

/// Adresse `http(s)://hôte[...]`, sans espace ni identifiants dans l'URL.
pub fn controler_adresse(adresse: &str) -> Result<(), String> {
    let reste = adresse
        .strip_prefix("https://")
        .or_else(|| adresse.strip_prefix("http://"))
        .ok_or("l'adresse doit commencer par https:// ou http://")?;
    let hote = reste.split(['/', '?', '#']).next().unwrap_or("");
    if hote.is_empty() || hote.contains('@') || adresse.chars().any(char::is_whitespace) {
        return Err("adresse invalide (exemple : https://api.ffnancy.fr)".into());
    }
    Ok(())
}

pub fn fin(jeton: &str) -> String {
    let c: Vec<char> = jeton.chars().collect();
    c[c.len().saturating_sub(4)..].iter().collect()
}

/// Racine des routes : `https://api.ffnancy.fr`, `…/` et `…/api` donnent `…/api`.
pub fn racine(adresse: &str) -> String {
    let a = adresse.trim().trim_end_matches('/');
    let a = a.strip_suffix("/api").unwrap_or(a);
    format!("{a}/api")
}

/// Adresse et jeton déchiffré, si l'adresse est configurée.
pub async fn lire(
    pool: &PgPool,
    chiffreur: &Chiffreur,
) -> anyhow::Result<Option<(ServeurApi, Option<String>)>> {
    let Some(v) = parametre::lire(pool, CLE).await? else {
        return Ok(None);
    };
    let s: ServeurApi = serde_json::from_value(v)?;
    let jeton = match parametre::lire_secret(pool, CLE).await? {
        Some(c) => Some(chiffreur.dechiffrer_texte(&c)?),
        None => None,
    };
    Ok(Some((s, jeton)))
}

// ---------- Appels (worker) ----------

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("GMFlow")
        .build()
        .map_err(|e| e.to_string())
}

fn source_la_plus_profonde(e: &(dyn std::error::Error + 'static)) -> String {
    let mut s = e;
    while let Some(suivante) = s.source() {
        s = suivante;
    }
    s.to_string()
}

/// GET authentifié (clé en `X-API-Key`, comme le demande Apiflow) ; erreur en français.
async fn obtenir(url: &str, jeton: Option<&str>) -> Result<Value, String> {
    let mut req = client()?.get(url);
    if let Some(j) = jeton {
        req = req.header("X-API-Key", j);
    }
    let r = req.send().await.map_err(|e| {
        if e.is_timeout() {
            "le serveur ne répond pas".to_string()
        } else if e.is_connect() {
            format!("connexion impossible : {}", source_la_plus_profonde(&e))
        } else {
            source_la_plus_profonde(&e)
        }
    })?;
    let s = r.status().as_u16();
    match s {
        200..=299 => r
            .json::<Value>()
            .await
            .map_err(|_| "réponse illisible (JSON attendu)".to_string()),
        401 | 403 => Err(format!("clé d'API refusée (HTTP {s})")),
        404 => Err("adresse introuvable sur le serveur (HTTP 404) : l'adresse attendue est la racine, ex. https://api.ffnancy.fr".into()),
        429 => Err("trop de requêtes (HTTP 429), réessayer dans une minute".into()),
        c => Err(format!("le serveur a répondu HTTP {c}")),
    }
}

/// Essai de connexion : une page d'un fournisseur ; renvoie le message affiché à l'admin.
pub async fn essayer(adresse: &str, jeton: Option<&str>) -> Result<String, String> {
    let v = obtenir(&format!("{}/fournisseurs?limit=1", racine(adresse)), jeton).await?;
    match v.get("total").and_then(entier) {
        Some(n) => Ok(format!("connexion réussie : {n} fournisseurs au magasin")),
        None => Err("le serveur répond, mais pas comme Apiflow (champ « total » absent)".into()),
    }
}

/// Nombre JSON ou chaîne (« 135046 » : les bigint PostgreSQL arrivent en texte).
fn entier(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn texte(v: &Value, cle: &str) -> Option<String> {
    let t = v.get(cle)?.as_str()?.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FournisseurMagasin {
    pub code: String,
    pub nom: Option<String>,
    pub adresse: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub nb_articles: Option<i64>,
    pub nb_actifs: Option<i64>,
}

/// Tous les fournisseurs du magasin (pages de 5000).
pub async fn fournisseurs(
    adresse: &str,
    jeton: Option<&str>,
) -> Result<Vec<FournisseurMagasin>, String> {
    let base = racine(adresse);
    let mut tous = Vec::new();
    for page in 1..=50 {
        let v = obtenir(
            &format!("{base}/fournisseurs?limit={PAGE}&page={page}"),
            jeton,
        )
        .await?;
        let lignes = v
            .get("fournisseurs")
            .and_then(Value::as_array)
            .ok_or("réponse sans liste « fournisseurs »")?;
        for l in lignes {
            let Some(code) = texte(l, "codefou") else {
                continue;
            };
            tous.push(FournisseurMagasin {
                code,
                nom: texte(l, "nom"),
                adresse: texte(l, "adresse"),
                telephone: texte(l, "telephone"),
                email: texte(l, "email"),
                nb_articles: l.get("nb_articles").and_then(entier),
                nb_actifs: l.get("nb_actifs").and_then(entier),
            });
        }
        let total = v.get("total").and_then(entier).unwrap_or(0) as usize;
        if lignes.is_empty() || tous.len() >= total {
            break;
        }
    }
    Ok(tous)
}

/// Remplace la copie locale. Une liste vide (serveur à moitié synchronisé) ne vide rien.
pub async fn enregistrer(pool: &PgPool, liste: &[FournisseurMagasin]) -> sqlx::Result<()> {
    if liste.is_empty() {
        return Ok(());
    }
    let col =
        |f: fn(&FournisseurMagasin) -> Option<String>| liste.iter().map(f).collect::<Vec<_>>();
    let codes: Vec<String> = liste.iter().map(|f| f.code.clone()).collect();
    let noms = col(|f| f.nom.clone());
    let adresses = col(|f| f.adresse.clone());
    let telephones = col(|f| f.telephone.clone());
    let emails = col(|f| f.email.clone());
    let articles: Vec<Option<i64>> = liste.iter().map(|f| f.nb_articles).collect();
    let actifs: Vec<Option<i64>> = liste.iter().map(|f| f.nb_actifs).collect();
    let mut tx = pool.begin().await?;
    sqlx::query!(
        r#"insert into fournisseur_magasin (code, nom, adresse, telephone, email, nb_articles, nb_actifs, vu_le)
           select c, n, a, t, e, na, nc, now()
           from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::int8[], $7::int8[])
                as x(c, n, a, t, e, na, nc)
           on conflict (code) do update set nom = excluded.nom, adresse = excluded.adresse,
               telephone = excluded.telephone, email = excluded.email,
               nb_articles = excluded.nb_articles, nb_actifs = excluded.nb_actifs, vu_le = now()"#,
        &codes,
        &noms as &[Option<String>],
        &adresses as &[Option<String>],
        &telephones as &[Option<String>],
        &emails as &[Option<String>],
        &articles as &[Option<i64>],
        &actifs as &[Option<i64>],
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "delete from fournisseur_magasin where code <> all($1::text[])",
        &codes
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

// ---------- Codes proposés pour une fiche ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    pub code: String,
    pub nom: Option<String>,
    pub email: Option<String>,
    pub adresse: Option<String>,
    pub nb_articles: Option<i64>,
    /// Pourquoi ce code est proposé (« même domaine mail »…) ; absent pour une recherche.
    pub raison: Option<String>,
    pub score: f64,
    /// Fiche GMFlow qui porte déjà ce code.
    pub deja_pris_par: Option<String>,
}

struct Ligne {
    code: String,
    nom: Option<String>,
    email: Option<String>,
    adresse: Option<String>,
    nb_articles: Option<i64>,
}

/// Score de ressemblance entre une fiche et un fournisseur du magasin, avec sa raison.
pub fn rapprocher(
    nom_fiche: &str,
    autres_noms: &[String],
    domaines: &[String],
    nom_magasin: Option<&str>,
    email_magasin: Option<&str>,
) -> Option<(f64, String)> {
    if let Some(d) = email_magasin.and_then(crate::fournisseurs::domaine)
        && !domaine_generique(&d)
        && domaines.iter().any(|x| x.eq_ignore_ascii_case(&d))
    {
        return Some((1.0, format!("même domaine mail ({d})")));
    }
    let nm = nom_magasin?;
    let mut meilleur: Option<(f64, String)> = None;
    for n in std::iter::once(nom_fiche).chain(autres_noms.iter().map(String::as_str)) {
        let s = if meme_nom(n, nm) {
            (0.95, "même nom".to_string())
        } else {
            // « L 3 C Distribution » commence par « L3C » : un nom suivi d'un complément.
            let (cn, cm) = (compact(n), compact(nm));
            let prefixe =
                cn.len().min(cm.len()) >= 3 && (cn.starts_with(&cm) || cm.starts_with(&cn));
            let r = ressemblance(&normaliser(n), &normaliser(nm))
                .max(trigrammes(&cn, &cm))
                .max(if prefixe { 0.85 } else { 0.0 });
            (r * 0.9, "nom proche".to_string())
        };
        if meilleur.as_ref().is_none_or(|m| s.0 > m.0) {
            meilleur = Some(s);
        }
    }
    meilleur.filter(|(s, _)| *s >= 0.45)
}

/// Codes du magasin pour la fiche `fournisseur_id` : avec `q`, recherche sur le code ou le nom
/// (20 au plus) ; sans, les 5 plus ressemblants.
pub async fn suggestions(
    pool: &PgPool,
    fournisseur_id: i64,
    q: Option<&str>,
) -> sqlx::Result<Vec<Suggestion>> {
    let Some(fiche) = sqlx::query!(
        "select nom, domaines, noms_documents from fournisseur where id = $1",
        fournisseur_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(vec![]);
    };
    let lignes = sqlx::query_as!(
        Ligne,
        "select code, nom, email, adresse, nb_articles from fournisseur_magasin"
    )
    .fetch_all(pool)
    .await?;
    let pris: Vec<(String, i64, String)> = sqlx::query!(
        "select lower(code_gestion) as \"c!\", id, nom from fournisseur
         where code_gestion is not null and fusionne_dans is null"
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| (r.c, r.id, r.nom))
    .collect();

    let q = q.map(str::trim).filter(|q| !q.is_empty());
    let mut res: Vec<Suggestion> = Vec::new();
    for l in lignes {
        let (score, raison) = match q {
            Some(q) => {
                let qn = normaliser(q);
                let qc = compact(q);
                let nom = l.nom.as_deref().unwrap_or("");
                let code_ok = l.code.to_lowercase().contains(&q.to_lowercase());
                let nom_ok = (!qn.is_empty() && normaliser(nom).contains(&qn))
                    || (!qc.is_empty() && compact(nom).contains(&qc));
                if !code_ok && !nom_ok {
                    continue;
                }
                let exact = l.code.eq_ignore_ascii_case(q) || meme_nom(nom, q);
                (if exact { 1.0 } else { 0.5 }, None)
            }
            None => match rapprocher(
                &fiche.nom,
                &fiche.noms_documents,
                &fiche.domaines,
                l.nom.as_deref(),
                l.email.as_deref(),
            ) {
                Some((s, r)) => (s, Some(r)),
                None => continue,
            },
        };
        let deja_pris_par = pris
            .iter()
            .find(|(c, id, _)| *c == l.code.to_lowercase() && *id != fournisseur_id)
            .map(|(_, _, n)| n.clone());
        res.push(Suggestion {
            code: l.code,
            nom: l.nom,
            email: l.email,
            adresse: l.adresse,
            nb_articles: l.nb_articles,
            raison,
            score,
            deja_pris_par,
        });
    }
    res.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.nb_articles.unwrap_or(0).cmp(&a.nb_articles.unwrap_or(0)))
            .then(a.code.cmp(&b.code))
    });
    res.truncate(if q.is_some() { 20 } else { 5 });
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresses() {
        assert!(controler_adresse("https://api.exemple.fr").is_ok());
        assert!(controler_adresse("http://192.168.1.10:8080/v1/").is_ok());
        assert!(controler_adresse("api.exemple.fr").is_err());
        assert!(controler_adresse("https://").is_err());
        assert!(controler_adresse("https://moi:mdp@exemple.fr").is_err());
        assert!(controler_adresse("https://a b.fr").is_err());
        assert_eq!(fin("abcdef"), "cdef");
        assert_eq!(fin("ab"), "ab");
        assert_eq!(
            racine("https://api.ffnancy.fr"),
            "https://api.ffnancy.fr/api"
        );
        assert_eq!(
            racine("https://api.ffnancy.fr/"),
            "https://api.ffnancy.fr/api"
        );
        assert_eq!(
            racine("https://api.ffnancy.fr/api/"),
            "https://api.ffnancy.fr/api"
        );
    }

    #[test]
    fn rapprochements() {
        let d = vec!["jja-sa.com".to_string()];
        let r = rapprocher(
            "JJA SA",
            &[],
            &d,
            Some("JJA"),
            Some("thanh.nguyen@jja-sa.com"),
        )
        .unwrap();
        assert_eq!(r.0, 1.0);
        let r = rapprocher("SAS DECOR10", &[], &[], Some("DECOR 10"), None).unwrap();
        assert_eq!(r.1, "même nom");
        let r = rapprocher(
            "Les Cotillons d'Alsace",
            &[],
            &[],
            Some("CDA - LES COTILLONS D'ALSACES"),
            None,
        )
        .unwrap();
        assert_eq!(r.1, "nom proche");
        assert!(rapprocher("Helios", &[], &[], Some("JJA"), None).is_none());
        // Domaine générique : pas une preuve.
        let g = vec!["gmail.com".to_string()];
        assert!(rapprocher("Durand", &[], &g, Some("MARTIN"), Some("x@gmail.com")).is_none());
        // Autre nom connu de la fiche.
        let r = rapprocher(
            "SARL X",
            &["Sud Trading".into()],
            &[],
            Some("SUD TRADING COMPANY"),
            None,
        )
        .unwrap();
        assert!(r.0 >= 0.45);
    }
}
