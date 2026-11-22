//! Suivi des dépôts par structure et par mois (critère du lot 3 : un mois de factures déposé
//! sans oubli ni doublon).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Deserialize, Default)]
pub struct Filtre {
    /// AAAA-MM ; par défaut, le mois en cours.
    pub(crate) mois: Option<String>,
}

#[derive(Serialize)]
pub struct LigneSuivi {
    pub(crate) societe_id: Option<i64>,
    societe: Option<String>,
    /// Factures et avoirs reçus (date du document, à défaut date du mail) ce mois-là.
    recues: i64,
    deposees: i64,
    pretes: i64,
    a_verifier: i64,
    en_erreur: i64,
    doublons: i64,
    /// Montant TTC déposé.
    montant_depose: f64,
}

#[derive(Serialize)]
pub struct Suivi {
    pub(crate) mois: String,
    pub(crate) lignes: Vec<LigneSuivi>,
}

pub async fn suivi(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<Suivi>> {
    let mois = match f.mois.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => {
            let ok = m.len() == 7
                && m[..4].chars().all(|c| c.is_ascii_digit())
                && &m[4..5] == "-"
                && m[5..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n));
            if !ok {
                return Err(ErreurApi::Donnees("mois : AAAA-MM".into()));
            }
            m.to_string()
        }
        None => {
            let d = gmflow_core::controles::aujourdhui_paris();
            format!("{:04}-{:02}", d.year(), u8::from(d.month()))
        }
    };
    let debut = format!("{mois}-01");
    let lignes = sqlx::query_as!(
        LigneSuivi,
        r#"select d.societe_id, s.raison_sociale as "societe?",
                  count(*) filter (where d.statut_depot <> 'doublon') as "recues!",
                  count(*) filter (where d.statut_depot = 'depose') as "deposees!",
                  count(*) filter (where d.statut_depot in ('pret', 'en_envoi')) as "pretes!",
                  count(*) filter (where d.statut_depot = 'a_verifier') as "a_verifier!",
                  count(*) filter (where d.statut_depot = 'erreur') as "en_erreur!",
                  count(*) filter (where d.statut_depot = 'doublon') as "doublons!",
                  coalesce(sum(d.montant_ttc) filter (where d.statut_depot = 'depose'), 0)::float8 as "montant_depose!"
           from document d
           join mail m on m.id = d.mail_id
           left join societe s on s.id = d.societe_id
           where d.statut_depot is not null
             and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $1::date
             and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) < $1::date + interval '1 month'
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($2::bigint is null or b.societe_id = $2))
           group by d.societe_id, s.raison_sociale
           order by s.raison_sociale nulls last"#,
        time::Date::parse(&debut, time::macros::format_description!("[year]-[month]-[day]"))
            .map_err(|_| ErreurApi::Donnees("mois invalide".into()))?,
        acces.societe_filtre()
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Suivi { mois, lignes }))
}
