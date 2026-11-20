//! Séance `claude setup-token` dans un pseudo-terminal, pilotée par la ligne `seance_claude`.
//!
//! Sortie vérifiée sur Claude CLI 2.1.318 (HOME vide) : bannière, URL
//! `https://claude.com/cai/oauth/authorize?…`, puis « Paste code here if prompted > », sans question
//! de premier lancement.
//!
//! demarrage → attente_code (URL lue) → code_recu (code collé dans l'UI) → terminee (jeton lu).
//! Sinon : erreur (pas d'URL, pas de jeton, fin inattendue, worker redémarré) ou expiree (pas de
//! code). Dans tous les cas le processus est tué, le code effacé et le HOME temporaire supprimé.

use std::io::{Read, Write};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gmflow_core::claude::DossierTemp;
use gmflow_core::{file, parametre};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use regex::Regex;
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::Contexte;

#[derive(Clone, Copy, Debug)]
pub struct DelaisSeance {
    pub url: Duration,
    pub jeton: Duration,
    pub code: Duration,
}

impl Default for DelaisSeance {
    fn default() -> Self {
        Self {
            url: Duration::from_secs(180),
            jeton: Duration::from_secs(60),
            code: Duration::from_secs(600),
        }
    }
}

fn motif_url() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"https://(?:claude\.ai|claude\.com|console\.anthropic\.com)/\S+").unwrap()
    })
}

fn motif_jeton() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"sk-ant-oat\d+-[A-Za-z0-9_-]+").unwrap())
}

/// Au démarrage du worker : aucune séance ouverte ne peut survivre.
pub async fn fermer_seances_ouvertes(pool: &PgPool) -> anyhow::Result<()> {
    let n = sqlx::query!(
        "update seance_claude set etat = 'erreur', message = 'worker redémarré', code_chiffre = null
         where etat in ('demarrage', 'attente_code', 'code_recu')"
    )
    .execute(pool)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::warn!(seances = n, "séances setup-token fermées au démarrage");
    }
    Ok(())
}

async fn terminer(pool: &PgPool, id: Uuid, etat: &str, message: Option<&str>) -> sqlx::Result<()> {
    sqlx::query!(
        "update seance_claude set etat = $2, message = $3, code_chiffre = null where id = $1",
        id,
        etat,
        message
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn executer(ctx: &Contexte, id: Uuid) -> anyhow::Result<()> {
    let ligne = sqlx::query!(
        "select etat, expire_le < now() as \"expiree!\" from seance_claude where id = $1",
        id
    )
    .fetch_optional(&ctx.pool)
    .await?;
    let Some(ligne) = ligne else { return Ok(()) };
    if ligne.etat != "demarrage" {
        return Ok(());
    }
    if ligne.expiree {
        terminer(
            &ctx.pool,
            id,
            "expiree",
            Some("séance non démarrée à temps"),
        )
        .await?;
        return Ok(());
    }
    let home = DossierTemp::creer("gmflow-setup-token")?;
    let resultat = piloter(ctx, id, &home).await;
    drop(home);
    match resultat {
        Ok(()) => Ok(()),
        Err(e) => {
            let message: String = e.to_string().chars().take(300).collect();
            terminer(&ctx.pool, id, "erreur", Some(&message)).await?;
            Err(e)
        }
    }
}

enum Fin {
    Termine,
    Expiree,
}

async fn piloter(ctx: &Contexte, id: Uuid, home: &DossierTemp) -> anyhow::Result<()> {
    let pty = native_pty_system();
    let paire = pty.openpty(PtySize {
        rows: 50,
        cols: 500,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(&ctx.config.claude_bin);
    cmd.arg("setup-token");
    cmd.env_clear();
    cmd.env(
        "PATH",
        std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into()),
    );
    cmd.env("HOME", &home.0);
    cmd.env("TERM", "xterm-256color");
    cmd.env("DISABLE_AUTOUPDATER", "1");
    cmd.cwd(&home.0);
    let mut enfant = paire.slave.spawn_command(cmd)?;
    drop(paire.slave);
    let mut lecteur = paire.master.try_clone_reader()?;
    let mut ecrivain = paire.master.take_writer()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut tampon = [0u8; 4096];
        loop {
            match lecteur.read(&mut tampon) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(tampon[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let resultat = async {
        let mut sortie: Vec<u8> = Vec::new();
        let mut ferme = false;
        let lire = |rx: &mut mpsc::UnboundedReceiver<Vec<u8>>, sortie: &mut Vec<u8>, ferme: &mut bool| {
            loop {
                match rx.try_recv() {
                    Ok(o) => sortie.extend(o),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        *ferme = true;
                        break;
                    }
                }
            }
        };
        let texte = |sortie: &[u8]| String::from_utf8_lossy(&strip_ansi_escapes::strip(sortie)).into_owned();

        // 1. L'URL de connexion.
        let debut = Instant::now();
        let url = loop {
            lire(&mut rx, &mut sortie, &mut ferme);
            // L'URL n'est retenue qu'une fois suivie d'un blanc : elle peut arriver en plusieurs morceaux.
            let t = texte(&sortie);
            if let Some(m) = motif_url().find(&t)
                && m.end() < t.len()
            {
                break m.as_str().to_string();
            }
            if ferme {
                anyhow::bail!("Claude CLI s'est arrêté sans donner d'URL");
            }
            if debut.elapsed() > ctx.delais.url {
                anyhow::bail!("pas d'URL de connexion après {} s", ctx.delais.url.as_secs());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        let secondes_code = ctx.delais.code.as_secs_f64();
        sqlx::query!(
            "update seance_claude set etat = 'attente_code', url = $2, expire_le = now() + make_interval(secs => $3)
             where id = $1 and etat = 'demarrage'",
            id,
            url,
            secondes_code
        )
        .execute(&ctx.pool)
        .await?;

        // 2. Le code collé dans l'UI (ligne relue chaque seconde).
        let debut = Instant::now();
        loop {
            lire(&mut rx, &mut sortie, &mut ferme);
            if ferme {
                anyhow::bail!("Claude CLI s'est arrêté avant de recevoir le code");
            }
            let l = sqlx::query!("select etat, code_chiffre from seance_claude where id = $1", id)
                .fetch_one(&ctx.pool)
                .await?;
            if l.etat == "code_recu"
                && let Some(c) = l.code_chiffre
            {
                let code = ctx.chiffreur.dechiffrer_texte(&c)?;
                sqlx::query!("update seance_claude set code_chiffre = null where id = $1", id)
                    .execute(&ctx.pool)
                    .await?;
                ecrivain.write_all(code.trim().as_bytes())?;
                ecrivain.write_all(b"\r")?;
                ecrivain.flush()?;
                break;
            }
            if l.etat != "attente_code" {
                anyhow::bail!("séance interrompue (état {})", l.etat);
            }
            if debut.elapsed() > ctx.delais.code {
                return Ok(Fin::Expiree);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // 3. Le jeton, dans la sortie qui suit le code.
        let depuis = sortie.len();
        let debut = Instant::now();
        let jeton = loop {
            lire(&mut rx, &mut sortie, &mut ferme);
            if let Some(m) = motif_jeton().find(&texte(&sortie[depuis..])) {
                break m.as_str().to_string();
            }
            if ferme {
                let fin = texte(&sortie[depuis..]);
                let fin: String = fin.trim().chars().take(200).collect();
                anyhow::bail!("Claude CLI s'est arrêté sans donner de jeton : {fin}");
            }
            if debut.elapsed() > ctx.delais.jeton {
                anyhow::bail!("pas de jeton {} s après le code", ctx.delais.jeton.as_secs());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        parametre::enregistrer_jeton_claude(&ctx.pool, &ctx.chiffreur, &jeton).await?;
        Ok(Fin::Termine)
    }
    .await;

    let _ = enfant.kill();
    let _ = tokio::task::spawn_blocking(move || enfant.wait()).await;

    match resultat? {
        Fin::Termine => {
            terminer(&ctx.pool, id, "terminee", None).await?;
            file::pousser_test_jeton(&ctx.pool).await?;
        }
        Fin::Expiree => terminer(&ctx.pool, id, "expiree", Some("aucun code reçu")).await?,
    }
    Ok(())
}
