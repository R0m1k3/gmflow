//! Worker GMFlow : relève IMAP, test des boîtes, lecture des pièces jointes, fiches
//! fournisseurs, dépôts Dext/Yooz et alertes, séances setup-token et test du jeton Claude.

pub mod envoi;
pub mod fournisseur;
pub mod lecture;
pub mod planif;
pub mod purge;
pub mod redaction;
pub mod releve;
pub mod reponse;
pub mod sante;
pub mod setup_token;
pub mod test_boite;
pub mod test_jeton;
pub mod tri;

use std::sync::Arc;

use apalis::prelude::*;
use gmflow_core::config::Config;
use gmflow_core::crypto::Chiffreur;
use gmflow_core::file::{
    self, TacheBoite, TacheClaude, TacheEnvoi, TacheLecture, TacheRedaction, TacheTri,
};
use gmflow_core::imap::Transport;
use sqlx::PgPool;

pub use setup_token::DelaisSeance;

#[derive(Clone)]
pub struct Contexte {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub chiffreur: Chiffreur,
    pub transport: Transport,
    pub delais: DelaisSeance,
}

impl Contexte {
    pub fn new(pool: PgPool, config: Config, transport: Transport, delais: DelaisSeance) -> Self {
        Self {
            pool,
            chiffreur: Chiffreur::new(&config.cle),
            config: Arc::new(config),
            transport,
            delais,
        }
    }
}

async fn tache_boite(tache: TacheBoite, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheBoite::Releve { boite_id } => {
            releve::relever(&ctx, boite_id).await?;
        }
        TacheBoite::TestBoite { boite_id } => test_boite::tester(&ctx, boite_id).await?,
        TacheBoite::Essai { essai_id } => test_boite::essayer(&ctx, essai_id).await?,
    }
    Ok(())
}

async fn tache_claude(tache: TacheClaude, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheClaude::SeanceSetupToken { seance_id } => {
            setup_token::executer(&ctx, seance_id).await?
        }
        TacheClaude::TestJeton => test_jeton::tester(&ctx).await?,
    }
    Ok(())
}

async fn tache_lecture(tache: TacheLecture, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheLecture::LireDocument {
            document_id,
            forcer,
            approfondie,
            hors_quota,
        } => lecture::lire(&ctx, document_id, forcer, approfondie, hors_quota).await?,
    }
    Ok(())
}

async fn tache_envoi(tache: TacheEnvoi, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheEnvoi::Deposer {
            document_id,
            forcer,
            auteur_id,
        } => envoi::deposer(&ctx, document_id, forcer, auteur_id).await?,
        TacheEnvoi::TestExpedition { destinataire } => {
            envoi::tester_expedition(&ctx, &destinataire).await?
        }
        TacheEnvoi::Alertes => {
            reponse::interrompus(&ctx.pool).await?;
            envoi::alertes(&ctx).await?
        }
        TacheEnvoi::EnvoyerReponse {
            brouillon_id,
            auteur_id,
        } => reponse::envoyer(&ctx, brouillon_id, auteur_id).await?,
    }
    Ok(())
}

async fn tache_tri(tache: TacheTri, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheTri::TrierMail { mail_id } => tri::trier(&ctx, mail_id).await?,
    }
    Ok(())
}

async fn tache_redaction(tache: TacheRedaction, ctx: Data<Contexte>) -> Result<(), BoxDynError> {
    match tache {
        TacheRedaction::GenererBrouillon { brouillon_id } => {
            redaction::generer(&ctx, brouillon_id).await?
        }
    }
    Ok(())
}

/// Lance les quatre workers apalis : `boites` (4 tâches à la fois), `claude` (1 à la fois),
/// `lecture` (2 à la fois) et `envoi` (1 à la fois : un dépôt n'en croise jamais un autre),
/// jusqu'à ce que `arret` passe à vrai (jamais en production). LISTEN/NOTIFY : une tâche
/// poussée par l'app est prise tout de suite, sans attendre le tour de scrutation (30 s).
pub async fn lancer_files(
    ctx: Contexte,
    arret: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let signal = |mut r: tokio::sync::watch::Receiver<bool>| async move {
        let _ = r.wait_for(|v| *v).await;
        Ok::<(), std::io::Error>(())
    };
    let boites = WorkerBuilder::new("gmflow-boites")
        .backend(file::stockage_boites(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(4)
        .parallelize(tokio::spawn)
        .build(tache_boite);
    let claude = WorkerBuilder::new("gmflow-claude")
        .backend(file::stockage_claude(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(1)
        .parallelize(tokio::spawn)
        .build(tache_claude);
    let lecture = WorkerBuilder::new("gmflow-lecture")
        .backend(file::stockage_lecture(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(2)
        .parallelize(tokio::spawn)
        .build(tache_lecture);
    let envoi = WorkerBuilder::new("gmflow-envoi")
        .backend(file::stockage_envoi(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(1)
        .parallelize(tokio::spawn)
        .build(tache_envoi);
    let tri = WorkerBuilder::new("gmflow-tri")
        .backend(file::stockage_tri(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(2)
        .parallelize(tokio::spawn)
        .build(tache_tri);
    let redaction = WorkerBuilder::new("gmflow-redaction")
        .backend(file::stockage_redaction(&ctx.pool).with_pubsub())
        .data(ctx.clone())
        .concurrency(1)
        .parallelize(tokio::spawn)
        .build(tache_redaction);
    let (a, b, c, d, e, f) = tokio::join!(
        boites.run_until(signal(arret.clone())),
        claude.run_until(signal(arret.clone())),
        lecture.run_until(signal(arret.clone())),
        envoi.run_until(signal(arret.clone())),
        tri.run_until(signal(arret.clone())),
        redaction.run_until(signal(arret))
    );
    a.map_err(|e| anyhow::anyhow!("worker boites : {e}"))?;
    b.map_err(|e| anyhow::anyhow!("worker claude : {e}"))?;
    c.map_err(|e| anyhow::anyhow!("worker lecture : {e}"))?;
    d.map_err(|e| anyhow::anyhow!("worker envoi : {e}"))?;
    e.map_err(|e| anyhow::anyhow!("worker tri : {e}"))?;
    f.map_err(|e| anyhow::anyhow!("worker redaction : {e}"))?;
    Ok(())
}

pub async fn lancer(config: Config) -> anyhow::Result<()> {
    let pool = gmflow_core::db::connecter(&config.database_url).await?;
    gmflow_core::db::initialiser(&pool).await?;
    let ctx = Contexte::new(
        pool,
        config,
        Transport::depuis_env(),
        DelaisSeance::default(),
    );
    if gmflow_core::reglages::passer_quota_a_1000(&ctx.pool).await? {
        tracing::info!("quota de lecture relevé de 100 à 1000 appels par jour");
    }
    setup_token::fermer_seances_ouvertes(&ctx.pool).await?;
    envoi::envois_interrompus(&ctx.pool).await?;
    reponse::interrompus(&ctx.pool).await?;
    tokio::spawn(planif::boucle(
        ctx.pool.clone(),
        ctx.config.fichiers.clone(),
    ));
    tokio::spawn(sante::serveur(ctx.pool.clone()));
    tracing::info!(transport = ?ctx.transport, "GMFlow worker démarré");
    let (_garde, arret) = tokio::sync::watch::channel(false);
    lancer_files(ctx, arret).await
}
