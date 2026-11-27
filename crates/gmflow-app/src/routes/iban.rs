//! IBAN connus des fournisseurs (fraude au RIB) : liste sur la fiche fournisseur (masqués),
//! « C'est le bon IBAN » sur une facture en alerte et retrait d'un IBAN connu (administrateur,
//! journalisés). Règles dans `gmflow_core::iban`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::{depot, iban, journal};
use serde::Serialize;
use serde_json::json;

use super::documents::controler_visible;
use super::fournisseurs::fournisseur_visible;
use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

#[derive(Serialize)]
pub struct IbanConnu {
    id: i64,
    /// Masqué au milieu (« FR76…1234 ») : l'IBAN entier reste sur la facture.
    iban: String,
    pays: String,
    bic: Option<String>,
    /// `validation` (première facture validée) ou `confirmation` (« C'est le bon IBAN »).
    origine: String,
    document_id: Option<i64>,
    #[serde(with = "date_iso::option")]
    premiere_vue_le: Option<time::Date>,
    #[serde(with = "time::serde::rfc3339")]
    ajoute_le: time::OffsetDateTime,
    ajoute_par: Option<String>,
    /// Connu au niveau du groupe de cette marque (et non de la fiche elle-même).
    du_groupe: bool,
}

/// IBAN connus d'un fournisseur : ceux de sa fiche et de son groupe.
pub async fn lister(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Vec<IbanConnu>>> {
    if !fournisseur_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let lignes = sqlx::query!(
        r#"select i.id, i.iban, i.bic, i.origine, i.document_id, i.premiere_vue_le, i.ajoute_le,
                  u.identifiant as "ajoute_par?", i.fournisseur_id <> $1 as "du_groupe!"
           from fournisseur_iban i
           left join utilisateur u on u.id = i.ajoute_par
           where i.fournisseur_id = $1
              or i.fournisseur_id = (select groupe_id from fournisseur where id = $1)
           order by i.ajoute_le desc, i.id desc"#,
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(
        lignes
            .into_iter()
            .map(|l| IbanConnu {
                id: l.id,
                pays: iban::nom_pays(&iban::pays(&l.iban)),
                iban: iban::masque(&l.iban),
                bic: l.bic,
                origine: l.origine,
                document_id: l.document_id,
                premiere_vue_le: l.premiere_vue_le,
                ajoute_le: l.ajoute_le,
                ajoute_par: l.ajoute_par,
                du_groupe: l.du_groupe,
            })
            .collect(),
    ))
}

/// Factures pas encore parties de ce fournisseur, de son groupe et de ses marques : contrôles
/// recalculés (sans dépôt automatique) après un changement de ses IBAN connus.
async fn reevaluer_famille(etat: &Etat, fournisseur_id: i64) -> Resultat<()> {
    let ids = sqlx::query_scalar!(
        "with racine as (select coalesce(groupe_id, id) as id from fournisseur where id = $1)
         select d.id from document d, racine r
         where d.statut_depot in ('a_verifier', 'pret', 'doublon')
           and d.fournisseur_id in (select id from fournisseur where id = r.id or groupe_id = r.id)
         order by d.id",
        fournisseur_id
    )
    .fetch_all(&etat.pool)
    .await?;
    for id in ids {
        depot::evaluer(&etat.pool, id)
            .await
            .map_err(ErreurApi::Interne)?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Confirmation {
    /// L'IBAN n'était pas encore connu.
    ajoute: bool,
    statut_depot: Option<String>,
}

/// « C'est le bon IBAN » (administrateur) : l'IBAN de la facture devient connu pour son
/// fournisseur, l'alerte tombe sur cette facture et sur les autres pas encore parties. Pas de
/// dépôt automatique ensuite : la facture se valide à la main.
pub async fn confirmer(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Confirmation>> {
    controler_visible(&etat, &acces, id).await?;
    let mut tx = etat.pool.begin().await?;
    let Some(ajoute) = iban::confirmer(&mut tx, id, acces.utilisateur_id).await? else {
        return Err(ErreurApi::Conflit(
            "ce document n'a pas d'IBAN lu ou pas de fournisseur".into(),
        ));
    };
    // Document déjà parti : ses contrôles ne sont plus recalculés, l'alerte est retirée ici.
    sqlx::query!(
        "update document set controles = $2 where id = $1 and statut_depot in ('en_envoi', 'depose', 'erreur')",
        id,
        iban::sans_alerte(
            &sqlx::query_scalar!("select controles from document where id = $1", id)
                .fetch_one(&mut *tx)
                .await?
        )
    )
    .execute(&mut *tx)
    .await?;
    let fournisseur = sqlx::query_scalar!("select fournisseur_id from document where id = $1", id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(f) = fournisseur {
        reevaluer_famille(&etat, f).await?;
    }
    let statut_depot = sqlx::query_scalar!("select statut_depot from document where id = $1", id)
        .fetch_one(&etat.pool)
        .await?;
    Ok(Json(Confirmation {
        ajoute,
        statut_depot,
    }))
}

/// Retire un IBAN connu (administrateur, journalisé) : les factures suivantes qui le portent
/// seront de nouveau en alerte (ou le feront connaître, s'il n'en reste aucun).
pub async fn retirer(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Path((fournisseur_id, iban_id)): Path<(i64, i64)>,
) -> Resultat<StatusCode> {
    let mut tx = etat.pool.begin().await?;
    let Some(r) = sqlx::query!(
        "delete from fournisseur_iban i
         where i.id = $2
           and (i.fournisseur_id = $1
                or i.fournisseur_id = (select groupe_id from fournisseur where id = $1))
         returning i.fournisseur_id, i.iban",
        fournisseur_id,
        iban_id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Err(ErreurApi::Absent);
    };
    journal::ecrire(
        &mut *tx,
        Some(acces.utilisateur_id),
        "iban_retire",
        "fournisseur",
        Some(r.fournisseur_id.to_string()),
        json!({ "iban": iban::masque(&r.iban) }),
    )
    .await?;
    tx.commit().await?;
    reevaluer_famille(&etat, fournisseur_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
