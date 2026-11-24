//! Worker GMFlow : relève IMAP, test des boîtes, lecture des pièces jointes, fiches
//! fournisseurs, dépôts Dext/Yooz et alertes, séances setup-token et test du jeton Claude.

pub mod envoi;
pub mod fournisseur;
pub mod lecture;
pub mod logiflow;
pub mod orchestre;
pub mod planif;
pub mod purge;
pub mod redaction;
pub mod relance;
pub mod releve;
pub mod reponse;
pub mod sante;
pub mod serveur_api;
pub mod setup_token;
pub mod test_boite;
pub mod test_jeton;
pub mod tri;

use std::sync::Arc;
use std::time::Duration;

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
    /// Tâches en cours et finies de chaque file (pour relancer une file sans rien doubler).
    pub suivi: Arc<relance::Suivi>,
    /// Pause de base avant la relance d'une file (× numéro de relance, 12 fois au plus).
    pub relance_base: Duration,
}

impl Contexte {
    pub fn new(pool: PgPool, config: Config, transport: Transport, delais: DelaisSeance) -> Self {
        Self {
            pool,
            chiffreur: Chiffreur::new(&config.cle),
            config: Arc::new(config),
            transport,
            delais,
            suivi: Arc::default(),
            relance_base: Duration::from_secs(5),
        }
    }
}

async fn tache_boite(
    tache: TacheBoite,
    ctx: Data<Contexte>,
    id: TaskId,
) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_BOITES, &id, async {
            match tache {
                TacheBoite::Releve { boite_id } => {
                    releve::relever(&ctx, boite_id).await?;
                }
                TacheBoite::TestBoite { boite_id } => test_boite::tester(&ctx, boite_id).await?,
                TacheBoite::Essai { essai_id } => test_boite::essayer(&ctx, essai_id).await?,
            }
            Ok(())
        })
        .await
}

async fn tache_claude(
    tache: TacheClaude,
    ctx: Data<Contexte>,
    id: TaskId,
) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_CLAUDE, &id, async {
            match tache {
                TacheClaude::SeanceSetupToken { seance_id } => {
                    setup_token::executer(&ctx, seance_id).await?
                }
                TacheClaude::TestJeton => test_jeton::tester(&ctx).await?,
            }
            Ok(())
        })
        .await
}

async fn tache_lecture(
    tache: TacheLecture,
    ctx: Data<Contexte>,
    id: TaskId,
) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_LECTURE, &id, async {
            match tache {
                TacheLecture::LireDocument {
                    document_id,
                    forcer,
                    approfondie,
                    hors_quota,
                } => lecture::lire(&ctx, document_id, forcer, approfondie, hors_quota).await?,
            }
            Ok(())
        })
        .await
}

async fn tache_envoi(
    tache: TacheEnvoi,
    ctx: Data<Contexte>,
    id: TaskId,
) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_ENVOI, &id, async {
            match tache {
                TacheEnvoi::Deposer {
                    document_id,
                    forcer,
                    auteur_id,
                } => envoi::deposer(&ctx, document_id, forcer, auteur_id).await?,
                TacheEnvoi::TestExpedition { destinataire } => {
                    envoi::tester_expedition(&ctx, &destinataire).await?
                }
                TacheEnvoi::TestServeurApi => serveur_api::tester(&ctx).await?,
                TacheEnvoi::SynchroMagasin => serveur_api::synchroniser(&ctx).await?,
                TacheEnvoi::TestLogiflow => logiflow::tester(&ctx).await?,
                TacheEnvoi::RapprocherLogiflow { document_id } => {
                    logiflow::rapprocher(&ctx, document_id).await?
                }
                TacheEnvoi::VerifierArticlesBl { document_id } => {
                    serveur_api::verifier_bl(&ctx, document_id).await?
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
        })
        .await
}

async fn tache_tri(tache: TacheTri, ctx: Data<Contexte>, id: TaskId) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_TRI, &id, async {
            match tache {
                TacheTri::TrierMail { mail_id } => tri::trier(&ctx, mail_id).await?,
            }
            Ok(())
        })
        .await
}

async fn tache_redaction(
    tache: TacheRedaction,
    ctx: Data<Contexte>,
    id: TaskId,
) -> Result<(), BoxDynError> {
    let c = &*ctx;
    c.suivi
        .executer(file::FILE_REDACTION, &id, async {
            match tache {
                TacheRedaction::GenererBrouillon { brouillon_id } => {
                    redaction::generer(&ctx, brouillon_id).await?
                }
            }
            Ok(())
        })
        .await
}

/// Une file sous le `Monitor` : nom neuf et reprise (pause, rangement des tâches de
/// l'incarnation précédente) à chaque relance.
macro_rules! enregistrer {
    ($monitor:expr, $ctx:expr, $file:expr, $stockage:path, $concurrence:expr, $tache:expr) => {{
        let ctx = $ctx.clone();
        $monitor.register(move |relance| {
            let (pool, suivi, base) = (ctx.pool.clone(), ctx.suivi.clone(), ctx.relance_base);
            let backend = $stockage(&ctx.pool).with_pubsub().before_start(move |_| {
                let (pool, suivi) = (pool.clone(), suivi.clone());
                async move {
                    relance::reprendre(&pool, &suivi, $file, relance, base).await?;
                    Ok(())
                }
            });
            WorkerBuilder::new(relance::nom_worker($file, relance))
                .backend(backend)
                .data(ctx.clone())
                .concurrency($concurrence)
                .parallelize(tokio::spawn)
                .build($tache)
        })
    }};
}

/// Lance les six files apalis sous un `Monitor` : `boites` (4 tâches à la fois), `claude`
/// (1), `lecture` (2), `envoi` (1 : un dépôt n'en croise jamais un autre), `tri` (2) et
/// `redaction` (1), jusqu'à ce que `arret` passe à vrai (jamais en production). Une file qui
/// s'arrête sur une erreur est relancée sans limite (voir [`relance`]). LISTEN/NOTIFY : une
/// tâche poussée par l'app est prise tout de suite, sans attendre le tour de scrutation.
pub async fn lancer_files(
    ctx: Contexte,
    arret: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let suivi = ctx.suivi.clone();
    let arret_relance = arret.clone();
    let monitor = Monitor::new().should_restart(move |w, e| {
        relance::relancer_apres(&suivi, &arret_relance, w.name(), w.is_shutting_down(), e)
    });
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_BOITES,
        file::stockage_boites,
        4,
        tache_boite
    );
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_CLAUDE,
        file::stockage_claude,
        1,
        tache_claude
    );
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_LECTURE,
        file::stockage_lecture,
        2,
        tache_lecture
    );
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_ENVOI,
        file::stockage_envoi,
        1,
        tache_envoi
    );
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_TRI,
        file::stockage_tri,
        2,
        tache_tri
    );
    let monitor = enregistrer!(
        monitor,
        ctx,
        file::FILE_REDACTION,
        file::stockage_redaction,
        1,
        tache_redaction
    );
    let mut arret = arret;
    monitor
        .run_with_signal(async move {
            let _ = arret.wait_for(|v| *v).await;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("files arrêtées : {e}"))
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
    tokio::spawn(orchestre::boucle(ctx.clone()));
    tracing::info!(transport = ?ctx.transport, "GMFlow worker démarré");
    let (_garde, arret) = tokio::sync::watch::channel(false);
    lancer_files(ctx, arret).await
}
