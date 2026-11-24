//! Logiflow (API externe `/api/ext/v1`) : adresse (en clair) et clé d'API (chiffrée), essai de
//! connexion, recherche des livraisons d'un BL et écriture de la facture sur la livraison
//! (référence, HT, TTC), comme la page Rapprochement de Logiflow. Les appels réseau partent
//! toujours du worker ; la clé n'est jamais renvoyée par l'API de GMFlow.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::crypto::Chiffreur;
use crate::fournisseurs::{meme_nom, normaliser};
use crate::parametre;
pub use crate::serveur_api::{ServeurApi, controler_adresse, fin};

/// `valeur` = [`ServeurApi`], `valeur_chiffree` = clé.
pub const CLE: &str = "logiflow";
/// Dernier essai : `{"quand", "ok", "message"}`.
pub const CLE_TEST: &str = "logiflow_test";

/// Racine de l'API externe : `https://logiflow.x`, `…/api` et `…/api/ext/v1` donnent
/// `…/api/ext/v1`.
pub fn racine(adresse: &str) -> String {
    let a = adresse.trim().trim_end_matches('/');
    let a = a.strip_suffix("/api/ext/v1").unwrap_or(a);
    let a = a.strip_suffix("/api").unwrap_or(a);
    format!("{a}/api/ext/v1")
}

/// Adresse et clé déchiffrée, si l'adresse est configurée.
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

/// Client de l'API externe de Logiflow.
pub struct Client {
    http: reqwest::Client,
    racine: String,
    jeton: Option<String>,
}

fn source_la_plus_profonde(e: &(dyn std::error::Error + 'static)) -> String {
    let mut s = e;
    while let Some(suivante) = s.source() {
        s = suivante;
    }
    s.to_string()
}

impl Client {
    pub fn new(adresse: &str, jeton: Option<&str>) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("GMFlow")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            racine: racine(adresse),
            jeton: jeton.map(str::to_string),
        })
    }

    /// Appel authentifié (clé en `X-API-Key`) ; erreur en français.
    async fn appeler(&self, req: reqwest::RequestBuilder) -> Result<Value, String> {
        let req = match &self.jeton {
            Some(j) => req.header("X-API-Key", j),
            None => req,
        };
        let r = req.send().await.map_err(|e| {
            if e.is_timeout() {
                "Logiflow ne répond pas".to_string()
            } else if e.is_connect() {
                format!("connexion impossible : {}", source_la_plus_profonde(&e))
            } else {
                source_la_plus_profonde(&e)
            }
        })?;
        let s = r.status().as_u16();
        if (200..300).contains(&s) {
            return r
                .json::<Value>()
                .await
                .map_err(|_| "réponse illisible (JSON attendu)".to_string());
        }
        // Logiflow détaille ses refus dans `message` ou `error`.
        let detail = r
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| {
                v.get("message")
                    .or_else(|| v.get("error"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .map(|m| format!(" : {m}"))
            .unwrap_or_default();
        Err(match s {
            401 => "clé d'API refusée (HTTP 401) : clé fausse, révoquée ou expirée".into(),
            403 => "clé d'API en lecture seule (HTTP 403) : il faut une clé « lecture + écriture »".into(),
            404 => format!("introuvable (HTTP 404){detail}"),
            409 => format!("refusé par Logiflow (HTTP 409){detail}"),
            429 => "trop de requêtes (HTTP 429), réessayer dans une minute".into(),
            503 => "API externe désactivée dans Logiflow (HTTP 503) : créer une clé dans Paramètres > API externe".into(),
            c => format!("Logiflow a répondu HTTP {c}{detail}"),
        })
    }

    async fn obtenir(&self, chemin: &str, requete: &[(&str, String)]) -> Result<Value, String> {
        let url = format!("{}{chemin}", self.racine);
        self.appeler(self.http.get(url).query(requete)).await
    }

    /// Essai de connexion : la liste des magasins visibles par la clé.
    pub async fn essayer(&self) -> Result<String, String> {
        let v = self.obtenir("/stores", &[]).await?;
        let Some(magasins) = v.as_array() else {
            return Err(
                "Logiflow répond, mais pas comme son API externe (liste des magasins attendue)"
                    .into(),
            );
        };
        let noms: Vec<&str> = magasins
            .iter()
            .filter_map(|m| m.get("name").and_then(Value::as_str))
            .collect();
        Ok(match noms.len() {
            0 => "connexion réussie, mais aucun magasin visible avec cette clé".into(),
            n => format!(
                "connexion réussie : {n} magasin{} ({})",
                if n > 1 { "s" } else { "" },
                noms.join(", ")
            ),
        })
    }

    /// Fournisseurs de Logiflow : `(id, code, nom)`.
    pub async fn fournisseurs(&self) -> Result<Vec<(i64, Option<String>, String)>, String> {
        let v = self.obtenir("/suppliers", &[]).await?;
        Ok(v.as_array()
            .map(|l| {
                l.iter()
                    .filter_map(|f| {
                        Some((
                            f.get("id")?.as_i64()?,
                            f.get("code")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|c| !c.is_empty())
                                .map(str::to_string),
                            f.get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Livraisons livrées dont le n° de BL est exactement `bl` (casse et espaces ignorés).
    pub async fn livraisons_du_bl(&self, bl: &str) -> Result<Vec<Livraison>, String> {
        let v = self
            .obtenir(
                "/deliveries",
                &[("blNumber", bl.to_string()), ("limit", "50".into())],
            )
            .await?;
        Ok(livraisons(&v))
    }

    /// Livraisons livrées d'un fournisseur, encore sans facture (500 au plus, les plus récentes).
    pub async fn livraisons_sans_facture(
        &self,
        fournisseur: i64,
    ) -> Result<Vec<Livraison>, String> {
        let v = self
            .obtenir(
                "/deliveries",
                &[
                    ("supplierId", fournisseur.to_string()),
                    ("hasInvoice", "false".into()),
                    ("limit", "500".into()),
                ],
            )
            .await?;
        Ok(livraisons(&v))
    }

    /// Écrit la facture sur la livraison ; rend la livraison mise à jour.
    pub async fn ecrire_facture(&self, livraison: i64, corps: &Value) -> Result<Livraison, String> {
        let url = format!("{}/deliveries/{livraison}", self.racine);
        let v = self.appeler(self.http.patch(url).json(corps)).await?;
        Livraison::lire(&v).ok_or_else(|| "réponse illisible après l'écriture".to_string())
    }
}

/// Une livraison de Logiflow, réduite à ce qui sert au rapprochement.
#[derive(Debug, Clone, PartialEq)]
pub struct Livraison {
    pub id: i64,
    pub magasin: Option<String>,
    pub fournisseur_id: Option<i64>,
    pub fournisseur: Option<String>,
    pub code_fournisseur: Option<String>,
    pub bl: Option<String>,
    pub facture: Option<String>,
    pub rapprochee: bool,
    pub automatique: bool,
}

impl Livraison {
    fn lire(v: &Value) -> Option<Self> {
        let t = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Some(Self {
            id: v.get("id")?.as_i64()?,
            magasin: t("storeName"),
            fournisseur_id: v.get("supplierId").and_then(Value::as_i64),
            fournisseur: t("supplierName"),
            code_fournisseur: t("supplierCode"),
            bl: t("blNumber"),
            facture: t("invoiceReference"),
            rapprochee: v.get("reconciled").and_then(Value::as_bool) == Some(true),
            automatique: v.get("automaticReconciliation").and_then(Value::as_bool) == Some(true),
        })
    }
}

fn livraisons(v: &Value) -> Vec<Livraison> {
    v.get("items")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Livraison::lire).collect())
        .unwrap_or_default()
}

// ---------- Choix de la livraison (pur) ----------

/// N° de BL comparable : lettres et chiffres en majuscules, sans préfixe « BL » ni zéros de tête
/// (« BL-004567 », « bl 4567 » et « 4567 » se valent).
pub fn bl_comparable(bl: &str) -> String {
    let t: String = bl
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase();
    let t = t.strip_prefix("BL").unwrap_or(&t);
    let t = t.trim_start_matches('0');
    t.to_string()
}

/// Ce que GMFlow sait de la facture pour reconnaître ses livraisons.
#[derive(Debug, Default)]
pub struct Reperes {
    /// Codes BO Phénix du fournisseur, de son groupe et de ses marques.
    pub codes: Vec<String>,
    /// Noms de la fiche fournisseur et du fournisseur lu sur la facture.
    pub noms: Vec<String>,
    /// Raison sociale et autres noms de la société (magasin) de la facture.
    pub societe: Vec<String>,
}

impl Reperes {
    fn meme_fournisseur(&self, l: &Livraison) -> Option<bool> {
        if let Some(c) = &l.code_fournisseur
            && !self.codes.is_empty()
        {
            return Some(self.codes.iter().any(|x| x.eq_ignore_ascii_case(c)));
        }
        let nom = l.fournisseur.as_deref()?;
        if self.noms.is_empty() {
            return None;
        }
        Some(self.noms.iter().any(|n| meme_nom(n, nom)))
    }

    fn meme_magasin(&self, l: &Livraison) -> bool {
        let Some(m) = l.magasin.as_deref() else {
            return false;
        };
        let m = normaliser(m);
        self.societe.iter().any(|s| {
            let s = normaliser(s);
            !s.is_empty()
                && (m.contains(&s)
                    || s.contains(&m)
                    || s.split_whitespace()
                        .any(|mot| mot.len() > 3 && m.split_whitespace().any(|x| x == mot)))
        })
    }
}

/// Résultat du choix pour un n° de BL.
#[derive(Debug, PartialEq)]
pub enum Choix {
    Une(Livraison),
    Aucune,
    /// Plusieurs livraisons possibles : rien n'est écrit.
    Plusieurs(usize),
}

/// Parmi les livraisons d'un n° de BL, celle de la facture : même fournisseur (code BO Phénix,
/// sinon nom), puis même magasin s'il en reste plusieurs. Un fournisseur différent écarte la
/// livraison ; un fournisseur inconnu des deux côtés n'est accepté que s'il est seul.
pub fn choisir(candidates: Vec<Livraison>, r: &Reperes) -> Choix {
    let mut sures = Vec::new();
    let mut douteuses = Vec::new();
    for l in candidates {
        match r.meme_fournisseur(&l) {
            Some(true) => sures.push(l),
            Some(false) => {}
            None => douteuses.push(l),
        }
    }
    let liste = if sures.is_empty() { douteuses } else { sures };
    if liste.len() <= 1 {
        return liste.into_iter().next().map_or(Choix::Aucune, Choix::Une);
    }
    let n = liste.len();
    let mut meme: Vec<Livraison> = liste.into_iter().filter(|l| r.meme_magasin(l)).collect();
    if meme.len() == 1 {
        Choix::Une(meme.remove(0))
    } else {
        Choix::Plusieurs(n)
    }
}

/// Corps du `PATCH` : référence toujours ; HT et TTC seulement quand la facture ne couvre qu'une
/// livraison (sinon le montant de la facture serait compté sur chaque BL). `reconciled` n'est
/// jamais envoyé : la validation reste faite dans Logiflow.
pub fn corps_facture(numero: &str, ht: Option<f64>, ttc: Option<f64>, une_seule: bool) -> Value {
    let mut c = json!({ "invoiceReference": numero });
    if une_seule {
        if let Some(h) = ht {
            c["invoiceAmount"] = json!(h);
        }
        if let Some(t) = ttc {
            c["invoiceAmountTTC"] = json!(t);
        }
    }
    c
}

/// Ligne du résultat, par n° de BL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LigneResultat {
    pub bl: String,
    pub livraison_id: Option<i64>,
    pub magasin: Option<String>,
    /// complete, deja_fait, deja_rapproche, autre_facture, introuvable, ambigu, erreur.
    pub etat: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liv(id: i64, code: Option<&str>, nom: &str, magasin: &str) -> Livraison {
        Livraison {
            id,
            magasin: Some(magasin.into()),
            fournisseur_id: Some(id * 10),
            fournisseur: Some(nom.into()),
            code_fournisseur: code.map(str::to_string),
            bl: Some("BL1".into()),
            facture: None,
            rapprochee: false,
            automatique: false,
        }
    }

    #[test]
    fn racines() {
        for a in [
            "https://l.x",
            "https://l.x/",
            "https://l.x/api",
            "https://l.x/api/ext/v1/",
        ] {
            assert_eq!(racine(a), "https://l.x/api/ext/v1", "{a}");
        }
    }

    #[test]
    fn bl_comparables() {
        assert_eq!(bl_comparable("BL-004567"), "4567");
        assert_eq!(bl_comparable("bl 4567"), "4567");
        assert_eq!(bl_comparable("4567"), "4567");
        assert_eq!(bl_comparable("A-12"), "A12");
    }

    #[test]
    fn choix() {
        let r = Reperes {
            codes: vec!["J009".into()],
            noms: vec!["JJA".into()],
            societe: vec![
                "SAS Frouard Distribution".into(),
                "La Foir'Fouille Frouard".into(),
            ],
        };
        // Autre fournisseur écarté, même code gardé.
        let c = choisir(
            vec![
                liv(1, Some("L0091"), "L3C", "Frouard"),
                liv(2, Some("j009"), "JJA", "Frouard"),
            ],
            &r,
        );
        assert!(matches!(c, Choix::Une(l) if l.id == 2));
        // Deux magasins : celui de la société.
        let c = choisir(
            vec![
                liv(1, Some("J009"), "JJA", "Houdemont"),
                liv(2, Some("J009"), "JJA", "Magasin Frouard"),
            ],
            &r,
        );
        assert!(matches!(c, Choix::Une(l) if l.id == 2));
        // Deux livraisons dans le même magasin : ambigu.
        let c = choisir(
            vec![
                liv(1, Some("J009"), "JJA", "Frouard"),
                liv(2, Some("J009"), "JJA", "Frouard"),
            ],
            &r,
        );
        assert_eq!(c, Choix::Plusieurs(2));
        // Rien du même fournisseur.
        assert_eq!(
            choisir(vec![liv(1, Some("X1"), "Autre", "Frouard")], &r),
            Choix::Aucune
        );
        // Sans code côté GMFlow : par le nom.
        let r2 = Reperes {
            codes: vec![],
            noms: vec!["JJA SA".into()],
            societe: vec![],
        };
        let c = choisir(
            vec![
                liv(1, Some("X1"), "Autre", "F"),
                liv(2, Some("J009"), "JJA", "F"),
            ],
            &r2,
        );
        assert!(matches!(c, Choix::Une(l) if l.id == 2));
    }

    #[test]
    fn corps() {
        assert_eq!(
            corps_facture("F1", Some(10.0), Some(12.0), true),
            json!({"invoiceReference": "F1", "invoiceAmount": 10.0, "invoiceAmountTTC": 12.0})
        );
        assert_eq!(
            corps_facture("F1", Some(10.0), Some(12.0), false),
            json!({"invoiceReference": "F1"})
        );
    }
}
