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
            dossier: None,
            delai: DELAI,
        }
    }
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
        .env("CLAUDE_CODE_OAUTH_TOKEN", a.jeton)
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
