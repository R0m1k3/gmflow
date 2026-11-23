//! État des files apalis et demandes adressées à l'orchestre du worker. L'app ne parle au
//! worker que par la base : elle lit les battements des files (`apalis.workers`) et l'état
//! écrit par l'orchestre (`parametre.orchestre_etat`), et dépose ses demandes de relance dans
//! `parametre.orchestre_demandes`.

use std::collections::HashMap;

use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::file::{FILE_BOITES, FILE_CLAUDE, FILE_ENVOI, FILE_LECTURE, FILE_REDACTION, FILE_TRI};

/// Les six files du worker.
pub const FILES: [&str; 6] = [
    FILE_BOITES,
    FILE_CLAUDE,
    FILE_LECTURE,
    FILE_ENVOI,
    FILE_TRI,
    FILE_REDACTION,
];

/// Une file bat toutes les 30 s ; sans battement depuis 3 minutes, elle est considérée arrêtée.
pub const BATTEMENT_FILE_MAX: time::Duration = time::Duration::minutes(3);

/// Au-delà de 15 minutes d'attente, un élément est « en retard » et l'orchestre écrit pourquoi.
pub const RETARD_MIN: i64 = 15;

/// État écrit par l'orchestre à chaque tour (`vu_le`, files, Claude, compteurs).
pub const ORCHESTRE_ETAT: &str = "orchestre_etat";
/// Demandes de relance : `{"lecture": "2026-11-23T14:00:00Z", ...}` (étape → demandée le).
pub const ORCHESTRE_DEMANDES: &str = "orchestre_demandes";

/// Étapes du traitement, dans l'ordre du courrier.
pub const ETAPES: [&str; 5] = ["releve", "lecture", "tri", "rapprochement", "depot"];

/// Dernier battement de chaque file (la plus récente de ses incarnations). Une file absente
/// n'a jamais tourné sur cette base.
pub async fn battements(pool: &PgPool) -> sqlx::Result<HashMap<String, OffsetDateTime>> {
    let files: Vec<String> = FILES.iter().map(|f| f.to_string()).collect();
    let lignes = sqlx::query!(
        r#"select worker_type as "file!", max(last_seen) as "dernier!"
           from apalis.workers where worker_type = any($1) group by worker_type"#,
        &files
    )
    .fetch_all(pool)
    .await?;
    Ok(lignes.into_iter().map(|l| (l.file, l.dernier)).collect())
}

/// La file a-t-elle cessé de battre ? `Some(dernier battement)` si oui ; `None` si elle bat
/// ou n'a jamais tourné (base neuve, tests sans worker).
pub async fn file_arretee(pool: &PgPool, file: &str) -> sqlx::Result<Option<OffsetDateTime>> {
    let dernier = sqlx::query_scalar!(
        "select max(last_seen) from apalis.workers where worker_type = $1",
        file
    )
    .fetch_one(pool)
    .await?;
    Ok(dernier.filter(|t| OffsetDateTime::now_utc() - *t > BATTEMENT_FILE_MAX))
}

/// Dépose une demande de relance d'une étape ; l'orchestre la prend dans les secondes qui
/// suivent. Rend l'heure de la demande.
pub async fn demander(pool: &PgPool, etape: &str) -> sqlx::Result<OffsetDateTime> {
    let maintenant = OffsetDateTime::now_utc();
    sqlx::query!(
        "insert into parametre (cle, valeur) values ($1, jsonb_build_object($2::text, $3::timestamptz))
         on conflict (cle) do update
           set valeur = coalesce(parametre.valeur, '{}'::jsonb) || excluded.valeur, modifie_le = now()",
        ORCHESTRE_DEMANDES,
        etape,
        maintenant
    )
    .execute(pool)
    .await?;
    Ok(maintenant)
}

/// Demandes en attente (étape → demandée le), sans les prendre.
pub async fn demandes(pool: &PgPool) -> sqlx::Result<HashMap<String, String>> {
    let v = crate::parametre::lire(pool, ORCHESTRE_DEMANDES).await?;
    Ok(en_carte(v))
}

/// Prend les demandes en attente (elles disparaissent de la base) : étapes connues seulement.
pub async fn prendre_demandes(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    let v = sqlx::query_scalar!(
        "delete from parametre where cle = $1 returning valeur",
        ORCHESTRE_DEMANDES
    )
    .fetch_optional(pool)
    .await?
    .flatten();
    let carte = en_carte(v);
    Ok(ETAPES
        .iter()
        .filter(|e| carte.contains_key(**e))
        .map(|e| e.to_string())
        .collect())
}

fn en_carte(v: Option<Value>) -> HashMap<String, String> {
    match v {
        Some(Value::Object(o)) => o
            .into_iter()
            .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_string()))
            .collect(),
        _ => HashMap::new(),
    }
}

/// Factures et avoirs qui attendent leur dépôt : validés (contrôles passés ou non) ou prêts
/// avec le dépôt automatique de leur structure, sans tâche de dépôt déjà en file. Les dépôts
/// en échec n'y sont pas : ils demandent un humain (vérifier dans Dext ou Yooz).
pub async fn documents_a_deposer(pool: &PgPool) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar!(
        r#"select d.id from document d left join societe s on s.id = d.societe_id
           where d.etat = 'lu'
             and ((d.valide_le is not null and d.statut_depot in ('a_verifier', 'pret'))
                  or (d.valide_le is null and d.statut_depot = 'pret' and coalesce(s.depot_auto, false)))
             and d.id not in (
                 select (convert_from(job, 'UTF8')::jsonb #>> '{Deposer,document_id}')::bigint
                 from apalis.jobs
                 where job_type = $1 and status in ('Pending', 'Queued', 'Running')
                   and convert_from(job, 'UTF8')::jsonb ? 'Deposer')
           order by coalesce(d.valide_le, d.lu_le), d.id"#,
        crate::file::FILE_ENVOI
    )
    .fetch_all(pool)
    .await
}
