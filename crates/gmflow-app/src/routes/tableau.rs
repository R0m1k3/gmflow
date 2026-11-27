use axum::Json;
use axum::extract::{Query, State};
use serde_json::{Value, json};

use crate::acces::Acces;
use crate::erreur::Resultat;
use crate::etat::Etat;

/// BL sans facture depuis ce nombre de jours : signalé sur l'accueil.
pub const JOURS_BL_SANS_FACTURE: i32 = 7;

#[derive(serde::Deserialize)]
pub struct Filtre {
    societe: Option<i64>,
}

pub async fn tableau(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<Value>> {
    let filtre = acces.societe_filtre();
    // Travail de l'accueil (U5), même périmètre que la file « À valider » (`a_traiter`) :
    // montant TTC des factures à contrôler, factures à payer bientôt, BL restés sans facture.
    let travail = sqlx::query!(
        r#"select coalesce(sum(d.montant_ttc) filter (where d.type = 'facture' and a.a_valider), 0)::float8 as "factures_ttc!",
                  count(*) filter (where d.type = 'facture' and a.a_valider
                                     and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $3::int) as "echeances!",
                  count(*) filter (where d.type = 'bl' and not d.rapprochement_ignore
                                     and coalesce(m.date_mail, m.recu_le) <= now() - make_interval(days => $4::int)
                                     and not exists (select 1 from rapprochement r where r.bl_id = d.id and not r.rejete)) as "bl_sans_facture!"
           from document d
           join mail m on m.id = d.mail_id
           cross join lateral (
               select d.type in ('facture', 'avoir', 'bl')
                      and ((d.verifie_le is null
                            and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                           or (d.verifie_le is not null and d.valide_le is null
                               and d.statut_depot in ('a_verifier', 'pret'))) as a_valider
           ) a
           where d.etat = 'lu'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))"#,
        filtre,
        f.societe,
        super::a_traiter::JOURS_ECHEANCE,
        JOURS_BL_SANS_FACTURE
    )
    .fetch_one(&etat.pool)
    .await?;
    let du_jour = sqlx::query!(
        r#"select s.id as societe_id, s.raison_sociale,
               (select count(distinct mb.mail_id) from mail_boite_vue mb
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
    let docs = sqlx::query!(
        r#"select count(*) filter (where etat = 'a_lire') as "a_lire!",
                  count(*) filter (where etat = 'erreur') as "erreur!",
                  count(*) filter (where etat = 'lu' and verifie_le is null) as "a_verifier!",
                  count(*) filter (where etat = 'lu' and lu_le >= (date_trunc('day', now() at time zone 'Europe/Paris') at time zone 'Europe/Paris')) as "lus_aujourdhui!"
           from document d
           where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))"#,
        filtre
    )
    .fetch_one(&etat.pool)
    .await?;
    let depots = sqlx::query!(
        r#"select count(*) filter (where statut_depot = 'a_verifier') as "a_verifier!",
                  count(*) filter (where statut_depot = 'pret') as "prets!",
                  count(*) filter (where statut_depot in ('erreur', 'doublon')) as "bloques!",
                  (select count(*) from envoi_depot e
                    where e.etat = 'envoye'
                      and e.quand >= (date_trunc('month', now() at time zone 'Europe/Paris') at time zone 'Europe/Paris')
                      and ($1::bigint is null or e.societe_id = $1)) as "deposes_mois!"
           from document d
           where d.statut_depot is not null
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))"#,
        filtre
    )
    .fetch_one(&etat.pool)
    .await?;
    let rfc = |t: Option<time::OffsetDateTime>| {
        t.and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
    };
    let mut r = json!({
        "mails_du_jour": du_jour.iter().map(|l| json!({ "societe_id": l.societe_id, "raison_sociale": l.raison_sociale, "n": l.n })).collect::<Vec<_>>(),
        "boites_alerte": alertes.iter().map(|b| json!({
            "id": b.id, "libelle": b.libelle,
            "motif": if b.pause_auto { "pause_auto" } else { "erreur" },
            "depuis": rfc(b.erreur_depuis), "message": b.derniere_erreur,
        })).collect::<Vec<_>>(),
        "a_repondre": super::reponses::compte_a_repondre(&acces, &etat).await?,
        "documents": {
            "a_lire": docs.a_lire, "erreur": docs.erreur, "a_verifier": docs.a_verifier,
            "lus_aujourdhui": docs.lus_aujourdhui,
        },
        "depots": {
            "a_verifier": depots.a_verifier, "prets": depots.prets, "bloques": depots.bloques,
            "deposes_mois": depots.deposes_mois,
        },
        "travail": {
            "factures_ttc": travail.factures_ttc,
            "echeances": travail.echeances,
            "jours_echeance": super::a_traiter::JOURS_ECHEANCE,
            "bl_sans_facture": travail.bl_sans_facture,
            "jours_bl_sans_facture": JOURS_BL_SANS_FACTURE,
        },
    });
    if acces.est_admin() {
        let claude = gmflow_core::parametre::lire(&etat.pool, gmflow_core::parametre::CLAUDE_ETAT)
            .await?
            .unwrap_or_else(|| json!({ "etat": "absent" }));
        let octets =
            sqlx::query_scalar!(r#"select coalesce(sum(taille), 0)::bigint as "n!" from mail"#)
                .fetch_one(&etat.pool)
                .await?;
        r["claude"] = json!({ "etat": claude.get("etat").cloned().unwrap_or(json!("absent")), "teste_le": claude.get("teste_le") });
        let r_lecture = gmflow_core::reglages::lire(&etat.pool).await?;
        r["lecture"] = json!({
            "auto": r_lecture.lecture_auto,
            "quota": r_lecture.quota_lecture_jour,
            "appels_du_jour": gmflow_core::reglages::appels_lecture_du_jour(&etat.pool).await?,
        });
        r["tri"] = json!({
            "auto": r_lecture.tri_auto,
            "quota": r_lecture.quota_tri_jour,
            "appels_du_jour": gmflow_core::reglages::appels_tri_du_jour(&etat.pool).await?,
            "en_attente": sqlx::query_scalar!(
                r#"select count(*) as "n!" from mail where tri_etat in ('a_trier', 'en_cours')"#
            )
            .fetch_one(&etat.pool)
            .await?,
            "en_erreur": sqlx::query_scalar!(
                r#"select count(*) as "n!" from mail where tri_etat = 'erreur'"#
            )
            .fetch_one(&etat.pool)
            .await?,
        });
        r["stockage"] =
            json!({ "octets": octets, "libre_pourcent": libre_pourcent(&etat.config.fichiers) });
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
