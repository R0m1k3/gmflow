//! Écran « Bilan du mois » en un appel : suivi des envois par société (`/api/depots/suivi`),
//! fournisseurs avec des BL du mois encore sans facture, et, pour la partie repliée, la
//! qualité de la lecture (`/api/documents/bilan`) et du tri (`/api/tri/semaines`).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use super::depots::{self, LigneSuivi};
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
    /// Une ligne par société (dont « sans société » : `societe_id` nul), comme le suivi.
    societes: Vec<LigneSuivi>,
    bl_en_attente: Vec<BlEnAttente>,
    qualite: Qualite,
}

pub async fn bilan(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<Bilan>> {
    let Json(suivi) = depots::suivi(
        acces.clone(),
        State(etat.clone()),
        Query(depots::Filtre { mois: f.mois }),
    )
    .await?;
    let societes: Vec<LigneSuivi> = suivi
        .lignes
        .into_iter()
        .filter(|l| f.societe.is_none() || l.societe_id == f.societe)
        .collect();
    let debut = time::Date::parse(
        &format!("{}-01", suivi.mois),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ErreurApi::Donnees("mois invalide".into()))?;
    let bl_en_attente = sqlx::query_as!(
        BlEnAttente,
        r#"select d.societe_id, s.raison_sociale as "societe?", d.fournisseur_id,
                  coalesce(fo.nom, min(d.fournisseur_nom)) as fournisseur, count(*) as "bl!"
           from document d
           join mail m on m.id = d.mail_id
           left join societe s on s.id = d.societe_id
           left join fournisseur fo on fo.id = d.fournisseur_id
           where d.etat = 'lu' and d.type = 'bl' and not d.rapprochement_ignore
             and not exists (select 1 from rapprochement r where r.bl_id = d.id and not r.rejete)
             and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $1::date
             and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) < $1::date + interval '1 month'
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($2::bigint is null or b.societe_id = $2))
             and ($2::bigint is null or d.societe_id is null or d.societe_id = $2)
             and ($3::bigint is null or d.societe_id = $3)
           group by d.societe_id, s.raison_sociale, d.fournisseur_id, fo.nom
           order by s.raison_sociale nulls last, 5 desc, 4"#,
        debut,
        acces.societe_filtre(),
        f.societe
    )
    .fetch_all(&etat.pool)
    .await?;
    let Json(lecture) = super::documents::bilan(acces.clone(), State(etat.clone())).await?;
    let Json(tri) = super::reponses::semaines(acces, State(etat)).await?;
    Ok(Json(Bilan {
        mois: suivi.mois,
        societes,
        bl_en_attente,
        qualite: Qualite { lecture, tri },
    }))
}
