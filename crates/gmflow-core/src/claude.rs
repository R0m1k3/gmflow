//! Appels à Claude CLI. Le jeton n'est passé qu'au processus enfant ; chaque appel a son propre
//! `HOME` temporaire. Jamais `--bare` : ce mode ignore le jeton OAuth.

use std::time::{Duration, Instant};

use serde_json::Value;
use sqlx::PgPool;

pub const DELAI: Duration = Duration::from_secs(120);
/// Lecture d'un scan : Claude ouvre le fichier, c'est plus long.
pub const DELAI_LECTURE: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum ErreurClaude {
    #[error("délai de {0} s dépassé")]
    Delai(u64),
    #[error("lancement de Claude CLI impossible : {0}")]
    Lancement(#[from] std::io::Error),
    #[error("Claude CLI a échoué (code {code:?}) : {stderr}")]
    Sortie { code: Option<i32>, stderr: String },
    #[error("réponse JSON illisible")]
    JsonInvalide,
    #[error("erreur du modèle : {0}")]
    Modele(String),
}

#[derive(Debug)]
pub struct SortieClaude {
    pub json: Value,
    pub duree_ms: u64,
}

/// Dossier temporaire supprimé à la fin de la portée.
pub struct DossierTemp(pub std::path::PathBuf);

impl DossierTemp {
    pub fn creer(prefixe: &str) -> std::io::Result<Self> {
        let p = std::env::temp_dir().join(format!("{prefixe}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p)?;
        Ok(Self(p))
    }
}

impl Drop for DossierTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tronquer(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Un appel à Claude CLI. `dossier` : dossier de travail (fichier à lire), sinon le `HOME`
/// temporaire.
pub struct Appel<'a> {
    pub tache: &'a str,
    pub modele: &'a str,
    pub effort: Option<&'a str>,
    pub prompt: &'a str,
    pub args_en_plus: &'a [&'a str],
    pub jeton: &'a str,
    /// `jeton` est une clé API Anthropic (`ANTHROPIC_API_KEY`) et non un setup token.
    pub cle_api: bool,
    pub dossier: Option<&'a std::path::Path>,
    pub delai: Duration,
    /// Document lu ou rattaché par cet appel, gardé dans `appel_claude` pour la mesure du coût
    /// et de la qualité de la lecture.
    pub document_id: Option<i64>,
}

impl<'a> Appel<'a> {
    pub fn new(tache: &'a str, modele: &'a str, prompt: &'a str, jeton: &'a str) -> Self {
        Self {
            tache,
            modele,
            effort: None,
            prompt,
            args_en_plus: &[],
            jeton,
            cle_api: false,
            dossier: None,
            delai: DELAI,
            document_id: None,
        }
    }

    /// Appel avec l'accès configuré (setup token ou clé API).
    pub fn avec_acces(tache: &'a str, modele: &'a str, prompt: &'a str, acces: &'a Acces) -> Self {
        let mut a = Self::new(tache, modele, prompt, &acces.secret);
        a.cle_api = acces.cle_api;
        a
    }
}

/// Secret utilisé pour appeler Claude : setup token (abonnement) ou clé API Anthropic,
/// selon le mode choisi dans l'administration (`parametre.claude_acces`).
#[derive(Clone)]
pub struct Acces {
    pub secret: String,
    pub cle_api: bool,
}

impl std::fmt::Debug for Acces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Acces")
            .field("cle_api", &self.cle_api)
            .finish_non_exhaustive()
    }
}

/// Mode d'accès choisi : `jeton` (défaut) ou `cle_api`.
pub async fn mode_acces(pool: &PgPool) -> sqlx::Result<String> {
    Ok(crate::parametre::lire(pool, crate::parametre::CLAUDE_ACCES)
        .await?
        .and_then(|v| v.get("mode").and_then(Value::as_str).map(str::to_string))
        .filter(|m| m == "cle_api")
        .unwrap_or_else(|| "jeton".into()))
}

fn cle_secret(mode: &str) -> &'static str {
    if mode == "cle_api" {
        crate::parametre::CLAUDE_CLE_API
    } else {
        crate::parametre::CLAUDE_JETON
    }
}

/// Un secret est-il enregistré pour le mode choisi ?
pub async fn acces_configure(pool: &PgPool) -> sqlx::Result<bool> {
    let mode = mode_acces(pool).await?;
    Ok(crate::parametre::lire_secret(pool, cle_secret(&mode))
        .await?
        .is_some())
}

/// Le secret du mode choisi, déchiffré.
pub async fn acces(
    pool: &PgPool,
    chiffreur: &crate::crypto::Chiffreur,
) -> anyhow::Result<Option<Acces>> {
    let mode = mode_acces(pool).await?;
    let Some(chiffre) = crate::parametre::lire_secret(pool, cle_secret(&mode)).await? else {
        return Ok(None);
    };
    Ok(Some(Acces {
        secret: chiffreur.dechiffrer_texte(&chiffre)?,
        cle_api: mode == "cle_api",
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn appeler(
    pool: &PgPool,
    claude_bin: &str,
    tache: &str,
    modele: &str,
    effort: Option<&str>,
    prompt: &str,
    args_en_plus: &[&str],
    jeton: &str,
) -> Result<SortieClaude, ErreurClaude> {
    let mut a = Appel::new(tache, modele, prompt, jeton);
    a.effort = effort;
    a.args_en_plus = args_en_plus;
    lancer(pool, claude_bin, &a).await
}

pub async fn lancer(
    pool: &PgPool,
    claude_bin: &str,
    a: &Appel<'_>,
) -> Result<SortieClaude, ErreurClaude> {
    let (tache, modele, effort) = (a.tache, a.modele, a.effort);
    let debut = Instant::now();
    let mut brut = None;
    let resultat = executer(claude_bin, a, &mut brut).await;
    let duree_ms = debut.elapsed().as_millis() as u64;
    let (succes, message) = match &resultat {
        Ok(_) => (true, None),
        Err(e) => (false, Some(tronquer(&e.to_string(), 500))),
    };
    match &resultat {
        Ok(_) => crate::attente::noter_succes(pool).await,
        Err(e) => {
            if let Some(r) = refus(e) {
                crate::attente::noter_refus(pool, &r, &e.to_string()).await;
            }
        }
    }
    let conso = consommation(resultat.as_ref().ok().or(brut.as_ref()));
    if let Err(e) = sqlx::query!(
        "insert into appel_claude (tache, modele, effort, duree_ms, succes, message, document_id,
                                   tokens_entree, tokens_sortie, tokens_cache_lecture,
                                   tokens_cache_ecriture, cout_usd)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::float8::numeric)",
        tache,
        modele,
        effort,
        duree_ms.min(i32::MAX as u64) as i32,
        succes,
        message,
        a.document_id,
        conso.tokens_entree,
        conso.tokens_sortie,
        conso.tokens_cache_lecture,
        conso.tokens_cache_ecriture,
        conso.cout_usd
    )
    .execute(pool)
    .await
    {
        tracing::warn!(erreur = %e, "écriture de appel_claude impossible");
    }
    resultat.map(|json| SortieClaude { json, duree_ms })
}

/// Consommation d'un appel lue dans la sortie JSON du CLI (`usage`, `total_cost_usd`). Un champ
/// absent ou d'un autre type reste vide : la mesure ne fait jamais échouer un appel.
#[derive(Debug, Default, PartialEq)]
pub struct Consommation {
    pub tokens_entree: Option<i32>,
    pub tokens_sortie: Option<i32>,
    pub tokens_cache_lecture: Option<i32>,
    pub tokens_cache_ecriture: Option<i32>,
    /// Coût estimé par le CLI, en dollars.
    pub cout_usd: Option<f64>,
}

pub fn consommation(json: Option<&Value>) -> Consommation {
    let Some(j) = json else {
        return Consommation::default();
    };
    let tokens = |champ: &str| {
        j.get("usage")
            .and_then(|u| u.get(champ))
            .and_then(Value::as_u64)
            .map(|n| n.min(i32::MAX as u64) as i32)
    };
    Consommation {
        tokens_entree: tokens("input_tokens"),
        tokens_sortie: tokens("output_tokens"),
        tokens_cache_lecture: tokens("cache_read_input_tokens"),
        tokens_cache_ecriture: tokens("cache_creation_input_tokens"),
        cout_usd: j
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .filter(|c| c.is_finite() && *c >= 0.0 && *c < 1e6),
    }
}

/// `brut` : la sortie JSON du CLI quand elle a pu être lue, même en erreur (sa consommation est
/// notée aussi).
async fn executer(
    claude_bin: &str,
    a: &Appel<'_>,
    brut: &mut Option<Value>,
) -> Result<Value, ErreurClaude> {
    let home = DossierTemp::creer("gmflow-claude")?;
    let mut cmd = tokio::process::Command::new(claude_bin);
    cmd.arg("-p").arg(a.prompt).arg("--model").arg(a.modele);
    if let Some(e) = a.effort {
        cmd.arg("--effort").arg(e);
    }
    cmd.args(["--output-format", "json", "--no-session-persistence"])
        .args(a.args_en_plus)
        .current_dir(a.dossier.unwrap_or(&home.0))
        .env_clear()
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
        )
        .env("HOME", &home.0)
        .env("DISABLE_AUTOUPDATER", "1")
        .env(
            if a.cle_api {
                "ANTHROPIC_API_KEY"
            } else {
                "CLAUDE_CODE_OAUTH_TOKEN"
            },
            a.jeton,
        )
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let sortie = tokio::time::timeout(a.delai, cmd.output())
        .await
        .map_err(|_| ErreurClaude::Delai(a.delai.as_secs()))??;
    if !sortie.status.success() {
        // La sortie JSON peut contenir le message d'erreur même avec un code non nul.
        if let Ok(json) = serde_json::from_slice::<Value>(&sortie.stdout)
            && json.get("is_error").and_then(Value::as_bool) == Some(true)
        {
            let m = message_json(&json);
            *brut = Some(json);
            return Err(ErreurClaude::Modele(m));
        }
        return Err(ErreurClaude::Sortie {
            code: sortie.status.code(),
            stderr: tronquer(&String::from_utf8_lossy(&sortie.stderr), 500),
        });
    }
    let json: Value =
        serde_json::from_slice(&sortie.stdout).map_err(|_| ErreurClaude::JsonInvalide)?;
    if json.get("is_error").and_then(Value::as_bool) != Some(false) {
        let m = message_json(&json);
        *brut = Some(json);
        return Err(ErreurClaude::Modele(m));
    }
    Ok(json)
}

fn message_json(json: &Value) -> String {
    tronquer(
        json.get("result")
            .and_then(Value::as_str)
            .unwrap_or("réponse en erreur sans message"),
        500,
    )
}

/// Sortie structurée d'une réponse de Claude CLI (`--json-schema`) : `structured_output`, ou à
/// défaut le JSON écrit dans `result` (éventuellement entre balises de code).
pub fn sortie_structuree(reponse: &Value) -> Option<Value> {
    if let Some(v) = reponse.get("structured_output")
        && v.is_object()
    {
        return Some(v.clone());
    }
    let r = reponse.get("result")?.as_str()?.trim();
    let r = r
        .strip_prefix("```json")
        .or_else(|| r.strip_prefix("```"))
        .unwrap_or(r);
    let r = r.strip_suffix("```").unwrap_or(r).trim();
    serde_json::from_str::<Value>(r)
        .ok()
        .filter(Value::is_object)
}

/// Refus de Claude qui vaut pour tous les appels (pas seulement pour ce document) : limite
/// d'usage de l'abonnement ou jeton refusé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refus {
    pub genre: GenreRefus,
    /// Heure de reprise annoncée par le CLI, si elle est lisible.
    pub reprise: Option<Reprise>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreRefus {
    /// Limite d'usage, quota de l'API, crédit épuisé.
    Limite,
    /// Jeton ou clé refusés (expirés, révoqués, invalides).
    Jeton,
}

impl GenreRefus {
    pub fn code(self) -> &'static str {
        match self {
            GenreRefus::Limite => "limite",
            GenreRefus::Jeton => "jeton",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reprise {
    /// Instant exact (secondes Unix), format `usage limit reached|1736272800`.
    Instant(i64),
    /// Heure du jour (« resets 3pm », « resets 15:30 (Europe/Paris) ») : le prochain passage à
    /// cette heure, dans le fuseau donné (UTC sinon).
    Heure {
        heure: u8,
        minute: u8,
        fuseau: Option<String>,
    },
}

/// Le message d'erreur est-il un refus général de Claude ? Les pannes, délais et réponses
/// illisibles n'en sont pas : elles ne concernent que l'appel en cours.
pub fn refus(e: &ErreurClaude) -> Option<Refus> {
    let texte = match e {
        ErreurClaude::Modele(m) => m.as_str(),
        ErreurClaude::Sortie { stderr, .. } => stderr.as_str(),
        _ => return None,
    };
    refus_texte(texte)
}

pub fn refus_texte(texte: &str) -> Option<Refus> {
    let t = texte.to_lowercase();
    const LIMITE: &[&str] = &[
        "usage limit",
        "limit reached",
        "hit your limit",
        "rate limit",
        "rate_limit",
        "credit balance",
        "quota",
        "too many requests",
        "resets ",
        "error 429",
        "status 429",
        "429 ",
    ];
    const JETON: &[&str] = &[
        "invalid bearer token",
        "invalid api key",
        "invalid x-api-key",
        "authentication_error",
        "authentication failed",
        "oauth token",
        "token has expired",
        "token expired",
        "revoked",
        "unauthorized",
        "please run /login",
        "not logged in",
        "error 401",
        "status 401",
        "401 ",
    ];
    let genre = if LIMITE.iter().any(|m| t.contains(m)) {
        GenreRefus::Limite
    } else if JETON.iter().any(|m| t.contains(m)) {
        GenreRefus::Jeton
    } else {
        return None;
    };
    let reprise = (genre == GenreRefus::Limite)
        .then(|| reprise(texte))
        .flatten();
    Some(Refus { genre, reprise })
}

/// Heure de reprise dans un message du CLI : `…|1736272800`, « resets 3pm », « resets 3:30 pm »,
/// « resets 15:00 (Europe/Paris) », « resets at 9am ».
fn reprise(texte: &str) -> Option<Reprise> {
    if let Some((_, fin)) = texte.rsplit_once('|') {
        let chiffres: String = fin
            .trim()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if chiffres.len() >= 9
            && let Ok(n) = chiffres.parse::<i64>()
        {
            return Some(Reprise::Instant(n));
        }
    }
    let bas = texte.to_lowercase();
    let i = bas.find("reset")?;
    let reste = &texte[i..];
    let re = regex::Regex::new(
        r"(?i)resets?\s+(?:at\s+|on\s+)?(?:[a-z]{3,9}\.?\s+)?(?:\d{1,2}\s*,?\s+)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)?(?:\s*\(([A-Za-z_]+(?:/[A-Za-z_+\-]+)*)\))?",
    )
    .ok()?;
    let c = re.captures(reste)?;
    let mut heure: u8 = c.get(1)?.as_str().parse().ok()?;
    let minute: u8 = c.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
    match c.get(3).map(|m| m.as_str().to_lowercase()) {
        Some(ap) if ap == "pm" && heure < 12 => heure += 12,
        Some(ap) if ap == "am" && heure == 12 => heure = 0,
        Some(_) => {}
        // Ni am ni pm ni minutes : un chiffre seul n'est pas une heure (« resets in 5 »).
        None if c.get(2).is_none() => return None,
        None => {}
    }
    if heure > 23 || minute > 59 {
        return None;
    }
    Some(Reprise::Heure {
        heure,
        minute,
        fuseau: c.get(4).map(|m| m.as_str().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consommation_lue_du_json_du_cli_sans_jamais_echouer() {
        // Forme de la sortie de Claude CLI 2.1 (`--output-format json`).
        let j = serde_json::json!({
            "type": "result", "is_error": false, "duration_ms": 5321,
            "total_cost_usd": 0.0123456,
            "usage": { "input_tokens": 1200, "output_tokens": 340,
                       "cache_creation_input_tokens": 5000, "cache_read_input_tokens": 0 }
        });
        assert_eq!(
            consommation(Some(&j)),
            Consommation {
                tokens_entree: Some(1200),
                tokens_sortie: Some(340),
                tokens_cache_lecture: Some(0),
                tokens_cache_ecriture: Some(5000),
                cout_usd: Some(0.0123456),
            }
        );
        // Champs absents, mal typés ou aberrants : vides.
        let j = serde_json::json!({ "usage": { "input_tokens": "12" }, "total_cost_usd": -1 });
        assert_eq!(consommation(Some(&j)), Consommation::default());
        assert_eq!(consommation(None), Consommation::default());
    }

    fn heure(h: u8, m: u8, f: Option<&str>) -> Option<Reprise> {
        Some(Reprise::Heure {
            heure: h,
            minute: m,
            fuseau: f.map(str::to_string),
        })
    }

    #[test]
    fn refus_reconnus() {
        let r = refus_texte("You've hit your limit · resets 3pm (Europe/Paris)").unwrap();
        assert_eq!(r.genre, GenreRefus::Limite);
        assert_eq!(r.reprise, heure(15, 0, Some("Europe/Paris")));
        let r = refus_texte("5-hour limit reached ∙ resets 11:30am").unwrap();
        assert_eq!(r.reprise, heure(11, 30, None));
        let r = refus_texte("Claude AI usage limit reached|1736272800").unwrap();
        assert_eq!(r.reprise, Some(Reprise::Instant(1736272800)));
        let r = refus_texte("Weekly limit reached · resets Mon 9am (UTC)").unwrap();
        assert_eq!(r.reprise, heure(9, 0, Some("UTC")));
        let r = refus_texte("Session limit reached ∙ resets 12am").unwrap();
        assert_eq!(r.reprise, heure(0, 0, None));
        let r = refus_texte("API Error: 429 rate_limit_error").unwrap();
        assert_eq!((r.genre, r.reprise), (GenreRefus::Limite, None));
        let r = refus_texte("Invalid bearer token").unwrap();
        assert_eq!((r.genre, r.reprise), (GenreRefus::Jeton, None));
        let r = refus_texte("OAuth token has expired. Please run /login").unwrap();
        assert_eq!(r.genre, GenreRefus::Jeton);
    }

    #[test]
    fn erreurs_qui_ne_sont_pas_des_refus() {
        assert!(refus_texte("Prompt is too long").is_none());
        assert!(refus_texte("Overloaded").is_none());
        assert!(refus(&ErreurClaude::Delai(300)).is_none());
        assert!(refus(&ErreurClaude::JsonInvalide).is_none());
        // Un chiffre seul n'est pas une heure.
        assert_eq!(
            refus_texte("limit reached, resets in 5").unwrap().reprise,
            None
        );
    }
}
