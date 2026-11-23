//! Écran « Bilan du mois » en un appel : suivi des envois par société, fournisseurs avec des
//! BL du mois encore sans facture, BL lus sans leurs lignes, et, pour la partie repliée, la
//! qualité de la lecture (`/api/documents/bilan`) et du tri (`/api/tri/semaines`).
//!
//! Règle commune de `docs/api-refonte.md` : un responsable voit les documents d'une boîte de sa
//! société, rangés dans sa société ou pas encore rangés ; avec `?societe=`, un document compte
//! pour la société où il est rangé, ou, pas encore rangé, pour la société d'une boîte qui l'a
//! reçu (il est alors compté dans la ligne de cette société).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use super::depots::{LigneSuivi, mois_choisi};
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Deserialize)]
pub struct Filtre {
    /// AAAA-MM ; par défaut, le mois en cours.
    mois: Option<String>,
    societe: Option<i64>,
}

#[derive(Serialize)]
pub struct BlEnAttente {
    societe_id: Option<i64>,
    societe: Option<String>,
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    /// BL datés du mois sans facture rapprochée (ni écartés du rapprochement).
    bl: i64,
}

#[derive(Serialize)]
pub struct Qualite {
    lecture: super::documents::Bilan,
    tri: Vec<super::reponses::Semaine>,
}

#[derive(Serialize)]
pub struct Bilan {
    mois: String,
    /// Une ligne par société ; `societe_id` nul : factures pas encore rangées (sans
    /// `?societe=` chez un administrateur).
    societes: Vec<LigneSuivi>,
    bl_en_attente: Vec<BlEnAttente>,
    /// BL lus avant le rapprochement, sans leurs lignes (tout l'historique, mêmes filtres).
    bl_sans_lignes: i64,
    qualite: Qualite,
}

pub async fn bilan(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<Bilan>> {
    let mois = mois_choisi(f.mois.as_deref())?;
    let debut = time::Date::parse(
        &format!("{mois}-01"),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ErreurApi::Donnees("mois invalide".into()))?;
    let perimetre = acces.societe_filtre();
    // Une autre société que la sienne ne rend rien à un responsable.
    let hors_perimetre = matches!((perimetre, f.societe), (Some(p), Some(s)) if p != s);
    // Société où compter les documents pas encore rangés.
    let cible = f.societe.or(perimetre);
    let (societes, bl_en_attente, bl_sans_lignes) = if hors_perimetre {
        (Vec::new(), Vec::new(), 0)
    } else {
        let societes = sqlx::query_as!(
            LigneSuivi,
            r#"select coalesce(d.societe_id, $3) as societe_id, s.raison_sociale as "societe?",
                      count(*) filter (where d.statut_depot <> 'doublon') as "recues!",
                      count(*) filter (where d.statut_depot = 'depose') as "deposees!",
                      count(*) filter (where d.statut_depot in ('pret', 'en_envoi')) as "pretes!",
                      count(*) filter (where d.statut_depot = 'a_verifier') as "a_verifier!",
                      count(*) filter (where d.statut_depot = 'erreur') as "en_erreur!",
                      count(*) filter (where d.statut_depot = 'doublon') as "doublons!",
                      coalesce(sum(case when d.type = 'avoir' then -abs(d.montant_ttc) else d.montant_ttc end)
                               filter (where d.statut_depot = 'depose'), 0)::float8 as "montant_depose!"
               from document d
               join mail m on m.id = d.mail_id
               left join societe s on s.id = coalesce(d.societe_id, $3)
               where d.statut_depot is not null
                 and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $1::date
                 and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) < $1::date + interval '1 month'
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = d.mail_id and ($2::bigint is null or b.societe_id = $2))
                 and ($2::bigint is null or d.societe_id is null or d.societe_id = $2)
                 and ($4::bigint is null or d.societe_id = $4
                      or (d.societe_id is null
                          and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                      where mb.mail_id = d.mail_id and b.societe_id = $4)))
               group by 1, s.raison_sociale
               order by s.raison_sociale nulls last"#,
            debut,
            perimetre,
            cible,
            f.societe
        )
        .fetch_all(&etat.pool)
        .await?;
        let bl_en_attente = sqlx::query_as!(
            BlEnAttente,
            r#"select coalesce(d.societe_id, $4) as societe_id, s.raison_sociale as "societe?",
                      d.fournisseur_id, coalesce(fo.nom, min(d.fournisseur_nom)) as fournisseur,
                      count(*) as "bl!"
               from document d
               join mail m on m.id = d.mail_id
               left join societe s on s.id = coalesce(d.societe_id, $4)
               left join fournisseur fo on fo.id = d.fournisseur_id
               where d.etat = 'lu' and d.type = 'bl' and not d.rapprochement_ignore
                 and not exists (select 1 from rapprochement r where r.bl_id = d.id and not r.rejete)
                 and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $1::date
                 and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) < $1::date + interval '1 month'
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = d.mail_id and ($2::bigint is null or b.societe_id = $2))
                 and ($2::bigint is null or d.societe_id is null or d.societe_id = $2)
                 and ($3::bigint is null or d.societe_id = $3
                      or (d.societe_id is null
                          and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                      where mb.mail_id = d.mail_id and b.societe_id = $3)))
               group by 1, s.raison_sociale, d.fournisseur_id, fo.nom
               order by s.raison_sociale nulls last, 5 desc, 4"#,
            debut,
            perimetre,
            f.societe,
            cible
        )
        .fetch_all(&etat.pool)
        .await?;
        let bl_sans_lignes = sqlx::query_scalar!(
            r#"select count(*) as "n!" from document d
               where d.type = 'bl' and d.etat = 'lu' and not d.lignes_lues
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
                 and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
                 and ($2::bigint is null or d.societe_id = $2
                      or (d.societe_id is null
                          and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                      where mb.mail_id = d.mail_id and b.societe_id = $2)))"#,
            perimetre,
            f.societe
        )
        .fetch_one(&etat.pool)
        .await?;
        (societes, bl_en_attente, bl_sans_lignes)
    };
    let Json(lecture) = super::documents::bilan(acces.clone(), State(etat.clone())).await?;
    let Json(tri) = super::reponses::semaines(acces, State(etat)).await?;
    Ok(Json(Bilan {
        mois,
        societes,
        bl_en_attente,
        bl_sans_lignes,
        qualite: Qualite { lecture, tri },
    }))
}
