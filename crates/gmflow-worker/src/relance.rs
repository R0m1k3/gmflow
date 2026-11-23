//! Relance des files apalis. Chaque file tourne sous un `Monitor` qui la relance sans limite,
//! après une pause qui grandit avec les relances, quand elle s'arrête sur une erreur (base
//! injoignable, connexion coupée…).
//!
//! Une relance ne double jamais une tâche. L'ancienne incarnation de la file laisse derrière
//! elle des tâches encore en cours (elles tournent dans leur propre tâche tokio) et des tâches
//! « prises » en base dont l'accusé de fin est perdu. Avant de reprendre, la nouvelle
//! incarnation attend la fin des tâches encore en cours, puis range chaque tâche restée prise
//! par une ancienne incarnation : finie pendant la panne → `Done` ou `Failed` ; jamais
//! commencée ou interrompue → remise en attente. Ensuite seulement elle s'inscrit et relève la
//! file (l'inscription d'apalis remet aussi en attente les tâches de workers morts depuis plus
//! de 5 minutes : aucune des nôtres n'est alors encore en cours).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apalis::prelude::{BoxDynError, TaskId};
use serde_json::json;
use sqlx::PgPool;

/// Fins de tâches gardées en mémoire par file : de quoi ranger une panne, pas l'historique.
const FINIES_MAX: usize = 5000;
/// Au-delà de 12 relances d'affilée, la pause ne grandit plus.
const PAUSE_MAX_PAS: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fin {
    Reussie,
    Echouee,
    /// Future abandonnée ou panique : la tâche n'est pas allée au bout.
    Interrompue,
}

#[derive(Default)]
struct EtatFile {
    en_cours: HashSet<String>,
    finies: VecDeque<(String, Fin)>,
    erreur: Option<String>,
}

/// Tâches en cours et récemment finies de chaque file de ce processus.
#[derive(Default)]
pub struct Suivi {
    files: Mutex<HashMap<&'static str, EtatFile>>,
}

/// Garde posée pendant l'exécution d'une tâche : sa fin est notée même sur panique.
struct Garde<'a> {
    suivi: &'a Suivi,
    file: &'static str,
    id: String,
    fin: Fin,
}

impl Drop for Garde<'_> {
    fn drop(&mut self) {
        self.suivi.terminer(self.file, &self.id, self.fin);
    }
}

impl Suivi {
    fn avec<R>(&self, file: &'static str, f: impl FnOnce(&mut EtatFile) -> R) -> R {
        let mut files = self.files.lock().unwrap_or_else(|e| e.into_inner());
        f(files.entry(file).or_default())
    }

    /// Exécute une tâche de `file` en notant son début et sa fin.
    pub async fn executer<F>(
        &self,
        file: &'static str,
        id: &TaskId,
        f: F,
    ) -> Result<(), BoxDynError>
    where
        F: Future<Output = Result<(), BoxDynError>>,
    {
        let id = id.to_string();
        self.avec(file, |e| e.en_cours.insert(id.clone()));
        let mut garde = Garde {
            suivi: self,
            file,
            id,
            fin: Fin::Interrompue,
        };
        let r = f.await;
        garde.fin = if r.is_ok() {
            Fin::Reussie
        } else {
            Fin::Echouee
        };
        r
    }

    fn terminer(&self, file: &'static str, id: &str, fin: Fin) {
        self.avec(file, |e| {
            e.en_cours.remove(id);
            e.finies.push_back((id.to_string(), fin));
            while e.finies.len() > FINIES_MAX {
                e.finies.pop_front();
            }
        });
    }

    pub fn en_cours(&self, file: &'static str) -> usize {
        self.avec(file, |e| e.en_cours.len())
    }

    pub fn fin_de(&self, file: &'static str, id: &str) -> Option<Fin> {
        self.avec(file, |e| {
            e.finies
                .iter()
                .rev()
                .find(|(i, _)| i == id)
                .map(|(_, f)| *f)
        })
    }

    pub fn noter_erreur(&self, file: &'static str, erreur: String) {
        self.avec(file, |e| e.erreur = Some(erreur));
    }

    fn prendre_erreur(&self, file: &'static str) -> Option<String> {
        self.avec(file, |e| e.erreur.take())
    }
}

/// Nom du worker apalis d'une file à sa `relance`-ième incarnation (0 : la première). Un nom
/// neuf à chaque relance : l'ancien peut encore être tenu par la connexion de l'incarnation
/// morte (verrou consultatif d'apalis), et ses tâches restent ainsi reconnaissables.
pub fn nom_worker(file: &str, relance: usize) -> String {
    if relance == 0 {
        format!("gmflow-{file}")
    } else {
        format!("gmflow-{file}-r{relance}")
    }
}

/// File d'un nom de worker (`gmflow-lecture-r3` → `lecture`).
pub fn file_du_worker(nom: &str) -> Option<&'static str> {
    let reste = nom.strip_prefix("gmflow-")?;
    let file = match reste.rsplit_once("-r") {
        Some((f, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => f,
        _ => reste,
    };
    gmflow_core::traitements::FILES
        .iter()
        .find(|f| **f == file)
        .copied()
}

/// Pause avant la `relance`-ième relance : `relance` × `base`, plafonnée à 12 × `base`.
pub fn pause(relance: usize, base: Duration) -> Duration {
    base * (relance.min(PAUSE_MAX_PAS as usize) as u32)
}

/// Bilan d'une reprise : tâches rangées comme finies et tâches remises en attente.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reprise {
    pub terminees: usize,
    pub remises: usize,
}

/// Avant la `relance`-ième incarnation d'une file (rien à faire pour la première) : pause,
/// attente des tâches encore en cours, rangement des tâches prises par les incarnations
/// précédentes, puis une ligne au journal.
pub async fn reprendre(
    pool: &PgPool,
    suivi: &Suivi,
    file: &'static str,
    relance: usize,
    base: Duration,
) -> sqlx::Result<Reprise> {
    if relance == 0 {
        return Ok(Reprise::default());
    }
    tokio::time::sleep(pause(relance, base)).await;
    let mut annonce = false;
    while suivi.en_cours(file) > 0 {
        if !annonce {
            tracing::info!(
                file,
                taches = suivi.en_cours(file),
                "relance de la file : attente de la fin des tâches en cours"
            );
            annonce = true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let anciens: Vec<String> = (0..relance).map(|n| nom_worker(file, n)).collect();
    let mut tx = pool.begin().await?;
    let prises = sqlx::query_scalar!(
        "select id from apalis.jobs
         where job_type = $1 and lock_by = any($2) and status in ('Queued', 'Running')
         for update",
        file,
        &anciens
    )
    .fetch_all(&mut *tx)
    .await?;
    let (mut reussies, mut echouees, mut interrompues, mut jamais) =
        (vec![], vec![], vec![], vec![]);
    for id in prises {
        match suivi.fin_de(file, &id) {
            Some(Fin::Reussie) => reussies.push(id),
            Some(Fin::Echouee) => echouees.push(id),
            Some(Fin::Interrompue) => interrompues.push(id),
            None => jamais.push(id),
        }
    }
    sqlx::query!(
        r#"update apalis.jobs set status = 'Done', done_at = now(), attempts = attempts + 1,
              last_result = '{"Ok": null}'
           where id = any($1)"#,
        &reussies
    )
    .execute(&mut *tx)
    .await?;
    // Comme un échec ordinaire : apalis la reprend tant qu'il reste des essais.
    sqlx::query!(
        r#"update apalis.jobs set status = 'Failed', done_at = now(), attempts = attempts + 1,
              last_result = '{"Err": "échec pendant une panne de la file"}'
           where id = any($1)"#,
        &echouees
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update apalis.jobs set status = 'Pending', done_at = null, lock_by = null, lock_at = null,
            attempts = attempts + case when id = any($2) then 1 else 0 end
         where id = any($1)",
        &[interrompues.clone(), jamais.clone()].concat(),
        &interrompues
    )
    .execute(&mut *tx)
    .await?;
    let r = Reprise {
        terminees: reussies.len() + echouees.len(),
        remises: interrompues.len() + jamais.len(),
    };
    let erreur = suivi.prendre_erreur(file);
    gmflow_core::journal::ecrire(
        &mut *tx,
        None,
        "file_relancee",
        "file",
        Some(file.to_string()),
        json!({
            "relance": relance,
            "erreur": erreur,
            "terminees": r.terminees,
            "remises": r.remises,
        }),
    )
    .await?;
    tx.commit().await?;
    tracing::warn!(
        file,
        relance,
        terminees = r.terminees,
        remises = r.remises,
        "file relancée"
    );
    Ok(r)
}

/// Arrêt demandé (signal) ou en cours : pas de relance. Sinon la file repart, l'erreur est
/// gardée pour le journal de la reprise.
pub fn relancer_apres(
    suivi: &Arc<Suivi>,
    arret: &tokio::sync::watch::Receiver<bool>,
    nom: &str,
    en_arret: bool,
    erreur: &dyn std::fmt::Display,
) -> bool {
    if *arret.borrow() || en_arret {
        return false;
    }
    let texte = erreur.to_string();
    tracing::warn!(worker = nom, erreur = %texte, "file arrêtée sur une erreur : relance");
    if let Some(file) = file_du_worker(nom) {
        suivi.noter_erreur(file, texte.chars().take(500).collect());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_et_pauses() {
        assert_eq!(nom_worker("lecture", 0), "gmflow-lecture");
        assert_eq!(nom_worker("lecture", 3), "gmflow-lecture-r3");
        assert_eq!(file_du_worker("gmflow-lecture"), Some("lecture"));
        assert_eq!(file_du_worker("gmflow-redaction-r12"), Some("redaction"));
        assert_eq!(file_du_worker("gmflow-inconnue"), None);
        let b = Duration::from_secs(5);
        assert_eq!(pause(1, b), Duration::from_secs(5));
        assert_eq!(pause(3, b), Duration::from_secs(15));
        assert_eq!(pause(40, b), Duration::from_secs(60));
    }
}
