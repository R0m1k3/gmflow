use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::acces::AccesAdmin;
use crate::erreur::Resultat;
use crate::etat::Etat;
use crate::routes::mails::{PAR_PAGE, Page, decalage};

#[derive(Deserialize)]
pub struct Filtres {
    objet_type: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct Ligne {
    #[serde(with = "time::serde::rfc3339")]
    quand: time::OffsetDateTime,
    auteur: Option<String>,
    action: String,
    objet_type: String,
    objet_id: Option<String>,
    details: Value,
}

pub async fn lister(_a: AccesAdmin, State(etat): State<Etat>, Query(f): Query<Filtres>) -> Resultat<Json<Page<Ligne>>> {
    let (page, decalage) = decalage(f.page);
    let objet = f.objet_type.filter(|s| !s.is_empty());
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from journal where ($1::text is null or objet_type = $1)"#,
        objet
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        Ligne,
        r#"select j.quand, u.identifiant as "auteur?", j.action, j.objet_type, j.objet_id, j.details
           from journal j left join utilisateur u on u.id = j.auteur_id
           where ($1::text is null or j.objet_type = $1)
           order by j.quand desc, j.id desc
           limit $2 offset $3"#,
        objet,
        PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Page { total, page, elements }))
}
