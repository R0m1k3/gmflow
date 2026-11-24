//! Serveur du magasin (Apiflow) : essai de connexion et copie des fournisseurs.

use gmflow_core::{articles_magasin, parametre, serveur_api};
use serde_json::json;
use sqlx::PgPool;

use crate::Contexte;

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub async fn tester(ctx: &Contexte) -> anyhow::Result<()> {
    let resultat = match serveur_api::lire(&ctx.pool, &ctx.chiffreur).await? {
        None => Err("adresse du serveur non configurée".to_string()),
        Some((s, jeton)) => serveur_api::essayer(&s.adresse, jeton.as_deref()).await,
    };
    let (ok, message) = match resultat {
        Ok(m) => (true, m),
        Err(m) => (false, m),
    };
    parametre::ecrire(
        &ctx.pool,
        serveur_api::CLE_TEST,
        &json!({ "quand": maintenant(), "ok": ok, "message": message }),
    )
    .await?;
    Ok(())
}

/// Copie les fournisseurs du magasin ; le résultat (réussi ou non) est noté dans `CLE_SYNCHRO`.
pub async fn synchroniser(ctx: &Contexte) -> anyhow::Result<()> {
    let resultat = match serveur_api::lire(&ctx.pool, &ctx.chiffreur).await? {
        None => return Ok(()),
        Some((s, jeton)) => serveur_api::fournisseurs(&s.adresse, jeton.as_deref()).await,
    };
    let v = match resultat {
        Ok(liste) if liste.is_empty() => {
            json!({ "quand": maintenant(), "ok": false, "message": "le serveur n'a renvoyé aucun fournisseur", "nombre": 0 })
        }
        Ok(liste) => {
            serveur_api::enregistrer(&ctx.pool, &liste).await?;
            json!({ "quand": maintenant(), "ok": true, "message": null, "nombre": liste.len() })
        }
        Err(m) => {
            tracing::warn!(erreur = %m, "copie des fournisseurs du magasin impossible");
            json!({ "quand": maintenant(), "ok": false, "message": m, "nombre": null })
        }
    };
    parametre::ecrire(&ctx.pool, serveur_api::CLE_SYNCHRO, &v).await?;
    Ok(())
}

/// Vrai si le serveur est configuré et que la dernière copie (réussie ou non) date de plus de
/// `SYNCHRO_HEURES`.
pub async fn synchro_due(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"select exists (select 1 from parametre where cle = $1)
              and not exists (
                  select 1 from parametre
                  where cle = $2 and (valeur->>'quand')::timestamptz > now() - make_interval(hours => $3)
              ) as "d!""#,
        serveur_api::CLE,
        serveur_api::CLE_SYNCHRO,
        serveur_api::SYNCHRO_HEURES
    )
    .fetch_one(pool)
    .await
}

/// Pousse la recherche des articles d'un BL lu, si le serveur du magasin est configuré.
pub async fn demander_verification_bl(pool: &PgPool, document_id: i64) -> anyhow::Result<()> {
    let bl = sqlx::query_scalar!(
        r#"select exists (select 1 from document d
                          where d.id = $1 and d.type = 'bl' and d.etat = 'lu'
                            and exists (select 1 from ligne_document l where l.document_id = d.id))
              and exists (select 1 from parametre where cle = $2) as "ok!""#,
        document_id,
        serveur_api::CLE
    )
    .fetch_one(pool)
    .await?;
    if bl {
        articles_magasin::demander(pool, document_id).await?;
    }
    Ok(())
}

/// Cherche les articles du BL au magasin ; une erreur (serveur absent, clé refusée) est notée
/// sur le document et affichée sur sa fiche.
pub async fn verifier_bl(ctx: &Contexte, document_id: i64) -> anyhow::Result<()> {
    match serveur_api::lire(&ctx.pool, &ctx.chiffreur).await? {
        None => {
            sqlx::query!(
                "update document set magasin_verifie_le = now(),
                        magasin_erreur = 'adresse du serveur du magasin non configurée'
                 where id = $1",
                document_id
            )
            .execute(&ctx.pool)
            .await?;
            Ok(())
        }
        Some((s, jeton)) => {
            let source = articles_magasin::Http::new(&s.adresse, jeton);
            articles_magasin::verifier_document(&ctx.pool, &source, document_id).await
        }
    }
}
