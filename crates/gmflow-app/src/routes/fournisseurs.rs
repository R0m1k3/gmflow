//! Référentiel des fournisseurs : liste et fiche (tous les comptes, pour corriger un
//! document), création, modification, validation et fusion (administrateur).

use axum::Json;
use axum::extract::{Path, Query, State};
use gmflow_core::controles::{chiffres, siret_valide, tva_compacte, tva_valide};
use gmflow_core::fournisseurs::{domaine, normaliser};
use gmflow_core::{depot, journal};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::mails::{Page, decalage, motif};
use crate::acces::{Acces, AccesAdmin};
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Deserialize)]
pub struct Filtres {
    q: Option<String>,
    statut: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct LigneFournisseur {
    id: i64,
    nom: String,
    siret: Option<String>,
    tva: Option<String>,
    domaines: Vec<String>,
    statut: String,
    societe_defaut_id: Option<i64>,
    societe_defaut: Option<String>,
    nb_documents: i64,
    a_verifier: i64,
}

pub async fn lister(
    _a: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Page<LigneFournisseur>>> {
    let q = motif(&f.q);
    let statut = f.statut.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (page, decalage) = decalage(f.page);
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from fournisseur f
           where f.fusionne_dans is null and ($1::text is null or f.statut = $1)
             and ($2::text is null or f.nom ilike $2 or f.siret ilike $2 or f.tva ilike $2
                  or array_to_string(f.domaines, ' ') ilike $2)"#,
        statut,
        q
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        LigneFournisseur,
        r#"select f.id, f.nom, f.siret, f.tva, f.domaines, f.statut, f.societe_defaut_id,
                  s.raison_sociale as "societe_defaut?",
                  (select count(*) from document d where d.fournisseur_id = f.id) as "nb_documents!",
                  (select count(*) from document d where d.fournisseur_id = f.id
                     and d.statut_depot = 'a_verifier') as "a_verifier!"
           from fournisseur f left join societe s on s.id = f.societe_defaut_id
           where f.fusionne_dans is null and ($1::text is null or f.statut = $1)
             and ($2::text is null or f.nom ilike $2 or f.siret ilike $2 or f.tva ilike $2
                  or array_to_string(f.domaines, ' ') ilike $2)
           order by (f.statut = 'a_confirmer') desc, f.nom
           limit $3 offset $4"#,
        statut,
        q,
        super::mails::PAR_PAGE,
        decalage
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(Page {
        total,
        page,
        elements,
    }))
}

#[derive(Serialize)]
pub struct Fiche {
    id: i64,
    nom: String,
    siret: Option<String>,
    tva: Option<String>,
    domaines: Vec<String>,
    statut: String,
    societe_defaut_id: Option<i64>,
    format_bl: Option<String>,
    notes: Option<String>,
    fusionne_dans: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    cree_le: time::OffsetDateTime,
}

pub async fn fiche(
    _a: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Fiche>> {
    let f = sqlx::query_as!(
        Fiche,
        "select id, nom, siret, tva, domaines, statut, societe_defaut_id, format_bl, notes,
                fusionne_dans, cree_le
         from fournisseur where id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    Ok(Json(f))
}

#[derive(Deserialize, Serialize)]
pub struct Entree {
    nom: String,
    #[serde(default)]
    siret: Option<String>,
    #[serde(default)]
    tva: Option<String>,
    #[serde(default)]
    domaines: Vec<String>,
    #[serde(default = "a_confirmer")]
    statut: String,
    #[serde(default)]
    societe_defaut_id: Option<i64>,
    #[serde(default)]
    format_bl: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

fn a_confirmer() -> String {
    "a_confirmer".into()
}

fn net(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Entree {
    fn nettoyer(mut self) -> Resultat<Self> {
        self.nom = self.nom.trim().to_string();
        if self.nom.is_empty() {
            return Err(ErreurApi::Donnees("nom obligatoire".into()));
        }
        if !["a_confirmer", "valide"].contains(&self.statut.as_str()) {
            return Err(ErreurApi::Donnees("statut : a_confirmer ou valide".into()));
        }
        self.siret = net(self.siret).map(|s| chiffres(&s));
        if let Some(s) = &self.siret
            && !siret_valide(s)
        {
            return Err(ErreurApi::Donnees(format!("SIRET « {s} » invalide")));
        }
        self.tva = net(self.tva).map(|t| tva_compacte(&t));
        if let Some(t) = &self.tva
            && !tva_valide(t)
        {
            return Err(ErreurApi::Donnees(format!("n° de TVA « {t} » invalide")));
        }
        let mut domaines = Vec::new();
        for d in &self.domaines {
            let d = d.trim().trim_start_matches('@').to_lowercase();
            if d.is_empty() {
                continue;
            }
            // « factures@durand.fr » est accepté et réduit à son domaine.
            let d = domaine(&d).unwrap_or(d);
            if !d.contains('.') || d.contains(' ') {
                return Err(ErreurApi::Donnees(format!("domaine « {d} » invalide")));
            }
            if !domaines.contains(&d) {
                domaines.push(d);
            }
        }
        self.domaines = domaines;
        self.format_bl = net(self.format_bl);
        self.notes = net(self.notes);
        Ok(self)
    }
}

pub async fn creer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Fiche>> {
    let e = e.nettoyer()?;
    let mut tx = etat.pool.begin().await?;
    let f = sqlx::query_as!(
        Fiche,
        "insert into fournisseur (nom, nom_normalise, siret, tva, domaines, statut,
                                  societe_defaut_id, format_bl, notes)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         returning id, nom, siret, tva, domaines, statut, societe_defaut_id, format_bl, notes,
                   fusionne_dans, cree_le",
        e.nom,
        normaliser(&e.nom),
        e.siret,
        e.tva,
        &e.domaines,
        e.statut,
        e.societe_defaut_id,
        e.format_bl,
        e.notes
    )
    .fetch_one(&mut *tx)
    .await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "creation",
        "fournisseur",
        Some(f.id.to_string()),
        json!(e),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(f))
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Fiche>> {
    let e = e.nettoyer()?;
    let mut tx = etat.pool.begin().await?;
    let f = sqlx::query_as!(
        Fiche,
        "update fournisseur set nom = $2, nom_normalise = $3, siret = $4, tva = $5, domaines = $6,
            statut = $7, societe_defaut_id = $8, format_bl = $9, notes = $10
         where id = $1 and fusionne_dans is null
         returning id, nom, siret, tva, domaines, statut, societe_defaut_id, format_bl, notes,
                   fusionne_dans, cree_le",
        id,
        e.nom,
        normaliser(&e.nom),
        e.siret,
        e.tva,
        &e.domaines,
        e.statut,
        e.societe_defaut_id,
        e.format_bl,
        e.notes
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ErreurApi::Absent)?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "fournisseur",
        Some(id.to_string()),
        json!(e),
    )
    .await?;
    tx.commit().await?;
    // Une fiche validée (ou modifiée) change le contrôle « fournisseur » de ses factures.
    depot::reevaluer(&etat.pool, None, Some(id))
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(Json(f))
}

#[derive(Deserialize)]
pub struct Fusion {
    cible_id: i64,
}

#[derive(Serialize)]
pub struct ResultatFusion {
    documents: u64,
}

/// Fusionne la fiche `id` dans `cible_id` (doublon « METRO » / « Metro Cash & Carry »).
pub async fn fusionner(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(f): Json<Fusion>,
) -> Resultat<Json<ResultatFusion>> {
    if f.cible_id == id {
        return Err(ErreurApi::Donnees(
            "une fiche ne se fusionne pas avec elle-même".into(),
        ));
    }
    let mut tx = etat.pool.begin().await?;
    let actives = sqlx::query_scalar!(
        r#"select count(*) as "n!" from fournisseur where id = any($1) and fusionne_dans is null"#,
        &[id, f.cible_id]
    )
    .fetch_one(&mut *tx)
    .await?;
    if actives != 2 {
        return Err(ErreurApi::Absent);
    }
    let n = gmflow_core::fournisseurs::fusionner(&mut tx, id, f.cible_id).await?;
    journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "fusion",
        "fournisseur",
        Some(id.to_string()),
        json!({ "cible_id": f.cible_id, "documents": n }),
    )
    .await?;
    tx.commit().await?;
    depot::reevaluer(&etat.pool, None, Some(f.cible_id))
        .await
        .map_err(ErreurApi::Interne)?;
    gmflow_core::rapprochement::recalculer_fournisseur(&etat.pool, f.cible_id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(Json(ResultatFusion { documents: n }))
}
