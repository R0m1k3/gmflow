//! Référentiel des fournisseurs : liste, fiche et fil (un responsable ne voit que les
//! fournisseurs de sa société : un document dans son périmètre, ou sa société par défaut),
//! création, modification (complète ou partielle), validation et fusion (administrateur).

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
    /// Seulement les fournisseurs qui ont au moins un document lu dans cette société ; les
    /// compteurs d'état sont limités à cette société.
    societe: Option<i64>,
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct LigneFournisseur {
    id: i64,
    nom: String,
    code_gestion: Option<String>,
    siret: Option<String>,
    tva: Option<String>,
    domaines: Vec<String>,
    statut: String,
    societe_defaut_id: Option<i64>,
    societe_defaut: Option<String>,
    nb_documents: i64,
    a_verifier: i64,
    /// Ligne d'état, sur les documents lus visibles (et de la société choisie) :
    /// factures et avoirs datés du mois en cours,
    factures_mois: i64,
    /// documents dans la file « À valider »,
    a_valider: i64,
    /// BL sans facture rapprochée (ni écartés du rapprochement),
    bl_sans_facture: i64,
    /// date du document le plus récent.
    #[serde(with = "date_iso::option")]
    dernier_document: Option<time::Date>,
}

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

pub async fn lister(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Page<LigneFournisseur>>> {
    let q = motif(&f.q);
    let statut = f.statut.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (page, decalage) = decalage(f.page);
    let perimetre = acces.societe_filtre();
    let total = sqlx::query_scalar!(
        r#"select count(*) as "n!" from fournisseur f
           where f.fusionne_dans is null and ($1::text is null or f.statut = $1)
             and ($2::text is null or f.nom ilike $2 or f.code_gestion ilike $2 or f.siret ilike $2 or f.tva ilike $2
                  or array_to_string(f.domaines, ' ') ilike $2)
             and ($3::bigint is null or f.societe_defaut_id = $3
                  or exists (select 1 from document d
                             where d.fournisseur_id = f.id
                               and (d.societe_id is null or d.societe_id = $3)
                               and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                           where mb.mail_id = d.mail_id and b.societe_id = $3)))
             and ($4::bigint is null or exists (
                   select 1 from document d
                   where d.fournisseur_id = f.id and d.etat = 'lu'
                     and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                 where mb.mail_id = d.mail_id and ($3::bigint is null or b.societe_id = $3))
                     and ($3::bigint is null or d.societe_id is null or d.societe_id = $3)
                     and (d.societe_id = $4
                          or (d.societe_id is null
                              and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                          where mb.mail_id = d.mail_id and b.societe_id = $4)))))"#,
        statut,
        q,
        perimetre,
        f.societe
    )
    .fetch_one(&etat.pool)
    .await?;
    let elements = sqlx::query_as!(
        LigneFournisseur,
        r#"select f.id, f.nom, f.code_gestion, f.siret, f.tva, f.domaines, f.statut, f.societe_defaut_id,
                  s.raison_sociale as "societe_defaut?",
                  (select count(*) from document d where d.fournisseur_id = f.id
                     and ($5::bigint is null
                          or ((d.societe_id is null or d.societe_id = $5)
                              and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                          where mb.mail_id = d.mail_id and b.societe_id = $5)))) as "nb_documents!",
                  (select count(*) from document d where d.fournisseur_id = f.id
                     and d.statut_depot = 'a_verifier'
                     and ($5::bigint is null
                          or ((d.societe_id is null or d.societe_id = $5)
                              and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                          where mb.mail_id = d.mail_id and b.societe_id = $5)))) as "a_verifier!",
                  e.factures_mois as "factures_mois!", e.a_valider as "a_valider!",
                  e.bl_sans_facture as "bl_sans_facture!", e.dernier as dernier_document
           from fournisseur f
           left join societe s on s.id = f.societe_defaut_id
           cross join lateral (
               select count(*) filter (where d.type in ('facture', 'avoir') and d.statut_depot is distinct from 'doublon'
                                         and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date)
                                             >= date_trunc('month', now() at time zone 'Europe/Paris')::date) as factures_mois,
                      count(*) filter (where d.type in ('facture', 'avoir', 'bl')
                                         and ((d.verifie_le is null
                                               and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                              or (d.verifie_le is not null and d.valide_le is null
                                                  and d.statut_depot in ('a_verifier', 'pret')))) as a_valider,
                      count(*) filter (where d.type = 'bl' and not d.rapprochement_ignore
                                         and not exists (select 1 from rapprochement r
                                                         where r.bl_id = d.id and not r.rejete)) as bl_sans_facture,
                      max(coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date)) as dernier,
                      count(*) as n
               from document d join mail m on m.id = d.mail_id
               where d.fournisseur_id = f.id and d.etat = 'lu'
                 and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = d.mail_id and ($5::bigint is null or b.societe_id = $5))
                 and ($5::bigint is null or d.societe_id is null or d.societe_id = $5)
                 and ($6::bigint is null or d.societe_id = $6
                      or (d.societe_id is null
                          and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                      where mb.mail_id = d.mail_id and b.societe_id = $6)))
           ) e
           where f.fusionne_dans is null and ($1::text is null or f.statut = $1)
             and ($2::text is null or f.nom ilike $2 or f.code_gestion ilike $2 or f.siret ilike $2 or f.tva ilike $2
                  or array_to_string(f.domaines, ' ') ilike $2)
             and ($6::bigint is null or e.n > 0)
             and ($5::bigint is null or f.societe_defaut_id = $5
                  or exists (select 1 from document d
                             where d.fournisseur_id = f.id
                               and (d.societe_id is null or d.societe_id = $5)
                               and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                           where mb.mail_id = d.mail_id and b.societe_id = $5)))
           order by (f.statut = 'a_confirmer') desc, f.nom
           limit $3 offset $4"#,
        statut,
        q,
        super::mails::PAR_PAGE,
        decalage,
        perimetre,
        f.societe
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
    /// Code du fournisseur dans le logiciel de gestion du magasin.
    code_gestion: Option<String>,
    siret: Option<String>,
    tva: Option<String>,
    domaines: Vec<String>,
    statut: String,
    societe_defaut_id: Option<i64>,
    /// Nom de la société par défaut.
    societe_defaut: Option<String>,
    format_bl: Option<String>,
    notes: Option<String>,
    fusionne_dans: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    cree_le: time::OffsetDateTime,
}

/// Vrai si le fournisseur existe et que cet accès le voit : tout le référentiel pour un
/// administrateur ; pour un responsable, un document du fournisseur dans son périmètre (boîte
/// de sa société, rangé dans sa société ou pas encore rangé), ou sa société par défaut.
pub(crate) async fn fournisseur_visible(etat: &Etat, acces: &Acces, id: i64) -> Resultat<bool> {
    Ok(sqlx::query_scalar!(
        r#"select exists (
             select 1 from fournisseur f
             where f.id = $1
               and ($2::bigint is null or f.societe_defaut_id = $2
                    or exists (select 1 from document d
                               where d.fournisseur_id = f.id
                                 and (d.societe_id is null or d.societe_id = $2)
                                 and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                             where mb.mail_id = d.mail_id and b.societe_id = $2)))) as "ok!""#,
        id,
        acces.societe_filtre()
    )
    .fetch_one(&etat.pool)
    .await?)
}

pub async fn fiche(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Fiche>> {
    if !fournisseur_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let f = sqlx::query_as!(
        Fiche,
        r#"select id, nom, code_gestion, siret, tva, domaines, statut, societe_defaut_id,
                  (select raison_sociale from societe where id = societe_defaut_id) as "societe_defaut?",
                  format_bl, notes, fusionne_dans, cree_le
           from fournisseur where id = $1"#,
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
    code_gestion: Option<String>,
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
        self.code_gestion = net(self.code_gestion);
        Ok(self)
    }
}

/// Un code de gestion désigne un seul fournisseur : refusé s'il est déjà sur une autre fiche.
async fn code_libre(etat: &Etat, e: &Entree, id: Option<i64>) -> Resultat<()> {
    let Some(code) = &e.code_gestion else {
        return Ok(());
    };
    let autre = sqlx::query_scalar!(
        "select nom from fournisseur
         where fusionne_dans is null and lower(code_gestion) = lower($1)
           and ($2::bigint is null or id <> $2)
         limit 1",
        code,
        id
    )
    .fetch_optional(&etat.pool)
    .await?;
    match autre {
        Some(nom) => Err(ErreurApi::Donnees(format!(
            "le code fournisseur « {code} » est déjà celui de {nom}"
        ))),
        None => Ok(()),
    }
}

pub async fn creer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Fiche>> {
    let e = e.nettoyer()?;
    code_libre(&etat, &e, None).await?;
    let mut tx = etat.pool.begin().await?;
    let f = sqlx::query_as!(
        Fiche,
        r#"insert into fournisseur (nom, nom_normalise, siret, tva, domaines, statut,
                                    societe_defaut_id, format_bl, notes, code_gestion)
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           returning id, nom, code_gestion, siret, tva, domaines, statut, societe_defaut_id,
                     (select raison_sociale from societe where id = societe_defaut_id) as "societe_defaut?",
                     format_bl, notes, fusionne_dans, cree_le"#,
        e.nom,
        normaliser(&e.nom),
        e.siret,
        e.tva,
        &e.domaines,
        e.statut,
        e.societe_defaut_id,
        e.format_bl,
        e.notes,
        e.code_gestion
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
    enregistrer(&etat, &a, id, e).await
}

/// Champ présent dans le corps, même à `null` (`Some(None)`) ; absent : `None`.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Modification partielle : seuls les champs présents changent (`null` efface un champ
/// facultatif).
#[derive(Deserialize)]
pub struct Changements {
    #[serde(default)]
    nom: Option<String>,
    #[serde(default, deserialize_with = "present")]
    code_gestion: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    siret: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    tva: Option<Option<String>>,
    #[serde(default)]
    domaines: Option<Vec<String>>,
    #[serde(default)]
    statut: Option<String>,
    #[serde(default, deserialize_with = "present")]
    societe_defaut_id: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present")]
    format_bl: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    notes: Option<Option<String>>,
}

/// `PATCH /api/admin/fournisseurs/{id}` : « Confirmer la fiche » (`{"statut": "valide"}`),
/// « Toujours ranger dans X » (`{"societe_defaut_id": 1}`), sans renvoyer toute la fiche.
pub async fn modifier_partiel(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(c): Json<Changements>,
) -> Resultat<Json<Fiche>> {
    let f = sqlx::query!(
        "select nom, code_gestion, siret, tva, domaines, statut, societe_defaut_id, format_bl, notes
         from fournisseur where id = $1 and fusionne_dans is null",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    let e = Entree {
        nom: c.nom.unwrap_or(f.nom),
        code_gestion: c.code_gestion.unwrap_or(f.code_gestion),
        siret: c.siret.unwrap_or(f.siret),
        tva: c.tva.unwrap_or(f.tva),
        domaines: c.domaines.unwrap_or(f.domaines),
        statut: c.statut.unwrap_or(f.statut),
        societe_defaut_id: c.societe_defaut_id.unwrap_or(f.societe_defaut_id),
        format_bl: c.format_bl.unwrap_or(f.format_bl),
        notes: c.notes.unwrap_or(f.notes),
    };
    enregistrer(&etat, &a, id, e).await
}

async fn enregistrer(etat: &Etat, a: &Acces, id: i64, e: Entree) -> Resultat<Json<Fiche>> {
    let e = e.nettoyer()?;
    code_libre(etat, &e, Some(id)).await?;
    if let Some(s) = e.societe_defaut_id {
        let existe = sqlx::query_scalar!(
            r#"select exists (select 1 from societe where id = $1) as "ok!""#,
            s
        )
        .fetch_one(&etat.pool)
        .await?;
        if !existe {
            return Err(ErreurApi::Donnees("société inconnue".into()));
        }
    }
    let mut tx = etat.pool.begin().await?;
    let f = sqlx::query_as!(
        Fiche,
        r#"update fournisseur set nom = $2, nom_normalise = $3, siret = $4, tva = $5, domaines = $6,
              statut = $7, societe_defaut_id = $8, format_bl = $9, notes = $10,
              code_gestion = $11
           where id = $1 and fusionne_dans is null
           returning id, nom, code_gestion, siret, tva, domaines, statut, societe_defaut_id,
                     (select raison_sociale from societe where id = societe_defaut_id) as "societe_defaut?",
                     format_bl, notes, fusionne_dans, cree_le"#,
        id,
        e.nom,
        normaliser(&e.nom),
        e.siret,
        e.tva,
        &e.domaines,
        e.statut,
        e.societe_defaut_id,
        e.format_bl,
        e.notes,
        e.code_gestion
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

#[derive(Deserialize)]
pub struct FiltresFil {
    societe: Option<i64>,
    /// Nombre de mois rendus, mois en cours compris (6 par défaut, de 1 à 24).
    mois: Option<i32>,
}

#[derive(Serialize)]
pub struct EtatFournisseur {
    factures_mois: i64,
    a_valider: i64,
    bl_sans_facture: i64,
}

#[derive(Serialize)]
#[serde(tag = "genre", rename_all = "snake_case")]
pub enum ElementFil {
    Document {
        id: i64,
        mail_id: i64,
        r#type: String,
        numero: Option<String>,
        #[serde(with = "date_iso")]
        date: time::Date,
        #[serde(with = "date_iso::option")]
        date_document: Option<time::Date>,
        montant_ht: Option<f64>,
        montant_ttc: Option<f64>,
        statut_depot: Option<String>,
        verifie: bool,
        societe_id: Option<i64>,
        societe: Option<String>,
        /// Outil de collecte de la société (`dext`, `yooz`), pour « Envoyée à Dext ».
        outil: Option<String>,
        /// BL : rattaché à une facture ; facture : au moins un BL rattaché.
        rapproche: bool,
        nb_alertes: i64,
    },
    Mail {
        id: i64,
        #[serde(with = "date_iso")]
        date: time::Date,
        #[serde(with = "time::serde::rfc3339::option")]
        date_mail: Option<time::OffsetDateTime>,
        expediteur: String,
        objet: String,
        categorie: Option<String>,
        statut_reponse: Option<String>,
        resume: Option<String>,
        /// Documents de ce fournisseur portés par le mail (aussi présents dans le fil).
        document_ids: Vec<i64>,
    },
}

impl ElementFil {
    fn cle(&self) -> (time::Date, i64) {
        match self {
            ElementFil::Document { date, id, .. } | ElementFil::Mail { date, id, .. } => {
                (*date, *id)
            }
        }
    }
}

#[derive(Serialize)]
pub struct MoisFil {
    /// AAAA-MM.
    mois: String,
    elements: Vec<ElementFil>,
}

#[derive(Serialize)]
pub struct ARapprocher {
    bl_sans_facture: Vec<super::rapprochement::DocRap>,
    factures_sans_bl: Vec<super::rapprochement::DocRap>,
    avec_ecart: Vec<super::rapprochement::Rapproche>,
    tronque: bool,
}

#[derive(Serialize)]
pub struct Fil {
    fournisseur: Fiche,
    etat: EtatFournisseur,
    /// Du plus récent au plus ancien ; les mois sans rien n'apparaissent pas.
    mois: Vec<MoisFil>,
    a_rapprocher: ARapprocher,
}

/// Fiche fournisseur : documents lus (factures, avoirs, BL) et mails du fournisseur (portant
/// un de ses documents, ou envoyés depuis un de ses domaines ; publicités exclues), mois par
/// mois, et encadré « À rapprocher » (sur tout l'historique).
pub async fn fil(
    acces: Acces,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Query(f): Query<FiltresFil>,
) -> Resultat<Json<Fil>> {
    if !fournisseur_visible(&etat, &acces, id).await? {
        return Err(ErreurApi::Absent);
    }
    let fiche = sqlx::query_as!(
        Fiche,
        r#"select id, nom, code_gestion, siret, tva, domaines, statut, societe_defaut_id,
                  (select raison_sociale from societe where id = societe_defaut_id) as "societe_defaut?",
                  format_bl, notes, fusionne_dans, cree_le
           from fournisseur where id = $1"#,
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    let nb_mois = f.mois.unwrap_or(6);
    if !(1..=24).contains(&nb_mois) {
        return Err(ErreurApi::Donnees("mois : de 1 à 24".into()));
    }
    let perimetre = acces.societe_filtre();
    let societe = f.societe;
    let etat_f = sqlx::query_as!(
        EtatFournisseur,
        r#"select count(*) filter (where d.type in ('facture', 'avoir') and d.statut_depot is distinct from 'doublon'
                                     and coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date)
                                         >= date_trunc('month', now() at time zone 'Europe/Paris')::date) as "factures_mois!",
                  count(*) filter (where d.type in ('facture', 'avoir', 'bl')
                                     and ((d.verifie_le is null
                                           and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                          or (d.verifie_le is not null and d.valide_le is null
                                              and d.statut_depot in ('a_verifier', 'pret')))) as "a_valider!",
                  count(*) filter (where d.type = 'bl' and not d.rapprochement_ignore
                                     and not exists (select 1 from rapprochement r
                                                     where r.bl_id = d.id and not r.rejete)) as "bl_sans_facture!"
           from document d join mail m on m.id = d.mail_id
           where d.fournisseur_id = $3 and d.etat = 'lu'
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))"#,
        perimetre,
        societe,
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    let documents = sqlx::query!(
        r#"select d.id, d.mail_id, d.type as "type!", d.numero, d.date_document,
                  coalesce(d.date_document, (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris')::date) as "date!",
                  d.montant_ht::float8 as montant_ht, d.montant_ttc::float8 as montant_ttc,
                  d.statut_depot, d.verifie_le is not null as "verifie!", d.societe_id,
                  s.raison_sociale as "societe?", s.outil_depot as "outil?",
                  exists (select 1 from rapprochement r
                          where not r.rejete and (r.bl_id = d.id or r.facture_id = d.id)) as "rapproche!",
                  jsonb_array_length(d.controles)::bigint as "nb_alertes!"
           from document d
           join mail m on m.id = d.mail_id
           left join societe s on s.id = d.societe_id
           where d.fournisseur_id = $3 and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and coalesce(d.date_document, (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris')::date)
                 >= (date_trunc('month', now() at time zone 'Europe/Paris') - make_interval(months => $4 - 1))::date
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))
           order by 6 desc, d.id desc
           limit 1000"#,
        perimetre,
        societe,
        id,
        nb_mois
    )
    .fetch_all(&etat.pool)
    .await?;
    let mails = sqlx::query!(
        r#"select m.id, m.expediteur, m.objet, m.date_mail, m.categorie, m.statut_reponse, m.resume,
                  ((coalesce(m.date_mail, m.recu_le)) at time zone 'Europe/Paris')::date as "date!",
                  array(select d.id from document d where d.mail_id = m.id and d.fournisseur_id = $3
                          and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
                          and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
                        order by d.id) as "document_ids!"
           from mail m, fournisseur f
           where f.id = $3
             and m.categorie is distinct from 'publicite'
             and (exists (select 1 from document d where d.mail_id = m.id and d.fournisseur_id = f.id)
                  or lower(substring(m.expediteur from '@([A-Za-z0-9.-]+)')) = any(f.domaines))
             and coalesce(m.date_mail, m.recu_le)
                 >= (date_trunc('month', now() at time zone 'Europe/Paris') - make_interval(months => $4 - 1))
                    at time zone 'Europe/Paris'
             and exists (select 1 from mail_boite mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2))
           order by coalesce(m.date_mail, m.recu_le) desc, m.id desc
           limit 1000"#,
        perimetre,
        societe,
        id,
        nb_mois
    )
    .fetch_all(&etat.pool)
    .await?;
    let mut elements: Vec<ElementFil> = documents
        .into_iter()
        .map(|d| ElementFil::Document {
            id: d.id,
            mail_id: d.mail_id,
            r#type: d.r#type,
            numero: d.numero,
            date: d.date,
            date_document: d.date_document,
            montant_ht: d.montant_ht,
            montant_ttc: d.montant_ttc,
            statut_depot: d.statut_depot,
            verifie: d.verifie,
            societe_id: d.societe_id,
            societe: d.societe,
            outil: d.outil,
            rapproche: d.rapproche,
            nb_alertes: d.nb_alertes,
        })
        .chain(mails.into_iter().map(|m| ElementFil::Mail {
            id: m.id,
            date: m.date,
            date_mail: m.date_mail,
            expediteur: m.expediteur,
            objet: m.objet,
            categorie: m.categorie,
            statut_reponse: m.statut_reponse,
            resume: m.resume,
            document_ids: m.document_ids,
        }))
        .collect();
    elements.sort_by_key(|e| std::cmp::Reverse(e.cle()));
    let mut mois: Vec<MoisFil> = Vec::new();
    for e in elements {
        let (date, _) = e.cle();
        let cle = format!("{:04}-{:02}", date.year(), u8::from(date.month()));
        match mois.last_mut() {
            Some(m) if m.mois == cle => m.elements.push(e),
            _ => mois.push(MoisFil {
                mois: cle,
                elements: vec![e],
            }),
        }
    }
    let Json(vue) = super::rapprochement::vue(
        acces,
        State(etat),
        Query(super::rapprochement::Filtres {
            societe,
            fournisseur: Some(id),
            ..Default::default()
        }),
    )
    .await?;
    Ok(Json(Fil {
        fournisseur: fiche,
        etat: etat_f,
        mois,
        a_rapprocher: ARapprocher {
            bl_sans_facture: vue.bl_sans_facture,
            factures_sans_bl: vue.factures_sans_bl,
            avec_ecart: vue.avec_ecart,
            tronque: vue.tronque,
        },
    }))
}
