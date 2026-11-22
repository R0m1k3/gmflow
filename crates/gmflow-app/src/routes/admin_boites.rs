use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Boite {
    id: i64,
    societe_id: i64,
    libelle: String,
    serveur: String,
    port: i32,
    identifiant: String,
    dossiers: Vec<String>,
    reprise_mois: i32,
    active: bool,
    pause_auto: bool,
    echecs_auth: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    derniere_releve: Option<time::OffsetDateTime>,
    derniere_erreur: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    erreur_depuis: Option<time::OffsetDateTime>,
    dernier_test: Option<Value>,
    smtp_serveur: Option<String>,
    smtp_port: Option<i32>,
    smtp_securite: Option<String>,
    dossier_envoyes: Option<String>,
}

#[derive(Deserialize)]
pub struct Entree {
    societe_id: i64,
    libelle: String,
    serveur: String,
    #[serde(default = "port_imaps")]
    port: i32,
    identifiant: String,
    #[serde(default)]
    mot_de_passe: Option<String>,
    #[serde(default = "inbox")]
    dossiers: Vec<String>,
    #[serde(default = "trois")]
    reprise_mois: i32,
    /// Envoi des réponses (lot 5) : serveur SMTP de la boîte, mêmes identifiants qu'en IMAP.
    #[serde(default)]
    smtp_serveur: Option<String>,
    #[serde(default)]
    smtp_port: Option<i32>,
    #[serde(default)]
    smtp_securite: Option<String>,
    /// Dossier IMAP des messages envoyés (copie des réponses, exemples de ton).
    #[serde(default)]
    dossier_envoyes: Option<String>,
    /// Création : essai réussi de ces paramètres (`POST /api/admin/boites/tester`, moins d'une
    /// heure, même serveur, port, identifiant et mot de passe), repris comme test de la boîte.
    #[serde(default)]
    essai_id: Option<uuid::Uuid>,
    /// Création : démarrer la surveillance tout de suite (exige `essai_id`).
    #[serde(default)]
    activer: bool,
}

fn vide(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn port_imaps() -> i32 {
    993
}
fn inbox() -> Vec<String> {
    vec!["INBOX".into()]
}
fn trois() -> i32 {
    3
}

impl Entree {
    fn nettoyer(mut self) -> Resultat<Self> {
        self.libelle = self.libelle.trim().to_string();
        self.serveur = self.serveur.trim().to_string();
        self.identifiant = self.identifiant.trim().to_string();
        if self.libelle.is_empty() || self.serveur.is_empty() || self.identifiant.is_empty() {
            return Err(ErreurApi::Donnees(
                "libellé, serveur et identifiant obligatoires".into(),
            ));
        }
        if !(1..=65535).contains(&self.port) {
            return Err(ErreurApi::Donnees("port invalide".into()));
        }
        if !(0..=24).contains(&self.reprise_mois) {
            return Err(ErreurApi::Donnees("reprise : de 0 à 24 mois".into()));
        }
        self.dossiers = self
            .dossiers
            .into_iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect();
        self.dossiers.sort();
        self.dossiers.dedup();
        if self.dossiers.is_empty() {
            return Err(ErreurApi::Donnees(
                "au moins un dossier à surveiller".into(),
            ));
        }
        self.mot_de_passe = self.mot_de_passe.filter(|m| !m.is_empty());
        self.dossier_envoyes = vide(self.dossier_envoyes);
        self.smtp_serveur = vide(self.smtp_serveur);
        self.smtp_securite = vide(self.smtp_securite);
        if self.smtp_serveur.is_some() {
            let securite = self.smtp_securite.get_or_insert_with(|| "tls".into());
            let permise = gmflow_core::expedition::SECURITES.contains(&securite.as_str())
                || (cfg!(feature = "test-utils") && securite == "clair");
            if !permise {
                return Err(ErreurApi::Donnees("sécurité SMTP : tls ou starttls".into()));
            }
            let defaut = if securite == "starttls" { 587 } else { 465 };
            let port = *self.smtp_port.get_or_insert(defaut);
            if !(1..=65535).contains(&port) {
                return Err(ErreurApi::Donnees("port SMTP invalide".into()));
            }
            if !gmflow_core::expedition::adresse_valide(&self.identifiant) {
                return Err(ErreurApi::Donnees(
                    "pour envoyer des réponses, l'identifiant de la boîte doit être une adresse mail"
                        .into(),
                ));
            }
        } else {
            self.smtp_port = None;
            self.smtp_securite = None;
        }
        Ok(self)
    }

    fn journal(&self) -> Value {
        json!({ "societe_id": self.societe_id, "libelle": self.libelle, "serveur": self.serveur, "port": self.port,
                "identifiant": self.identifiant, "dossiers": self.dossiers, "reprise_mois": self.reprise_mois,
                "mot_de_passe_change": self.mot_de_passe.is_some(), "smtp_serveur": self.smtp_serveur,
                "smtp_port": self.smtp_port, "smtp_securite": self.smtp_securite, "dossier_envoyes": self.dossier_envoyes,
                "essai_id": self.essai_id })
    }
}

async fn lire(etat: &Etat, id: i64) -> Resultat<Boite> {
    sqlx::query_as!(
        Boite,
        "select id, societe_id, libelle, serveur, port, identifiant, dossiers, reprise_mois, active, pause_auto,
             echecs_auth, derniere_releve, derniere_erreur, erreur_depuis, dernier_test,
             smtp_serveur, smtp_port, smtp_securite, dossier_envoyes
         from boite_mail where id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)
}

/// `GET /api/admin/boites/{id}`.
pub async fn fiche(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Boite>> {
    Ok(Json(lire(&etat, id).await?))
}

/// Essai réussi repris à la création d'une boîte : rend le `dernier_test` à enregistrer, ou
/// `409 {"code": "essai"}` si l'essai ne vaut pas pour ces paramètres.
async fn test_de_l_essai(
    etat: &Etat,
    essai_id: uuid::Uuid,
    e: &Entree,
    mot_de_passe: &str,
) -> Resultat<Value> {
    use gmflow_core::essai_boite;
    let refus = |m: &str| ErreurApi::ConflitCode {
        code: "essai",
        message: format!("{m} : relancez l'essai de connexion"),
    };
    let essai = match super::mise_en_route::lire_essai(etat, essai_id).await {
        Ok(x) => x,
        Err(ErreurApi::Absent) => return Err(refus("essai introuvable ou expiré")),
        Err(x) => return Err(x),
    };
    if essai.etat != "termine" {
        return Err(refus("essai pas terminé"));
    }
    if essai_boite::age_minutes(&essai).is_none_or(|m| m > i64::from(essai_boite::DUREE_MINUTES)) {
        return Err(refus("essai de plus d'une heure"));
    }
    let memes = essai.boite_id.is_none()
        && essai.identifiant == e.identifiant
        && essai
            .imap
            .as_ref()
            .is_some_and(|p| p.serveur == e.serveur && i32::from(p.port) == e.port)
        && essai.mdp_empreinte.as_deref()
            == Some(essai_boite::empreinte(essai_id, mot_de_passe).as_str());
    if !memes {
        return Err(refus("l'essai a été fait avec d'autres paramètres"));
    }
    let Some(r) = essai.resultat_imap.filter(|r| r.ok) else {
        return Err(refus("la connexion n'a pas réussi"));
    };
    let noms: Vec<&str> = r.dossiers.iter().map(|d| d.nom.as_str()).collect();
    Ok(json!({
        "quand": essai.termine_le.unwrap_or(essai.cree_le),
        "ok": true,
        "message": r.message,
        "dossiers": noms,
        "dossiers_detail": r.dossiers,
        "envoyes": r.envoyes,
        "essai_id": essai_id,
    }))
}

pub async fn lister(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Vec<Boite>>> {
    let v = sqlx::query_as!(
        Boite,
        "select id, societe_id, libelle, serveur, port, identifiant, dossiers, reprise_mois, active, pause_auto,
             echecs_auth, derniere_releve, derniere_erreur, erreur_depuis, dernier_test,
             smtp_serveur, smtp_port, smtp_securite, dossier_envoyes
         from boite_mail order by libelle"
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}

async fn societe_existe(etat: &Etat, id: i64) -> Resultat<()> {
    let ok = sqlx::query_scalar!(
        "select exists (select 1 from societe where id = $1) as \"ok!\"",
        id
    )
    .fetch_one(&etat.pool)
    .await?;
    if ok {
        Ok(())
    } else {
        Err(ErreurApi::Donnees("structure inconnue".into()))
    }
}

pub async fn creer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Boite>> {
    let e = e.nettoyer()?;
    societe_existe(&etat, e.societe_id).await?;
    let Some(mdp) = &e.mot_de_passe else {
        return Err(ErreurApi::Donnees("mot de passe obligatoire".into()));
    };
    let test = match e.essai_id {
        Some(essai) => Some(test_de_l_essai(&etat, essai, &e, mdp).await?),
        None if e.activer => {
            return Err(ErreurApi::Donnees(
                "pour démarrer la surveillance à la création, donnez un essai réussi (essai_id)"
                    .into(),
            ));
        }
        None => None,
    };
    let chiffre = etat.chiffreur.chiffrer(mdp.as_bytes());
    let mut tx = etat.pool.begin().await?;
    let id = sqlx::query_scalar!(
        "insert into boite_mail (societe_id, libelle, serveur, port, identifiant, mot_de_passe_chiffre, dossiers, reprise_mois,
                                 smtp_serveur, smtp_port, smtp_securite, dossier_envoyes, dernier_test, active)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) returning id",
        e.societe_id,
        e.libelle,
        e.serveur,
        e.port,
        e.identifiant,
        chiffre,
        &e.dossiers,
        e.reprise_mois,
        e.smtp_serveur,
        e.smtp_port,
        e.smtp_securite,
        e.dossier_envoyes,
        test,
        e.activer
    )
    .fetch_one(&mut *tx)
    .await?;
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "creation",
        "boite_mail",
        Some(id.to_string()),
        e.journal(),
    )
    .await?;
    if e.activer {
        gmflow_core::journal::ecrire(
            &mut *tx,
            Some(a.utilisateur_id),
            "activation",
            "boite_mail",
            Some(id.to_string()),
            json!({ "essai_id": e.essai_id }),
        )
        .await?;
    }
    tx.commit().await?;
    // Surveillée tout de suite : la première relève part sans attendre la planification (5 min).
    if e.activer {
        gmflow_core::file::pousser_releve(&etat.pool, id).await?;
    }
    Ok(Json(lire(&etat, id).await?))
}

pub async fn modifier(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
    Json(e): Json<Entree>,
) -> Resultat<Json<Boite>> {
    let e = e.nettoyer()?;
    societe_existe(&etat, e.societe_id).await?;
    let chiffre = e
        .mot_de_passe
        .as_ref()
        .map(|m| etat.chiffreur.chiffrer(m.as_bytes()));
    let mut tx = etat.pool.begin().await?;
    // Paramètres de connexion changés : le test précédent ne vaut plus.
    let n = sqlx::query!(
        "update boite_mail set
             dernier_test = case when serveur <> $3 or port <> $4 or identifiant <> $5 or $6::bytea is not null
                                 then null else dernier_test end,
             societe_id = $2, serveur = $3, port = $4, identifiant = $5,
             mot_de_passe_chiffre = coalesce($6, mot_de_passe_chiffre),
             libelle = $7, dossiers = $8, reprise_mois = $9,
             smtp_serveur = $10, smtp_port = $11, smtp_securite = $12, dossier_envoyes = $13
         where id = $1",
        id,
        e.societe_id,
        e.serveur,
        e.port,
        e.identifiant,
        chiffre,
        e.libelle,
        &e.dossiers,
        e.reprise_mois,
        e.smtp_serveur,
        e.smtp_port,
        e.smtp_securite,
        e.dossier_envoyes
    )
    .execute(&mut *tx)
    .await?;
    if n.rows_affected() == 0 {
        return Err(ErreurApi::Absent);
    }
    gmflow_core::journal::ecrire(
        &mut *tx,
        Some(a.utilisateur_id),
        "modification",
        "boite_mail",
        Some(id.to_string()),
        e.journal(),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(lire(&etat, id).await?))
}

pub async fn tester(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    lire(&etat, id).await?;
    gmflow_core::file::pousser_test_boite(&etat.pool, id).await?;
    gmflow_core::journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "test",
        "boite_mail",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn activer(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Boite>> {
    let b = lire(&etat, id).await?;
    if b.dernier_test
        .as_ref()
        .and_then(|t| t.get("ok"))
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(ErreurApi::Conflit(
            "testez la connexion avec succès avant d'activer la boîte".into(),
        ));
    }
    sqlx::query!(
        "update boite_mail set active = true, pause_auto = false, echecs_auth = 0 where id = $1",
        id
    )
    .execute(&etat.pool)
    .await?;
    gmflow_core::journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "activation",
        "boite_mail",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    gmflow_core::file::pousser_releve(&etat.pool, id).await?;
    Ok(Json(lire(&etat, id).await?))
}

pub async fn pause(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Boite>> {
    lire(&etat, id).await?;
    sqlx::query!("update boite_mail set active = false where id = $1", id)
        .execute(&etat.pool)
        .await?;
    gmflow_core::journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "pause",
        "boite_mail",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(Json(lire(&etat, id).await?))
}

pub async fn relever(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<StatusCode> {
    let b = lire(&etat, id).await?;
    if !b.active || b.pause_auto {
        return Err(ErreurApi::Conflit("la boîte n'est pas active".into()));
    }
    gmflow_core::file::pousser_releve(&etat.pool, id).await?;
    gmflow_core::journal::ecrire(
        &etat.pool,
        Some(a.utilisateur_id),
        "releve_manuelle",
        "boite_mail",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct Compteur {
    dossier: String,
    messages_vus: i64,
    mails_uniques: i64,
    messages_vus_total: i64,
    dernier_uid: i64,
}

/// Par dossier surveillé : messages vus sur la période de reprise (à comparer au webmail).
pub async fn compteurs(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<Json<Vec<Compteur>>> {
    lire(&etat, id).await?;
    let v = sqlx::query_as!(
        Compteur,
        r#"select d.nom as "dossier!",
               count(mb.id) filter (where coalesce(m.date_mail, mb.vu_le) >= now() - make_interval(months => b.reprise_mois)) as "messages_vus!",
               count(distinct mb.mail_id) filter (where coalesce(m.date_mail, mb.vu_le) >= now() - make_interval(months => b.reprise_mois)) as "mails_uniques!",
               count(mb.id) as "messages_vus_total!",
               coalesce(bd.dernier_uid, 0) as "dernier_uid!"
           from boite_mail b
           cross join unnest(b.dossiers) as d(nom)
           left join boite_dossier bd on bd.boite_id = b.id and bd.nom = d.nom
           left join mail_boite mb on mb.boite_id = b.id and mb.dossier = d.nom
           left join mail m on m.id = mb.mail_id
           where b.id = $1
           group by d.nom, bd.dernier_uid
           order by d.nom"#,
        id
    )
    .fetch_all(&etat.pool)
    .await?;
    Ok(Json(v))
}
