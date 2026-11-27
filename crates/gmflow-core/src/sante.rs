//! Santé et métriques (constat C25), lues dans la base au moment de l'appel : l'app et le
//! worker ne se parlent que par elle.
//!
//! - [`worker_vivant`] : battement du planificateur (moins de 10 min) et de chacune des six
//!   files apalis (moins de 5 min). Sert au `/health` du worker, à son healthcheck Docker et à
//!   sa sortie d'office quand il reste bloqué.
//! - [`sonde`] : état public « ok / dégradé » de `/api/sante` (Uptime Kuma), sans détail.
//! - [`metriques`] : texte au format Prometheus (`/metrics` du worker sur 8081,
//!   `/api/metriques` de l'app, protégé).

use std::fmt::Write as _;

use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::{parametre, traitements};

/// Battement du planificateur du worker (toutes les minutes) au-delà duquel il est arrêté.
pub const BATTEMENT_MAX: time::Duration = time::Duration::minutes(10);
/// Une file bat toutes les 30 s ; une file relancée attend d'abord la fin des tâches de son
/// incarnation précédente, d'où une marge plus large que pour l'orchestre (3 min).
pub const BATTEMENT_FILE_MAX: time::Duration = time::Duration::minutes(5);
/// Une boîte active sans relève réussie depuis ce temps est « en retard » (relève toutes les
/// 5 min).
pub const RELEVE_RETARD_MINUTES: i32 = 30;

/// Dernier battement du planificateur du worker.
pub async fn battement_planificateur(pool: &PgPool) -> Option<OffsetDateTime> {
    let v = parametre::lire(pool, parametre::WORKER_BATTEMENT)
        .await
        .ok()??;
    let quand = v.get("quand")?.as_str()?;
    OffsetDateTime::parse(quand, &time::format_description::well_known::Rfc3339).ok()
}

/// Le worker vit-il : base joignable, planificateur et six files qui battent.
pub async fn worker_vivant(pool: &PgPool) -> bool {
    let maintenant = OffsetDateTime::now_utc();
    let planificateur = battement_planificateur(pool)
        .await
        .is_some_and(|t| maintenant - t < BATTEMENT_MAX);
    if !planificateur {
        return false;
    }
    let Ok(b) = traitements::battements(pool).await else {
        return false;
    };
    traitements::FILES.iter().all(|f| {
        b.get(*f)
            .is_some_and(|t| maintenant - *t < BATTEMENT_FILE_MAX)
    })
}

/// Boîtes actives dont la dernière relève réussie (ou l'activation) date de plus de
/// `RELEVE_RETARD_MINUTES`, hors reprise en cours ; une boîte en erreur ou en pause compte.
pub async fn releves_en_retard(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"select count(*) as "n!" from boite_mail
           where active
             and greatest(derniere_releve, activee_le, cree_le) < now() - make_interval(mins => $1)
             and (releve_jusqu_a is null or releve_jusqu_a < now())"#,
        RELEVE_RETARD_MINUTES
    )
    .fetch_one(pool)
    .await
}

/// Réponse publique de la sonde : aucun nom, aucun chiffre.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Sonde {
    /// `ok` ou `degrade`.
    pub etat: &'static str,
    /// Causes, en codes courts : `base`, `worker`, `releve`.
    pub causes: Vec<&'static str>,
}

impl Sonde {
    pub fn ok(&self) -> bool {
        self.etat == "ok"
    }
}

pub async fn sonde(pool: &PgPool) -> Sonde {
    let mut causes = Vec::new();
    if sqlx::query("select 1").execute(pool).await.is_err() {
        causes.push("base");
    } else {
        if !worker_vivant(pool).await {
            causes.push("worker");
        }
        if releves_en_retard(pool).await.map_or(true, |n| n > 0) {
            causes.push("releve");
        }
    }
    Sonde {
        etat: if causes.is_empty() { "ok" } else { "degrade" },
        causes,
    }
}

/// Valeur d'étiquette Prometheus (barre oblique inverse, guillemet et saut de ligne échappés).
fn etiquette(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Accumule les familles de métriques.
struct Sortie(String);

impl Sortie {
    fn famille(&mut self, nom: &str, aide: &str, genre: &str) {
        let _ = writeln!(self.0, "# HELP {nom} {aide}\n# TYPE {nom} {genre}");
    }

    fn valeur(&mut self, nom: &str, etiquettes: &[(&str, &str)], v: f64) {
        let e: Vec<String> = etiquettes
            .iter()
            .map(|(k, val)| format!("{k}=\"{}\"", etiquette(val)))
            .collect();
        if e.is_empty() {
            let _ = writeln!(self.0, "{nom} {v}");
        } else {
            let _ = writeln!(self.0, "{nom}{{{}}} {v}", e.join(","));
        }
    }
}

fn age_secondes(t: Option<OffsetDateTime>) -> f64 {
    t.map_or(-1.0, |t| {
        (OffsetDateTime::now_utc() - t).as_seconds_f64().max(0.0)
    })
}

/// Métriques au format texte de Prometheus (version 0.0.4).
pub async fn metriques(pool: &PgPool) -> sqlx::Result<String> {
    let mut s = Sortie(String::new());
    let files: Vec<String> = traitements::FILES.iter().map(|f| f.to_string()).collect();

    let taches = sqlx::query!(
        r#"select f as "file!",
                  count(j.id) filter (where j.status in ('Pending', 'Queued')
                      or (j.status = 'Failed' and j.attempts < j.max_attempts)) as "en_attente!",
                  count(j.id) filter (where j.status = 'Running') as "en_cours!",
                  count(j.id) filter (where (j.status = 'Failed' and j.attempts >= j.max_attempts)
                      or j.status = 'Killed') as "echouees!",
                  count(j.id) filter (where ((j.status = 'Failed' and j.attempts >= j.max_attempts)
                      or j.status = 'Killed')
                      and coalesce(j.done_at, j.run_at) > now() - interval '24 hours') as "echouees_24h!",
                  extract(epoch from now() - min(j.run_at) filter (
                      where j.status in ('Pending', 'Queued') and j.run_at <= now()))::float8
                      as "plus_vieille"
           from unnest($1::text[]) f
           left join apalis.jobs j on j.job_type = f and j.status <> 'Done'
           group by f order by f"#,
        &files
    )
    .fetch_all(pool)
    .await?;
    s.famille(
        "gmflow_file_taches",
        "Tâches apalis par file et par état (en_attente, en_cours, echouees : abandonnées).",
        "gauge",
    );
    for t in &taches {
        for (etat, n) in [
            ("en_attente", t.en_attente),
            ("en_cours", t.en_cours),
            ("echouees", t.echouees),
        ] {
            s.valeur(
                "gmflow_file_taches",
                &[("file", &t.file), ("etat", etat)],
                n as f64,
            );
        }
    }
    s.famille(
        "gmflow_file_taches_echouees_24h",
        "Tâches abandonnées (essais épuisés) depuis 24 h, par file.",
        "gauge",
    );
    for t in &taches {
        s.valeur(
            "gmflow_file_taches_echouees_24h",
            &[("file", &t.file)],
            t.echouees_24h as f64,
        );
    }
    s.famille(
        "gmflow_file_plus_vieille_tache_secondes",
        "Âge de la plus vieille tâche prête qui attend, par file (0 : aucune).",
        "gauge",
    );
    for t in &taches {
        s.valeur(
            "gmflow_file_plus_vieille_tache_secondes",
            &[("file", &t.file)],
            t.plus_vieille.unwrap_or(0.0).max(0.0),
        );
    }

    let battements = traitements::battements(pool).await?;
    s.famille(
        "gmflow_file_battement_age_secondes",
        "Temps depuis le dernier battement de chaque file du worker (-1 : jamais).",
        "gauge",
    );
    for f in traitements::FILES {
        s.valeur(
            "gmflow_file_battement_age_secondes",
            &[("file", f)],
            age_secondes(battements.get(f).copied()),
        );
    }
    s.famille(
        "gmflow_worker_battement_age_secondes",
        "Temps depuis le dernier battement du planificateur du worker (-1 : jamais).",
        "gauge",
    );
    s.valeur(
        "gmflow_worker_battement_age_secondes",
        &[],
        age_secondes(battement_planificateur(pool).await),
    );

    let boites = sqlx::query!(
        r#"select id, libelle, active, pause_auto, (erreur_depuis is not null) as "en_erreur!",
                  extract(epoch from derniere_releve)::float8 as "derniere"
           from boite_mail order by id"#
    )
    .fetch_all(pool)
    .await?;
    s.famille(
        "gmflow_boite_derniere_releve_horodatage",
        "Heure (Unix) de la dernière relève réussie de chaque boîte (0 : jamais).",
        "gauge",
    );
    for b in &boites {
        let id = b.id.to_string();
        s.valeur(
            "gmflow_boite_derniere_releve_horodatage",
            &[("boite_id", &id), ("boite", &b.libelle)],
            b.derniere.unwrap_or(0.0),
        );
    }
    s.famille(
        "gmflow_boite_etat",
        "État de chaque boîte : 1 pour l'état courant (active, pause_auto, erreur, inactive).",
        "gauge",
    );
    for b in &boites {
        let id = b.id.to_string();
        let etat = if !b.active {
            "inactive"
        } else if b.pause_auto {
            "pause_auto"
        } else if b.en_erreur {
            "erreur"
        } else {
            "active"
        };
        s.valeur(
            "gmflow_boite_etat",
            &[("boite_id", &id), ("boite", &b.libelle), ("etat", etat)],
            1.0,
        );
    }
    s.famille(
        "gmflow_releves_en_retard",
        "Boîtes actives sans relève réussie depuis 30 minutes.",
        "gauge",
    );
    s.valeur(
        "gmflow_releves_en_retard",
        &[],
        releves_en_retard(pool).await? as f64,
    );

    let documents = sqlx::query!(
        r#"select etat as "etat!", count(*) as "n!" from document group by etat order by etat"#
    )
    .fetch_all(pool)
    .await?;
    s.famille(
        "gmflow_documents",
        "Documents par état de lecture.",
        "gauge",
    );
    for d in &documents {
        s.valeur("gmflow_documents", &[("etat", &d.etat)], d.n as f64);
    }
    let depots = sqlx::query!(
        r#"select statut_depot as "statut!", count(*) as "n!" from document
           where statut_depot is not null group by statut_depot order by statut_depot"#
    )
    .fetch_all(pool)
    .await?;
    s.famille(
        "gmflow_documents_depot",
        "Factures et avoirs par statut d'envoi (a_verifier, pret, en_envoi, depose, erreur, doublon).",
        "gauge",
    );
    for d in &depots {
        s.valeur(
            "gmflow_documents_depot",
            &[("statut", &d.statut)],
            d.n as f64,
        );
    }

    let claude = sqlx::query!(
        r#"select count(*) filter (where succes) as "reussis!",
                  count(*) filter (where not succes) as "echoues!",
                  coalesce(sum(cout_usd), 0)::float8 as "cout!"
           from appel_claude
           where quand >= (date_trunc('day', now() at time zone 'Europe/Paris')
                           at time zone 'Europe/Paris')"#
    )
    .fetch_one(pool)
    .await?;
    s.famille(
        "gmflow_claude_appels_jour",
        "Appels à Claude depuis minuit (heure de Paris), réussis ou échoués.",
        "gauge",
    );
    s.valeur(
        "gmflow_claude_appels_jour",
        &[("resultat", "reussi")],
        claude.reussis as f64,
    );
    s.valeur(
        "gmflow_claude_appels_jour",
        &[("resultat", "echec")],
        claude.echoues as f64,
    );
    s.famille(
        "gmflow_claude_cout_jour_dollars",
        "Coût des appels à Claude depuis minuit (heure de Paris), en dollars.",
        "gauge",
    );
    s.valeur("gmflow_claude_cout_jour_dollars", &[], claude.cout);

    let envois = sqlx::query_scalar!(
        r#"select count(*) as "n!" from envoi_depot
           where etat = 'erreur' and quand > now() - interval '24 hours'"#
    )
    .fetch_one(pool)
    .await?;
    s.famille(
        "gmflow_envois_echec_24h",
        "Envois vers Dext ou Yooz en échec depuis 24 h.",
        "gauge",
    );
    s.valeur("gmflow_envois_echec_24h", &[], envois as f64);
    Ok(s.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquettes_echappees() {
        assert_eq!(etiquette(r#"Boîte "Frouard"\x"#), r#"Boîte \"Frouard\"\\x"#);
        let mut s = Sortie(String::new());
        s.valeur("m", &[("a", "1"), ("b", "x\ny")], 2.0);
        s.valeur("n", &[], 0.5);
        assert_eq!(s.0, "m{a=\"1\",b=\"x\\ny\"} 2\nn 0.5\n");
    }
}
