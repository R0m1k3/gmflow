//! Test de connexion d'une boîte : LOGIN puis LIST ; le résultat va dans `dernier_test`.

use gmflow_core::imap;
use serde_json::json;

use crate::Contexte;

pub async fn tester(ctx: &Contexte, boite_id: i64) -> anyhow::Result<()> {
    let b = sqlx::query!(
        "select serveur, port, identifiant, mot_de_passe_chiffre from boite_mail where id = $1",
        boite_id
    )
    .fetch_optional(&ctx.pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("boîte {boite_id} introuvable"))?;

    let resultat = async {
        let mdp = ctx
            .chiffreur
            .dechiffrer_texte(&b.mot_de_passe_chiffre)
            .map_err(|e| e.to_string())?;
        let port = u16::try_from(b.port).map_err(|_| "port invalide".to_string())?;
        let mut s = imap::connecter(&b.serveur, port, &b.identifiant, &mdp, ctx.transport)
            .await
            .map_err(|e| e.to_string())?;
        let dossiers = imap::dossiers(&mut s).await.map_err(|e| e.to_string())?;
        let _ = s.logout().await;
        Ok::<_, String>(dossiers)
    }
    .await;

    let quand =
        time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
    let test = match &resultat {
        Ok(d) => {
            json!({ "quand": quand, "ok": true, "message": format!("{} dossiers", d.len()), "dossiers": d })
        }
        Err(m) => json!({ "quand": quand, "ok": false, "message": m, "dossiers": [] }),
    };
    sqlx::query!(
        "update boite_mail set dernier_test = $2 where id = $1",
        boite_id,
        test
    )
    .execute(&ctx.pool)
    .await?;
    Ok(())
}
