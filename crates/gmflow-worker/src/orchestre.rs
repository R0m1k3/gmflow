//! Orchestre du worker. Toutes les 5 secondes, il exécute les relances demandées depuis l'app
//! (Réglages > Traitements, par la table `parametre`). Chaque minute, il fait un tour :
//! - files arrêtées ou reparties (battement apalis) ;
//! - lectures et tris restés « en cours » plus de 30 minutes remis en attente ;
//! - pour les documents qui attendent depuis plus de 15 minutes, la raison du blocage écrite
//!   dans `document.message` ;
//! - disponibilité de Claude (jeton, refus, quota) et mails pas encore triés ;
//! - le tout écrit dans `parametre.orchestre_etat`.
//!
//! Il ne journalise que les changements (file arrêtée, Claude de nouveau disponible…).

use std::collections::HashMap;
use std::time::Duration;

use gmflow_core::traitements::{self, BATTEMENT_FILE_MAX, FILES, RETARD_MIN};
use gmflow_core::{attente, file, journal, parametre, reglages};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::Contexte;

pub const PERIODE: Duration = Duration::from_secs(60);
pub const SCRUTATION: Duration = Duration::from_secs(5);
/// Laisse aux files le temps de s'inscrire avant le premier tour.
const PREMIER_TOUR: Duration = Duration::from_secs(30);
const DIAGNOSTICS_PAR_TOUR: i64 = 100;
/// Raisons qui valent d'être écrites sur le document : un vrai blocage, pas une attente
/// ordinaire dans la file.
const GENRES_NOTES: &[&str] = &[
    "bloque",
    "file_arretee",
    "jeton_absent",
    "claude_refuse",
    "quota",
    "lecture_coupee",
];

/// Ce que l'orchestre a vu au tour précédent (pour ne journaliser que les changements).
#[derive(Default)]
pub struct Memoire {
    files: HashMap<&'static str, bool>,
    claude: Option<Option<String>>,
    retard_lecture: bool,
    retard_tri: bool,
}

pub async fn boucle(ctx: Contexte) {
    let mut m = Memoire::default();
    let mut prochain = tokio::time::Instant::now() + PREMIER_TOUR;
    loop {
        if let Err(e) = executer_demandes(&ctx.pool).await {
            tracing::warn!(erreur = ?e, "relances demandées impossibles");
        }
        if tokio::time::Instant::now() >= prochain {
            prochain = tokio::time::Instant::now() + PERIODE;
            if let Err(e) = tour(&ctx.pool, &mut m).await {
                tracing::warn!(erreur = ?e, "tour de l'orchestre impossible");
            }
        }
        tokio::time::sleep(SCRUTATION).await;
    }
}

fn rfc(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Exécute les relances demandées. Rend `(étape, éléments relancés)`.
pub async fn executer_demandes(pool: &PgPool) -> anyhow::Result<Vec<(String, usize)>> {
    let mut faites = vec![];
    for etape in traitements::prendre_demandes(pool).await? {
        let n = relancer(pool, &etape).await?;
        journal::ecrire(
            pool,
            None,
            "traitement_relance",
            "traitement",
            Some(etape.clone()),
            json!({ "relances": n }),
        )
        .await?;
        tracing::info!(etape, relances = n, "étape relancée à la demande");
        faites.push((etape, n));
    }
    Ok(faites)
}

/// Relance une étape tout de suite (sans attendre le prochain tour du planificateur).
pub async fn relancer(pool: &PgPool, etape: &str) -> anyhow::Result<usize> {
    Ok(match etape {
        "releve" => {
            let boites = sqlx::query_scalar!(
                "select id from boite_mail where active and not pause_auto order by id"
            )
            .fetch_all(pool)
            .await?;
            for id in &boites {
                file::pousser_releve(pool, *id).await?;
            }
            boites.len()
        }
        "lecture" => {
            sqlx::query!("update document set planifie_le = null where etat = 'a_lire'")
                .execute(pool)
                .await?;
            crate::lecture::pompe(pool).await?
        }
        "tri" => {
            sqlx::query!("update mail set tri_planifie_le = null where tri_etat = 'a_trier'")
                .execute(pool)
                .await?;
            crate::tri::pompe(pool).await?
        }
        "rapprochement" => {
            let fournisseurs = sqlx::query_scalar!(
                "select distinct fournisseur_id as \"id!\" from document
                 where fournisseur_id is not null and etat = 'lu' and type in ('facture', 'avoir', 'bl')"
            )
            .fetch_all(pool)
            .await?;
            let mut n = 0;
            for id in fournisseurs {
                n += gmflow_core::rapprochement::recalculer_fournisseur(pool, id).await?;
            }
            n
        }
        "depot" => {
            let ids = traitements::documents_a_deposer(pool).await?;
            for id in &ids {
                file::pousser_envoi(
                    pool,
                    file::TacheEnvoi::Deposer {
                        document_id: *id,
                        forcer: false,
                        auteur_id: None,
                    },
                )
                .await?;
            }
            ids.len()
        }
        _ => 0,
    })
}

/// Un tour de surveillance. Rend l'état écrit dans `orchestre_etat`.
pub async fn tour(pool: &PgPool, m: &mut Memoire) -> anyhow::Result<Value> {
    let maintenant = OffsetDateTime::now_utc();

    // Files : battement de chaque file, changements journalisés.
    let battements = traitements::battements(pool).await?;
    let mut files = serde_json::Map::new();
    for f in FILES {
        let dernier = battements.get(f).copied();
        let vivante = dernier.is_some_and(|t| maintenant - t <= BATTEMENT_FILE_MAX);
        let avant = m.files.insert(f, vivante).unwrap_or(true);
        if avant != vivante {
            let action = if vivante {
                "file_repartie"
            } else {
                "file_arretee"
            };
            journal::ecrire(
                pool,
                None,
                action,
                "file",
                Some(f.to_string()),
                json!({ "battement": dernier.map(rfc) }),
            )
            .await?;
            if vivante {
                tracing::info!(file = f, "file repartie");
            } else {
                tracing::error!(file = f, battement = ?dernier.map(rfc), "file arrêtée : plus de battement");
            }
        }
        files.insert(
            f.to_string(),
            json!({ "battement": dernier.map(rfc), "vivante": vivante }),
        );
    }

    // Lectures et tris restés en cours : remis en attente.
    let docs = sqlx::query_scalar!(
        "update document set etat = 'a_lire', pris_le = null, planifie_le = null
         where etat = 'en_cours' and pris_le < now() - interval '30 minutes'
         returning id"
    )
    .fetch_all(pool)
    .await?;
    if !docs.is_empty() {
        journal::ecrire(
            pool,
            None,
            "lecture_debloquee",
            "document",
            None,
            json!({ "documents": docs }),
        )
        .await?;
        tracing::warn!(documents = ?docs, "lectures bloquées remises en attente");
    }
    let mails = sqlx::query_scalar!(
        "update mail set tri_etat = 'a_trier', tri_pris_le = null, tri_planifie_le = null
         where tri_etat = 'en_cours' and tri_pris_le < now() - interval '30 minutes'
         returning id"
    )
    .fetch_all(pool)
    .await?;
    if !mails.is_empty() {
        journal::ecrire(
            pool,
            None,
            "tri_debloque",
            "mail",
            None,
            json!({ "mails": mails }),
        )
        .await?;
        tracing::warn!(mails = ?mails, "tris bloqués remis en attente");
    }

    // Documents qui attendent depuis plus de 15 minutes : la raison sur la fiche.
    let notes = diagnostiquer(pool).await?;

    // Claude.
    let r = reglages::lire(pool).await?;
    let claude_lecture = attente::lecture_indisponible(pool, &r, false)
        .await?
        .map(|i| i.message);
    let claude_tri = attente::tri_indisponible(pool, &r)
        .await?
        .map(|i| i.message);
    if m.claude.as_ref() != Some(&claude_lecture) {
        match &claude_lecture {
            Some(msg) => tracing::warn!(raison = %msg, "lecture par Claude indisponible"),
            None if m.claude.is_some() => tracing::info!("lecture par Claude de nouveau possible"),
            None => {}
        }
        m.claude = Some(claude_lecture.clone());
    }

    // Compteurs : documents à lire et mails à trier.
    let lecture = sqlx::query!(
        r#"select count(*) as "en_attente!",
                  count(*) filter (where greatest(cree_le, coalesce(lu_le, cree_le))
                                         < now() - make_interval(mins => $1)) as "en_retard!",
                  min(greatest(cree_le, coalesce(lu_le, cree_le))) as plus_vieux
           from document where etat = 'a_lire'"#,
        RETARD_MIN as i32
    )
    .fetch_one(pool)
    .await?;
    let tri = sqlx::query!(
        r#"select count(*) as "en_attente!",
                  count(*) filter (where recu_le < now() - make_interval(mins => $1)) as "en_retard!",
                  min(recu_le) as plus_vieux
           from mail where tri_etat = 'a_trier'"#,
        RETARD_MIN as i32
    )
    .fetch_one(pool)
    .await?;
    if (lecture.en_retard > 0) != m.retard_lecture {
        m.retard_lecture = lecture.en_retard > 0;
        if m.retard_lecture {
            tracing::warn!(
                documents = lecture.en_retard,
                "documents en attente depuis plus de 15 minutes"
            );
        } else {
            tracing::info!("plus aucun document en retard");
        }
    }
    if (tri.en_retard > 0) != m.retard_tri {
        m.retard_tri = tri.en_retard > 0;
        if m.retard_tri {
            tracing::warn!(
                mails = tri.en_retard,
                "mails à trier depuis plus de 15 minutes"
            );
        } else {
            tracing::info!("plus aucun mail en retard de tri");
        }
    }

    let etat = json!({
        "vu_le": rfc(maintenant),
        "files": files,
        "claude": { "lecture": claude_lecture, "tri": claude_tri },
        "lecture": {
            "en_attente": lecture.en_attente,
            "en_retard": lecture.en_retard,
            "plus_vieux": lecture.plus_vieux.map(rfc),
            "debloques": docs.len(),
            "raisons_notees": notes,
        },
        "tri": {
            "en_attente": tri.en_attente,
            "en_retard": tri.en_retard,
            "plus_vieux": tri.plus_vieux.map(rfc),
            "debloques": mails.len(),
        },
    });
    parametre::ecrire(pool, traitements::ORCHESTRE_ETAT, &etat).await?;
    Ok(etat)
}

/// Écrit la raison du blocage sur les documents qui attendent depuis plus de 15 minutes (les
/// plus anciens d'abord). Une attente ordinaire dans la file n'efface pas le dernier essai
/// noté. Rend le nombre de documents mis à jour.
pub async fn diagnostiquer(pool: &PgPool) -> anyhow::Result<usize> {
    let ids = sqlx::query_scalar!(
        "select id from document
         where etat = 'a_lire'
           and greatest(cree_le, coalesce(lu_le, cree_le)) < now() - make_interval(mins => $1)
         order by greatest(cree_le, coalesce(lu_le, cree_le)), id
         limit $2",
        RETARD_MIN as i32,
        DIAGNOSTICS_PAR_TOUR
    )
    .fetch_all(pool)
    .await?;
    let mut n = 0;
    for id in ids {
        let Some(a) = attente::expliquer(pool, id).await? else {
            continue;
        };
        if !GENRES_NOTES.contains(&a.genre) {
            continue;
        }
        n += sqlx::query!(
            "update document set message = $2
             where id = $1 and etat = 'a_lire' and message is distinct from $2",
            id,
            a.raison
        )
        .execute(pool)
        .await?
        .rows_affected() as usize;
    }
    Ok(n)
}
