//! Statut de dépôt d'une facture ou d'un avoir : contrôles (module `controles`) et doublon,
//! puis dépôt automatique si la structure l'a activé.

use serde_json::json;
use sqlx::{PgConnection, PgExecutor, PgPool};

use crate::controles::{self, Echec};
use crate::file::{self, TacheEnvoi};

/// Types déposés dans Dext ou Yooz.
pub const TYPES_DEPOSES: &[&str] = &["facture", "avoir"];

/// Statuts qu'une réévaluation ne touche plus (envoi lancé ou fait).
pub const STATUTS_FIGES: &[&str] = &["en_envoi", "depose"];

/// Autre document qui fait de celui-ci un doublon : même fichier, ou même fournisseur, même
/// numéro et même montant TTC (ou TTC manquant d'un côté : lecture incomplète). Un document déjà déposé (ou en cours d'envoi) l'emporte
/// toujours ; sinon, le plus ancien.
pub async fn doublon_de<'e>(
    db: impl PgExecutor<'e>,
    id: i64,
) -> sqlx::Result<Option<(i64, String)>> {
    let r = sqlx::query!(
        r#"select o.id, o.statut_depot as "statut!" from document d, document o
           where d.id = $1 and o.id <> d.id
             and o.statut_depot is not null and o.statut_depot <> 'doublon'
             and (o.statut_depot in ('en_envoi', 'depose') or o.id < d.id)
             and (o.sha256 = d.sha256
                  or (d.fournisseur_id is not null and o.fournisseur_id = d.fournisseur_id
                      and d.numero is not null and lower(trim(o.numero)) = lower(trim(d.numero))
                      and (o.montant_ttc is null or d.montant_ttc is null
                           or o.montant_ttc = d.montant_ttc)))
           order by (o.statut_depot in ('en_envoi', 'depose')) desc, o.id
           limit 1"#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(r.map(|r| (r.id, r.statut)))
}

/// Recalcule le statut de dépôt d'un document et ses contrôles en échec. Rend le statut
/// (rien pour un document qui n'est pas une facture lue).
pub async fn evaluer(pool: &PgPool, id: i64) -> anyhow::Result<Option<String>> {
    let mut conn = pool.acquire().await?;
    evaluer_dans(&mut conn, id).await
}

/// `evaluer` sur une connexion donnée (dans une transaction : validation en un appel).
///
/// Le doublon est toujours le plus récent des deux : quand un document est évalué après un
/// autre plus récent qui lui est identique (lectures en parallèle à la première relève), ce
/// dernier est réévalué pour devenir le doublon.
pub async fn evaluer_dans(conn: &mut PgConnection, id: i64) -> anyhow::Result<Option<String>> {
    let statut = evaluer_seul(conn, id).await?;
    if statut.as_deref().is_some_and(|s| s != "doublon") {
        for plus_recent in identiques_plus_recents(&mut *conn, id).await? {
            evaluer_seul(conn, plus_recent).await?;
        }
    }
    Ok(statut)
}

/// Documents plus récents, identiques à celui-ci (critères de `doublon_de`), pas encore marqués
/// doublons ni envoyés.
async fn identiques_plus_recents<'e>(db: impl PgExecutor<'e>, id: i64) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar!(
        r#"select o.id from document d, document o
           where d.id = $1 and o.id > d.id
             and o.statut_depot is not null and o.statut_depot not in ('doublon', 'en_envoi', 'depose')
             and (o.sha256 = d.sha256
                  or (d.fournisseur_id is not null and o.fournisseur_id = d.fournisseur_id
                      and d.numero is not null and lower(trim(o.numero)) = lower(trim(d.numero))
                      and (o.montant_ttc is null or d.montant_ttc is null
                           or o.montant_ttc = d.montant_ttc)))
           order by o.id"#,
        id
    )
    .fetch_all(db)
    .await
}

async fn evaluer_seul(conn: &mut PgConnection, id: i64) -> anyhow::Result<Option<String>> {
    let Some(d) = sqlx::query!(
        r#"select d.etat, d.type, d.statut_depot, d.societe_id,
                  coalesce((d.confiance->>'societe')::float8, 0) as "conf_societe!",
                  d.verifie_le is not null or d.corrections ? 'societe_id' as "societe_humaine!",
                  f.statut as "fournisseur_statut?",
                  coalesce(d.fournisseur_siret, f.siret) as siret,
                  coalesce(d.fournisseur_tva, f.tva) as tva,
                  d.numero, d.date_document,
                  d.montant_ht::float8 as ht, d.montant_tva::float8 as tva_montant,
                  d.montant_ttc::float8 as ttc,
                  s.adresse_depot as "adresse_depot?"
           from document d
           left join fournisseur f on f.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.id = $1"#,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    // Envoi lancé, fait ou en échec : seul un administrateur relance (« Redéposer »).
    if let Some(s) = &d.statut_depot
        && (STATUTS_FIGES.contains(&s.as_str()) || s == "erreur")
    {
        return Ok(d.statut_depot);
    }
    let est_facture = d.etat == "lu"
        && d.r#type
            .as_deref()
            .is_some_and(|t| TYPES_DEPOSES.contains(&t));
    if !est_facture {
        sqlx::query!(
            "update document set statut_depot = null, controles = '[]' where id = $1",
            id
        )
        .execute(&mut *conn)
        .await?;
        return Ok(None);
    }
    let f = controles::Facture {
        societe_id: d.societe_id,
        confiance_societe: if d.societe_humaine {
            1.0
        } else {
            d.conf_societe
        },
        fournisseur_statut: d.fournisseur_statut.as_deref(),
        fournisseur_siret: d.siret.as_deref(),
        fournisseur_tva: d.tva.as_deref(),
        numero: d.numero.as_deref(),
        date: d.date_document,
        ht: d.ht,
        tva: d.tva_montant,
        ttc: d.ttc,
        adresse_depot: d
            .adresse_depot
            .as_deref()
            .is_some_and(|a| !a.trim().is_empty()),
    };
    let mut echecs = controles::controler(&f, controles::aujourdhui_paris());
    // Le statut est écrit avant la recherche de doublon : un document qui entre dans le
    // circuit compte pour les suivants.
    let doublon = {
        sqlx::query!(
            "update document set statut_depot = coalesce(statut_depot, 'a_verifier') where id = $1",
            id
        )
        .execute(&mut *conn)
        .await?;
        doublon_de(&mut *conn, id).await?
    };
    let statut = if let Some((autre, statut)) = doublon {
        echecs.insert(
            0,
            Echec {
                code: "doublon".into(),
                message: format!(
                    "Même facture que le document {autre} ({})",
                    if STATUTS_FIGES.contains(&statut.as_str()) {
                        "déjà envoyée à Dext/Yooz"
                    } else {
                        "reçue avant"
                    }
                ),
                champs: Vec::new(),
                proposition: None,
            },
        );
        "doublon"
    } else if echecs.is_empty() {
        "pret"
    } else {
        "a_verifier"
    };
    sqlx::query!(
        "update document set statut_depot = $2, controles = $3 where id = $1",
        id,
        statut,
        json!(echecs)
    )
    .execute(&mut *conn)
    .await?;
    Ok(Some(statut.to_string()))
}

/// `evaluer`, puis dépôt automatique d'une facture prête si sa structure l'a activé.
pub async fn evaluer_et_deposer(pool: &PgPool, id: i64) -> anyhow::Result<Option<String>> {
    let statut = evaluer(pool, id).await?;
    if statut.as_deref() == Some("pret") {
        let auto = sqlx::query_scalar!(
            "select s.depot_auto from document d join societe s on s.id = d.societe_id where d.id = $1",
            id
        )
        .fetch_optional(pool)
        .await?
        .unwrap_or(false);
        if auto {
            file::pousser_envoi(
                pool,
                TacheEnvoi::Deposer {
                    document_id: id,
                    forcer: false,
                    auteur_id: None,
                },
            )
            .await?;
        }
    }
    Ok(statut)
}

/// Réévalue les factures pas encore parties d'une structure ou d'un fournisseur (après une
/// modification de leur fiche).
pub async fn reevaluer(
    pool: &PgPool,
    societe_id: Option<i64>,
    fournisseur_id: Option<i64>,
) -> anyhow::Result<usize> {
    let ids = sqlx::query_scalar!(
        "select id from document
         where statut_depot in ('a_verifier', 'pret', 'doublon')
           and ($1::bigint is null or societe_id = $1)
           and ($2::bigint is null or fournisseur_id = $2)
         order by id",
        societe_id,
        fournisseur_id
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        evaluer_et_deposer(pool, *id).await?;
    }
    Ok(ids.len())
}

/// Version des phrases des contrôles enregistrées dans `document.controles` (2 : vocabulaire
/// de l'interface, montants à la française, champs visés et correction proposée).
pub const VERSION_CONTROLES: i64 = 2;
const CLE_VERSION_CONTROLES: &str = "controles_version";

/// Au démarrage de l'app : si les phrases des contrôles ont changé, les factures pas encore
/// parties sont réévaluées une fois (sans dépôt automatique). Rend le nombre de documents revus.
pub async fn mettre_a_jour_controles(pool: &PgPool) -> anyhow::Result<usize> {
    let version = crate::parametre::lire(pool, CLE_VERSION_CONTROLES)
        .await?
        .and_then(|v| v.as_i64())
        .unwrap_or(1);
    if version >= VERSION_CONTROLES {
        return Ok(0);
    }
    let ids = sqlx::query_scalar!(
        "select id from document where statut_depot in ('a_verifier', 'pret', 'doublon') order by id"
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        evaluer(pool, *id).await?;
    }
    crate::parametre::ecrire(pool, CLE_VERSION_CONTROLES, &json!(VERSION_CONTROLES)).await?;
    Ok(ids.len())
}
