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
    let resultat = executer(claude_bin, a).await;
    let duree_ms = debut.elapsed().as_millis() as u64;
    let (succes, message) = match &resultat {
        Ok(_) => (true, None),
        Err(e) => (false, Some(tronquer(&e.to_string(), 500))),
    };
    if let Err(e) = sqlx::query!(
        "insert into appel_claude (tache, modele, effort, duree_ms, succes, message) values ($1, $2, $3, $4, $5, $6)",
        tache,
        modele,
        effort,
        duree_ms.min(i32::MAX as u64) as i32,
        succes,
        message
    )
    .execute(pool)
    .await
    {
        tracing::warn!(erreur = %e, "écriture de appel_claude impossible");
    }
    resultat.map(|json| SortieClaude { json, duree_ms })
}

async fn executer(claude_bin: &str, a: &Appel<'_>) -> Result<Value, ErreurClaude> {
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
            return Err(ErreurClaude::Modele(message_json(&json)));
        }
        return Err(ErreurClaude::Sortie {
            code: sortie.status.code(),
            stderr: tronquer(&String::from_utf8_lossy(&sortie.stderr), 500),
        });
    }
    let json: Value =
        serde_json::from_slice(&sortie.stdout).map_err(|_| ErreurClaude::JsonInvalide)?;
    if json.get("is_error").and_then(Value::as_bool) != Some(false) {
        return Err(ErreurClaude::Modele(message_json(&json)));
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
