use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::acces::Acces;
use crate::erreur::Resultat;
use crate::etat::Etat;

pub async fn tableau(acces: Acces, State(etat): State<Etat>) -> Resultat<Json<Value>> {
    let filtre = acces.societe_filtre();
    let du_jour = sqlx::query!(
        r#"select s.id as societe_id, s.raison_sociale,
               (select count(distinct mb.mail_id) from mail_boite mb
                  join boite_mail b on b.id = mb.boite_id
                  join mail m on m.id = mb.mail_id
                where b.societe_id = s.id
                  and coalesce(m.date_mail, m.recu_le) >= (date_trunc('day', now() at time zone 'Europe/Paris') at time zone 'Europe/Paris')
               ) as "n!"
           from societe s
           where not s.archivee and ($1::bigint is null or s.id = $1)
           order by s.raison_sociale"#,
        filtre
    )
    .fetch_all(&etat.pool)
    .await?;
    let alertes = sqlx::query!(
        r#"select id, libelle, pause_auto, erreur_depuis, derniere_erreur
           from boite_mail
           where ($1::bigint is null or societe_id = $1)
             and (pause_auto or (active and erreur_depuis < now() - interval '30 minutes'))
           order by libelle"#,
        filtre
    )
    .fetch_all(&etat.pool)
    .await?;
    let rfc = |t: Option<time::OffsetDateTime>| {
        t.and_then(|t| t.format(&time::format_description::well_known::Rfc3339).ok())
    };
    let mut r = json!({
        "mails_du_jour": du_jour.iter().map(|l| json!({ "societe_id": l.societe_id, "raison_sociale": l.raison_sociale, "n": l.n })).collect::<Vec<_>>(),
        "boites_alerte": alertes.iter().map(|b| json!({
            "id": b.id, "libelle": b.libelle,
            "motif": if b.pause_auto { "pause_auto" } else { "erreur" },
            "depuis": rfc(b.erreur_depuis), "message": b.derniere_erreur,
        })).collect::<Vec<_>>(),
    });
    if acces.est_admin() {
        let claude = gmflow_core::parametre::lire(&etat.pool, gmflow_core::parametre::CLAUDE_ETAT)
            .await?
            .unwrap_or_else(|| json!({ "etat": "absent" }));
        let octets = sqlx::query_scalar!(r#"select coalesce(sum(taille), 0)::bigint as "n!" from mail"#)
            .fetch_one(&etat.pool)
            .await?;
        r["claude"] = json!({ "etat": claude.get("etat").cloned().unwrap_or(json!("absent")), "teste_le": claude.get("teste_le") });
        r["stockage"] = json!({ "octets": octets, "libre_pourcent": libre_pourcent(&etat.config.fichiers) });
    }
    Ok(Json(r))
}

/// Place libre du volume des fichiers, en pourcentage (None si illisible).
fn libre_pourcent(chemin: &std::path::Path) -> Option<f64> {
    let _ = std::fs::create_dir_all(chemin);
    let s = nix::sys::statvfs::statvfs(chemin).ok()?;
    let total = s.blocks() as f64;
    if total == 0.0 {
        return None;
    }
    Some(((s.blocks_available() as f64 / total) * 1000.0).round() / 10.0)
}
