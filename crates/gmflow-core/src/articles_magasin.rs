//! Articles d'un BL retrouvés sur le serveur du magasin (Apiflow) : par gencod, sinon par
//! référence fournisseur, puis, pour un lot, le détail de ses composants. Le résultat est
//! gardé sur chaque ligne (`ligne_document.magasin`) : l'export vers BO Phénix le lit sans
//! appel réseau. Les appels partent du worker.

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;

use crate::file;
use crate::serveur_api::{entier, obtenir, racine, texte};

/// Composant d'un lot, tel que le magasin le connaît.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Composant {
    pub no_id: String,
    pub codein: Option<String>,
    pub libelle: Option<String>,
    pub lib_ticket: Option<String>,
    pub gencod: Option<String>,
    pub reference: Option<String>,
    /// Quantité du composant dans un lot.
    pub quantite: f64,
    /// Prix d'achat HT du magasin.
    pub pa: Option<f64>,
    /// Prix de vente TTC du magasin.
    pub pv: Option<f64>,
    pub pv_conseille: Option<f64>,
    pub eco_ht: Option<f64>,
    /// Code de famille de 3e niveau.
    pub famille: Option<String>,
    pub pourc_marge: Option<f64>,
}

/// Article d'une ligne de BL retrouvé au magasin.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArticleMagasin {
    /// « gencod » ou « reference » : ce qui a permis de le retrouver.
    pub par: String,
    pub no_id: String,
    pub codein: Option<String>,
    pub libelle: Option<String>,
    pub gencod: Option<String>,
    /// Vide si l'article n'est pas un lot.
    #[serde(default)]
    pub composants: Vec<Composant>,
}

/// Ce qu'on cherche pour une ligne.
#[derive(Debug, Clone, Default)]
pub struct Recherche {
    pub gencod: Option<String>,
    pub reference: Option<String>,
}

/// Lecture d'une route d'Apiflow (`chemin` sous `/api`, paramètres de requête).
pub trait Source {
    fn obtenir(
        &self,
        chemin: &str,
        params: &[(&str, &str)],
    ) -> impl Future<Output = Result<Value, String>> + Send;
}

pub struct Http {
    pub base: String,
    pub jeton: Option<String>,
}

impl Http {
    pub fn new(adresse: &str, jeton: Option<String>) -> Self {
        Http {
            base: racine(adresse),
            jeton,
        }
    }
}

impl Source for Http {
    async fn obtenir(&self, chemin: &str, params: &[(&str, &str)]) -> Result<Value, String> {
        let url = reqwest::Url::parse_with_params(&format!("{}{chemin}", self.base), params)
            .map_err(|e| e.to_string())?;
        // 600 requêtes par minute au plus : sur un gros BL, on attend que le quota revienne.
        for essai in 0..3 {
            match obtenir(url.as_str(), self.jeton.as_deref()).await {
                Err(m) if m.contains("HTTP 429") && essai < 2 => {
                    tokio::time::sleep(Duration::from_secs(30)).await
                }
                r => return r,
            }
        }
        unreachable!()
    }
}

fn nombre(v: &Value, cle: &str) -> Option<f64> {
    let x = v.get(cle)?;
    x.as_f64()
        .or_else(|| x.as_str()?.trim().replace(',', ".").parse().ok())
}

/// Identifiant : bigint en texte ou en nombre.
fn id(v: &Value, cle: &str) -> Option<String> {
    texte(v, cle).or_else(|| v.get(cle).and_then(entier).map(|n| n.to_string()))
}

/// Référence comparée sans casse ni espaces.
pub fn normaliser_ref(r: &str) -> String {
    r.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

fn gencod_propre(g: &str) -> Option<String> {
    let g: String = g.chars().filter(|c| !c.is_whitespace()).collect();
    (g.len() >= 8 && g.bytes().all(|b| b.is_ascii_digit())).then_some(g)
}

/// Codes BO Phénix d'une fiche (« J009, J012 » : plusieurs codes).
pub fn codes(code_gestion: Option<&str>) -> Vec<String> {
    code_gestion
        .unwrap_or("")
        .split([',', ';', '/', ' '])
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect()
}

fn liste<'a>(v: &'a Value, cle: &str) -> &'a [Value] {
    v.get(cle)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn article(par: &str, a: &Value) -> Option<ArticleMagasin> {
    Some(ArticleMagasin {
        par: par.to_string(),
        no_id: id(a, "no_id")?,
        codein: texte(a, "codein"),
        libelle: texte(a, "libelle1"),
        gencod: texte(a, "gtin"),
        composants: Vec::new(),
    })
}

/// Article dont un GTIN est exactement `g`.
async fn par_gencod<S: Source>(s: &S, g: &str) -> Result<Option<ArticleMagasin>, String> {
    let v = s
        .obtenir("/articles", &[("ean", g), ("limit", "20")])
        .await?;
    let candidats = liste(&v, "articles");
    if let Some(a) = candidats
        .iter()
        .find(|a| texte(a, "gtin").as_deref() == Some(g))
    {
        return Ok(article("gencod", a));
    }
    // Le gencod lu n'est pas le GTIN préférentiel : on regarde tous ceux des candidats.
    for a in candidats.iter().take(5) {
        let Some(no_id) = id(a, "no_id") else {
            continue;
        };
        let d = s.obtenir(&format!("/articles/{no_id}"), &[]).await?;
        if liste(&d, "gtins")
            .iter()
            .any(|x| texte(x, "gtin").as_deref() == Some(g))
        {
            let mut r = article("gencod", a);
            if let Some(r) = r.as_mut() {
                r.gencod = Some(g.to_string());
            }
            return Ok(r);
        }
    }
    Ok(None)
}

/// Article dont la référence fournisseur est `reference`, chez l'un des `codes` (n'importe
/// quel fournisseur si aucun code). Chaque candidat est revérifié sur sa fiche : un Apiflow
/// sans filtre `ref` renverrait n'importe quels articles.
async fn par_reference<S: Source>(
    s: &S,
    reference: &str,
    codes: &[String],
) -> Result<Option<ArticleMagasin>, String> {
    let cible = normaliser_ref(reference);
    let essais: Vec<Option<&str>> = if codes.is_empty() {
        vec![None]
    } else {
        codes.iter().map(|c| Some(c.as_str())).collect()
    };
    for code in essais {
        let mut params = vec![("ref", reference), ("limit", "20")];
        if let Some(c) = code {
            params.push(("codefou", c));
        }
        let v = s.obtenir("/articles", &params).await?;
        let mut trouves: Vec<(bool, ArticleMagasin)> = Vec::new();
        for a in liste(&v, "articles").iter().take(10) {
            // Liste filtrée sur la référence préférentielle : pas besoin de la fiche.
            let pref = texte(a, "ref_fou_principale").is_some_and(|r| normaliser_ref(&r) == cible)
                && code.is_none_or(|c| {
                    texte(a, "codefou_principal").is_some_and(|x| x.eq_ignore_ascii_case(c))
                });
            let ok = pref || {
                let Some(no_id) = id(a, "no_id") else {
                    continue;
                };
                let d = s.obtenir(&format!("/articles/{no_id}"), &[]).await?;
                liste(&d, "fournisseurs").iter().any(|f| {
                    texte(f, "reference").is_some_and(|r| normaliser_ref(&r) == cible)
                        && code.is_none_or(|c| {
                            texte(f, "codefou").is_some_and(|x| x.eq_ignore_ascii_case(c))
                        })
                })
            };
            if ok && let Some(art) = article("reference", a) {
                let actif = a.get("suspendu").is_none_or(Value::is_null);
                trouves.push((actif, art));
            }
        }
        // Plusieurs fiches pour une même référence : la première active.
        trouves.sort_by_key(|(actif, _)| !*actif);
        if let Some((_, a)) = trouves.into_iter().next() {
            return Ok(Some(a));
        }
    }
    Ok(None)
}

/// Composants d'un lot (vide si l'article n'en est pas un).
async fn composants<S: Source>(
    s: &S,
    no_id: &str,
    codes: &[String],
    cache: &mut HashMap<String, Value>,
) -> Result<Vec<Composant>, String> {
    let v = s.obtenir(&format!("/appro/article/{no_id}"), &[]).await?;
    let mut r = Vec::new();
    for l in liste(&v, "lots") {
        let Some(cid) = id(l, "art_lot_noid") else {
            continue;
        };
        let f = match cache.get(&cid) {
            Some(f) => f.clone(),
            None => {
                let f = s
                    .obtenir(&format!("/articles/{cid}/referentiel"), &[])
                    .await?;
                cache.insert(cid.clone(), f.clone());
                f
            }
        };
        let a = f.get("article").cloned().unwrap_or(Value::Null);
        let gtins = liste(&f, "gtins");
        let gencod = gtins
            .iter()
            .find(|g| g.get("preferentiel").and_then(entier) == Some(1))
            .or_else(|| gtins.first())
            .and_then(|g| texte(g, "gtin"));
        let fous = liste(&f, "fournisseurs");
        let fou = fous
            .iter()
            .find(|x| {
                texte(x, "codefou")
                    .is_some_and(|c| codes.iter().any(|k| k.eq_ignore_ascii_case(&c)))
            })
            .or_else(|| {
                fous.iter()
                    .find(|x| x.get("preference").and_then(entier) == Some(1))
            })
            .or_else(|| fous.first());
        let pv = f
            .get("prix")
            .map(|p| liste(p, "vente_par_site"))
            .and_then(|l| {
                l.iter()
                    .find(|x| texte(x, "site").as_deref() == Some("000"))
                    .or(l.first())
            })
            .and_then(|x| nombre(x, "pv"));
        r.push(Composant {
            no_id: cid,
            codein: texte(l, "codein").or_else(|| texte(&a, "codein")),
            libelle: texte(&a, "libelle1").or_else(|| texte(l, "libelle1")),
            lib_ticket: texte(&a, "lib_ticket"),
            gencod,
            reference: fou.and_then(|x| texte(x, "ref_fou").or_else(|| texte(x, "reference"))),
            quantite: nombre(l, "quantite").filter(|q| *q > 0.0).unwrap_or(1.0),
            pa: f
                .get("prix")
                .and_then(|p| nombre(p, "achat"))
                .or_else(|| fou.and_then(|x| nombre(x, "prixachat"))),
            pv,
            pv_conseille: nombre(&a, "pv_conseille"),
            eco_ht: nombre(&a, "eco_ht").filter(|e| *e > 0.0),
            famille: texte(&a, "nom_code"),
            pourc_marge: nombre(l, "pourc_marge"),
        });
    }
    Ok(r)
}

/// Article d'une ligne : par gencod, sinon par référence ; avec ses composants si c'est un lot.
pub async fn chercher<S: Source>(
    s: &S,
    r: &Recherche,
    codes: &[String],
    cache: &mut HashMap<String, Value>,
) -> Result<Option<ArticleMagasin>, String> {
    let mut a = None;
    if let Some(g) = r.gencod.as_deref().and_then(gencod_propre) {
        a = par_gencod(s, &g).await?;
    }
    if a.is_none()
        && let Some(re) = r
            .reference
            .as_deref()
            .map(str::trim)
            .filter(|x| !x.is_empty())
    {
        a = par_reference(s, re, codes).await?;
    }
    if let Some(art) = a.as_mut() {
        art.composants = composants(s, &art.no_id, codes, cache).await?;
    }
    Ok(a)
}

/// Codes BO Phénix du fournisseur d'un document : sa fiche, son groupe, puis ses marques.
pub async fn codes_du_document(pool: &PgPool, document_id: i64) -> sqlx::Result<Vec<String>> {
    let r = sqlx::query!(
        r#"select f.code_gestion as "code?", g.code_gestion as "code_groupe?",
                  coalesce(array(select m.code_gestion from fournisseur m
                                 where m.groupe_id = f.id and m.code_gestion is not null
                                 order by m.nom), '{}') as "marques!"
           from document d
           left join fournisseur f on f.id = d.fournisseur_id
           left join fournisseur g on g.id = f.groupe_id
           where d.id = $1"#,
        document_id
    )
    .fetch_optional(pool)
    .await?;
    let Some(r) = r else {
        return Ok(Vec::new());
    };
    let mut tous = Vec::new();
    for c in [r.code.as_deref(), r.code_groupe.as_deref()]
        .into_iter()
        .chain(r.marques.iter().map(|m| Some(m.as_str())))
    {
        for k in codes(c) {
            if !tous.contains(&k) {
                tous.push(k);
            }
        }
    }
    Ok(tous)
}

/// Note la demande et pousse la recherche (file `envoi`).
pub async fn demander(pool: &PgPool, document_id: i64) -> anyhow::Result<()> {
    sqlx::query!(
        "update document set magasin_demande_le = now() where id = $1",
        document_id
    )
    .execute(pool)
    .await?;
    file::pousser_envoi(pool, file::TacheEnvoi::VerifierArticlesBl { document_id }).await
}

/// Cherche chaque ligne du BL et note le résultat (ligne par ligne, puis sur le document).
pub async fn verifier_document<S: Source>(
    pool: &PgPool,
    s: &S,
    document_id: i64,
) -> anyhow::Result<()> {
    let codes = codes_du_document(pool, document_id).await?;
    let lignes = sqlx::query!(
        "select id, gencod, reference from ligne_document where document_id = $1 order by rang",
        document_id
    )
    .fetch_all(pool)
    .await?;
    let mut cache = HashMap::new();
    let mut resultats = Vec::with_capacity(lignes.len());
    let mut erreur = None;
    for l in &lignes {
        let r = Recherche {
            gencod: l.gencod.clone(),
            reference: l.reference.clone(),
        };
        match chercher(s, &r, &codes, &mut cache).await {
            Ok(a) => resultats.push((l.id, a)),
            Err(m) => {
                erreur = Some(m);
                break;
            }
        }
    }
    let mut tx = pool.begin().await?;
    if erreur.is_none() {
        for (id, a) in &resultats {
            let v = a.as_ref().map(serde_json::to_value).transpose()?;
            sqlx::query!(
                "update ligne_document set magasin = $2 where id = $1",
                id,
                v
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query!(
        "update document set magasin_verifie_le = now(), magasin_erreur = $2 where id = $1",
        document_id,
        erreur
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
