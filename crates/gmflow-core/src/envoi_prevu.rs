//! Envoi à Dext/Yooz après un délai d'annulation (audit U1, U2). « Valider » une facture ou un
//! avoir ne l'envoie plus tout de suite : une ligne `envoi_prevu` (état `attente`) note l'envoi
//! pour dans [`DELAI_ANNULATION_S`] secondes, avec l'état de la pièce juste avant la validation.
//! La tâche de dépôt est poussée aussitôt ; le worker attend l'heure prévue, puis prend l'envoi.
//!
//! Course entre l'annulation et le worker : les deux verrouillent d'abord la ligne du document
//! (`for update`), puis font passer la ligne `envoi_prevu` de `attente` à leur état par un
//! `update … where etat = 'attente' returning`, dans la même transaction. PostgreSQL les met
//! l'un derrière l'autre ; le second voit l'état écrit par le premier et ne trouve plus de ligne
//! en `attente`. Soit l'annulation gagne (rien ne part, la pièce retrouve son état d'avant), soit
//! le worker gagne (statut `en_envoi` posé dans la même transaction, l'annulation répond « déjà
//! partie »).

use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

use crate::file::{self, TacheEnvoi};
use crate::journal;

/// Délai entre la validation et l'envoi, pendant lequel « Annuler » remet la pièce à contrôler.
/// Renvoyé à l'interface (compte à rebours du message).
pub const DELAI_ANNULATION_S: i64 = 10;

/// Envoi en attente depuis plus longtemps que ça après son heure, sans tâche en file (worker
/// arrêté pendant l'attente, tâche perdue) : le planificateur repousse sa tâche.
const RELANCE_APRES_S: i32 = 120;

/// Le worker n'attend jamais plus que ça une heure prévue (horloge, délai changé entre deux
/// versions).
pub const ATTENTE_MAX_S: i64 = DELAI_ANNULATION_S + 5;

/// Champs du document que l'annulation remet : ceux qu'une validation change (corrections
/// envoyées avec elle comprises).
pub async fn photo_avant(c: &mut PgConnection, document_id: i64) -> sqlx::Result<Value> {
    sqlx::query_scalar!(
        r#"select to_jsonb(x) as "v!" from (
             select type, societe_id, fournisseur_id, numero, date_document, numero_commande,
                    montant_ht, montant_tva, montant_ttc, date_echeance, corrections, corrige_le,
                    type_corrige, societe_corrigee, verifie_le, verifie_par, valide_le, valide_par,
                    statut_depot, controles
             from document where id = $1) x"#,
        document_id
    )
    .fetch_one(&mut *c)
    .await
}

#[derive(Debug, Clone)]
pub struct Prevu {
    pub id: i64,
    pub prevu_pour: time::OffsetDateTime,
}

/// Note l'envoi d'un document validé, pour dans `delai_s` secondes (dans la transaction de la
/// validation).
pub async fn prevoir(
    c: &mut PgConnection,
    document_id: i64,
    lot: Uuid,
    auteur_id: Option<i64>,
    forcer: bool,
    avant: &Value,
    delai_s: i64,
) -> sqlx::Result<Prevu> {
    let r = sqlx::query!(
        "insert into envoi_prevu (document_id, lot, auteur_id, forcer, avant, prevu_pour)
         values ($1, $2, $3, $4, $5, now() + make_interval(secs => $6::float8))
         returning id, prevu_pour",
        document_id,
        lot,
        auteur_id,
        forcer,
        avant,
        delai_s as f64
    )
    .fetch_one(&mut *c)
    .await?;
    Ok(Prevu {
        id: r.id,
        prevu_pour: r.prevu_pour,
    })
}

/// Pousse la tâche de dépôt d'un envoi prévu (après la validation, hors transaction : apalis).
pub async fn pousser(pool: &PgPool, prevu_id: i64) -> anyhow::Result<()> {
    let p = sqlx::query!(
        "select document_id, forcer, auteur_id from envoi_prevu where id = $1",
        prevu_id
    )
    .fetch_one(pool)
    .await?;
    file::pousser_envoi(
        pool,
        TacheEnvoi::DeposerPrevu {
            prevu_id,
            document_id: p.document_id,
            forcer: p.forcer,
            auteur_id: p.auteur_id,
        },
    )
    .await
}

/// Worker : prend l'envoi, document déjà verrouillé par l'appelant. Faux : annulé, déjà pris
/// ou d'un autre document ; rien ne doit partir.
pub async fn prendre(c: &mut PgConnection, prevu_id: i64, document_id: i64) -> sqlx::Result<bool> {
    let r = sqlx::query_scalar!(
        "update envoi_prevu set etat = 'pris', fini_le = now()
         where id = $1 and document_id = $2 and etat = 'attente' returning id",
        prevu_id,
        document_id
    )
    .fetch_optional(&mut *c)
    .await?;
    Ok(r.is_some())
}

/// Worker : envoi pris, mais rien n'est parti (doublon, adresse manquante, pièce changée).
pub async fn sans_suite(c: &mut PgConnection, prevu_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "update envoi_prevu set etat = 'sans_suite' where id = $1 and etat = 'pris'",
        prevu_id
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Un envoi prévu attend pour ce document : un autre dépôt (relance, dépôt automatique) ne
/// passe pas devant lui ni ne contourne son annulation.
pub async fn en_attente<'e>(db: impl PgExecutor<'e>, document_id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"select exists (select 1 from envoi_prevu
                          where document_id = $1 and etat = 'attente') as "e!""#,
        document_id
    )
    .fetch_one(db)
    .await
}

/// Une validation de ce document a été annulée par un humain : le dépôt automatique ne l'envoie
/// plus sans une nouvelle validation.
pub async fn annulee<'e>(db: impl PgExecutor<'e>, document_id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"select exists (select 1 from envoi_prevu
                          where document_id = $1 and etat = 'annule') as "e!""#,
        document_id
    )
    .fetch_one(db)
    .await
}

/// Secondes à attendre avant l'heure prévue (0 si passée), bornées par [`ATTENTE_MAX_S`].
/// `None` : envoi inconnu.
pub async fn attente_restante(pool: &PgPool, prevu_id: i64) -> sqlx::Result<Option<f64>> {
    let s = sqlx::query_scalar!(
        r#"select greatest(extract(epoch from prevu_pour - now()), 0)::float8 as "s!"
           from envoi_prevu where id = $1"#,
        prevu_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(s.map(|s| s.min(ATTENTE_MAX_S as f64)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Issue {
    /// Validation annulée : rien n'est parti, la pièce est de nouveau à contrôler.
    Annulee,
    /// Trop tard : le worker a pris l'envoi (envoyée ou en cours d'envoi).
    DejaPartie,
    /// Rien à annuler (déjà annulée, ou envoi abandonné par le worker).
    RienAAnnuler,
}

/// Annule un envoi prévu s'il n'est pas encore pris : la pièce retrouve exactement les champs
/// notés avant la validation (alertes et statut compris), l'annulation est journalisée.
pub async fn annuler(
    pool: &PgPool,
    prevu_id: i64,
    auteur_id: Option<i64>,
) -> anyhow::Result<Issue> {
    let mut tx = pool.begin().await?;
    let Some(document_id) = sqlx::query_scalar!(
        "select document_id from envoi_prevu where id = $1",
        prevu_id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(Issue::RienAAnnuler);
    };
    // Même ordre de verrous que le worker : le document d'abord.
    sqlx::query!(
        "select id from document where id = $1 for update",
        document_id
    )
    .fetch_one(&mut *tx)
    .await?;
    let avant = sqlx::query_scalar!(
        "update envoi_prevu set etat = 'annule', fini_le = now(), annule_par = $2
         where id = $1 and etat = 'attente' returning avant",
        prevu_id,
        auteur_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(avant) = avant else {
        let etat = sqlx::query_scalar!("select etat from envoi_prevu where id = $1", prevu_id)
            .fetch_one(&mut *tx)
            .await?;
        return Ok(if etat == "pris" {
            Issue::DejaPartie
        } else {
            Issue::RienAAnnuler
        });
    };
    sqlx::query!(
        "update document d set type = a.type, societe_id = a.societe_id,
                fournisseur_id = a.fournisseur_id, numero = a.numero,
                date_document = a.date_document, numero_commande = a.numero_commande,
                montant_ht = a.montant_ht, montant_tva = a.montant_tva, montant_ttc = a.montant_ttc,
                date_echeance = a.date_echeance, corrections = coalesce(a.corrections, '{}'),
                corrige_le = a.corrige_le, type_corrige = coalesce(a.type_corrige, false),
                societe_corrigee = coalesce(a.societe_corrigee, false),
                verifie_le = a.verifie_le, verifie_par = a.verifie_par,
                valide_le = a.valide_le, valide_par = a.valide_par,
                statut_depot = a.statut_depot, controles = coalesce(a.controles, '[]')
         from jsonb_populate_record(null::document, $2) a
         where d.id = $1",
        document_id,
        avant
    )
    .execute(&mut *tx)
    .await?;
    // Premier IBAN du fournisseur appris par cette validation (`iban::retenir_premier`, dans
    // la même transaction : même `now()` que `cree_le`) : il n'est plus « connu ». Un IBAN
    // appris avant (vérification plus ancienne) ou confirmé par un administrateur reste.
    let ibans_oublies = sqlx::query_scalar!(
        "delete from fournisseur_iban i using envoi_prevu p
         where p.id = $1 and i.document_id = $2 and i.origine = 'validation'
           and i.ajoute_le >= p.cree_le
         returning i.iban",
        prevu_id,
        document_id
    )
    .fetch_all(&mut *tx)
    .await?;
    let ibans_oublies: Vec<String> = ibans_oublies
        .iter()
        .map(|i| crate::iban::masque(i))
        .collect();
    journal::ecrire(
        &mut *tx,
        auteur_id,
        "validation_annulee",
        "document",
        Some(document_id.to_string()),
        json!({ "envoi_prevu": prevu_id, "ibans_oublies": ibans_oublies }),
    )
    .await?;
    tx.commit().await?;
    // Corrections défaites (n° de commande, fournisseur…) : liens et écarts d'avant.
    crate::rapprochement::recalculer(pool, document_id).await?;
    Ok(Issue::Annulee)
}

/// Envois d'un lot : `(id de l'envoi prévu, document)`.
pub async fn du_lot(pool: &PgPool, lot: Uuid) -> sqlx::Result<Vec<(i64, i64)>> {
    let r = sqlx::query!(
        "select id, document_id from envoi_prevu where lot = $1 order by id",
        lot
    )
    .fetch_all(pool)
    .await?;
    Ok(r.into_iter().map(|r| (r.id, r.document_id)).collect())
}

/// Planificateur : envois en attente bien après leur heure, sans tâche en file (worker arrêté
/// pendant l'attente, tâche perdue) : leur tâche est poussée de nouveau. Sans risque de double
/// envoi : un seul `prendre` réussit. Rend le nombre relancé.
pub async fn relancer_en_retard(pool: &PgPool) -> anyhow::Result<usize> {
    let ids = sqlx::query_scalar!(
        r#"select p.id from envoi_prevu p
           where p.etat = 'attente' and p.prevu_pour < now() - make_interval(secs => $1::float8)
             and not exists (
                 select 1 from apalis.jobs j
                 where j.job_type = $2 and j.status in ('Pending', 'Queued', 'Running')
                   and (convert_from(j.job, 'UTF8')::jsonb #>> '{DeposerPrevu,prevu_id}')::bigint = p.id)
           order by p.id"#,
        RELANCE_APRES_S as f64,
        file::FILE_ENVOI
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        pousser(pool, *id).await?;
    }
    Ok(ids.len())
}
