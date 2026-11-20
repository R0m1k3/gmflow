//! Réglages Claude par tâche (modèle, effort) et protection du quota de lecture, dans
//! `parametre` (clé `claude_reglages`). Les valeurs absentes prennent les défauts du cahier.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgExecutor;

use crate::parametre;

pub const CLE: &str = "claude_reglages";

pub const MODELES: &[&str] = &["haiku", "sonnet", "opus"];
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Tâches de lecture (comptées dans le quota du jour).
pub const EXTRACTION_TEXTE: &str = "extraction_texte";
pub const OCR_SCAN: &str = "ocr_scan";
pub const NOUVEL_ESSAI: &str = "nouvel_essai";
pub const IDENTIFICATION_FOURNISSEUR: &str = "identification_fournisseur";
pub const TACHES_LECTURE: &[&str] = &[
    EXTRACTION_TEXTE,
    OCR_SCAN,
    NOUVEL_ESSAI,
    IDENTIFICATION_FOURNISSEUR,
];

/// Tri des mails (quota du jour à part, pour ne pas prendre celui de la lecture).
pub const TRI_MAIL: &str = "tri_mail";
pub const TACHES_TRI: &[&str] = &[TRI_MAIL];
/// Brouillon de réponse, à la demande seulement (pas de quota : chaque appel suit un clic).
pub const BROUILLON_REPONSE: &str = "brouillon_reponse";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reglage {
    pub modele: String,
    pub effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reglages {
    pub extraction_texte: Reglage,
    pub ocr_scan: Reglage,
    pub nouvel_essai: Reglage,
    #[serde(default = "defaut_identification")]
    pub identification_fournisseur: Reglage,
    #[serde(default = "defaut_tri")]
    pub tri_mail: Reglage,
    #[serde(default = "defaut_brouillon")]
    pub brouillon_reponse: Reglage,
    /// Lecture automatique des pièces jointes reçues.
    pub lecture_auto: bool,
    /// Nombre maximal d'appels de lecture par jour (UTC).
    pub quota_lecture_jour: i64,
    /// Tri automatique des mails reçus.
    #[serde(default = "vrai")]
    pub tri_auto: bool,
    /// Nombre maximal d'appels de tri par jour (UTC).
    #[serde(default = "defaut_quota_tri")]
    pub quota_tri_jour: i64,
    /// Seuls les mails des N derniers jours sont triés (les plus anciens restent sans catégorie).
    #[serde(default = "defaut_tri_depuis")]
    pub tri_depuis_jours: i64,
}

fn defaut_identification() -> Reglage {
    r("haiku", "low")
}

fn defaut_tri() -> Reglage {
    r("haiku", "low")
}

fn defaut_brouillon() -> Reglage {
    r("sonnet", "low")
}

fn vrai() -> bool {
    true
}

fn defaut_quota_tri() -> i64 {
    200
}

fn defaut_tri_depuis() -> i64 {
    30
}

fn r(modele: &str, effort: &str) -> Reglage {
    Reglage {
        modele: modele.into(),
        effort: effort.into(),
    }
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            extraction_texte: r("haiku", "low"),
            ocr_scan: r("haiku", "medium"),
            nouvel_essai: r("sonnet", "medium"),
            identification_fournisseur: defaut_identification(),
            tri_mail: defaut_tri(),
            brouillon_reponse: defaut_brouillon(),
            lecture_auto: true,
            quota_lecture_jour: 100,
            tri_auto: true,
            quota_tri_jour: defaut_quota_tri(),
            tri_depuis_jours: defaut_tri_depuis(),
        }
    }
}

impl Reglages {
    pub fn pour(&self, tache: &str) -> &Reglage {
        match tache {
            OCR_SCAN => &self.ocr_scan,
            NOUVEL_ESSAI => &self.nouvel_essai,
            IDENTIFICATION_FOURNISSEUR => &self.identification_fournisseur,
            TRI_MAIL => &self.tri_mail,
            BROUILLON_REPONSE => &self.brouillon_reponse,
            _ => &self.extraction_texte,
        }
    }

    /// Message d'erreur si une valeur est hors des listes connues.
    pub fn controler(&self) -> Result<(), String> {
        for (nom, r) in [
            (EXTRACTION_TEXTE, &self.extraction_texte),
            (OCR_SCAN, &self.ocr_scan),
            (NOUVEL_ESSAI, &self.nouvel_essai),
            (IDENTIFICATION_FOURNISSEUR, &self.identification_fournisseur),
            (TRI_MAIL, &self.tri_mail),
            (BROUILLON_REPONSE, &self.brouillon_reponse),
        ] {
            if !MODELES.contains(&r.modele.as_str()) {
                return Err(format!("{nom} : modèle « {} » inconnu", r.modele));
            }
            if !EFFORTS.contains(&r.effort.as_str()) {
                return Err(format!("{nom} : effort « {} » inconnu", r.effort));
            }
        }
        if !(0..=10_000).contains(&self.quota_lecture_jour) {
            return Err("le quota doit être entre 0 et 10 000".into());
        }
        if !(0..=10_000).contains(&self.quota_tri_jour) {
            return Err("le quota de tri doit être entre 0 et 10 000".into());
        }
        if !(1..=365).contains(&self.tri_depuis_jours) {
            return Err("la période de tri doit être entre 1 et 365 jours".into());
        }
        Ok(())
    }
}

/// Fusionne la valeur stockée avec les défauts (une clé absente garde son défaut).
fn fusionner(base: &mut Value, stocke: &Value) {
    if let (Some(b), Some(s)) = (base.as_object_mut(), stocke.as_object()) {
        for (k, v) in s {
            match b.get_mut(k) {
                Some(bv) if bv.is_object() && v.is_object() => fusionner(bv, v),
                Some(bv) => *bv = v.clone(),
                None => {}
            }
        }
    }
}

pub async fn lire<'e>(db: impl PgExecutor<'e>) -> sqlx::Result<Reglages> {
    let defaut = Reglages::default();
    let Some(stocke) = parametre::lire(db, CLE).await? else {
        return Ok(defaut);
    };
    let mut v = serde_json::to_value(&defaut).unwrap_or_default();
    fusionner(&mut v, &stocke);
    Ok(serde_json::from_value(v).unwrap_or(defaut))
}

pub async fn ecrire<'e>(db: impl PgExecutor<'e>, r: &Reglages) -> sqlx::Result<()> {
    parametre::ecrire(db, CLE, &serde_json::to_value(r).unwrap_or_default()).await
}

/// Appels de lecture faits aujourd'hui (UTC), comptés dans le quota.
pub async fn appels_lecture_du_jour<'e>(db: impl PgExecutor<'e>) -> sqlx::Result<i64> {
    let taches: Vec<String> = TACHES_LECTURE.iter().map(|t| t.to_string()).collect();
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from appel_claude
           where quand >= date_trunc('day', now(), 'UTC') and tache = any($1)"#,
        &taches
    )
    .fetch_one(db)
    .await
}

/// Appels de tri faits aujourd'hui (UTC), comptés dans le quota de tri.
pub async fn appels_tri_du_jour<'e>(db: impl PgExecutor<'e>) -> sqlx::Result<i64> {
    let taches: Vec<String> = TACHES_TRI.iter().map(|t| t.to_string()).collect();
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from appel_claude
           where quand >= date_trunc('day', now(), 'UTC') and tache = any($1)"#,
        &taches
    )
    .fetch_one(db)
    .await
}
