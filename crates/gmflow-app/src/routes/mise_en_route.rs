//! Réglages, mise en route : état des 5 étapes (sociétés, boîtes mail, Claude, envoi vers
//! Dext/Yooz, utilisateurs), et essais de boîte mail (paramètres pas encore enregistrés, ou
//! SMTP d'une boîte existante). Les essais passent par le worker (module
//! `gmflow_core::essai_boite`) : l'app ne se connecte à aucun serveur de mail.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::essai_boite::{self, Essai, ParamImap, ParamSmtp};
use gmflow_core::{expedition, journal, parametre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

#[derive(Serialize)]
pub struct Etape {
    /// `societes`, `boites`, `claude`, `envoi`, `utilisateurs`.
    cle: &'static str,
    fait: bool,
    /// Ce qui est en place, en une phrase.
    resume: String,
    /// Ce qui manque encore (vide quand l'étape est faite).
    manques: Vec<String>,
}

#[derive(Serialize)]
pub struct MiseEnRoute {
    etapes: Vec<Etape>,
    terminee: bool,
}

fn pluriel(n: i64, un: &str, plusieurs: &str) -> String {
    if n <= 1 {
        format!("{n} {un}")
    } else {
        format!("{n} {plusieurs}")
    }
}

pub async fn etat(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<MiseEnRoute>> {
    let societes = sqlx::query!(
        r#"select s.id, s.raison_sociale, s.adresse_depot, s.outil_depot,
                  (select count(*) from boite_mail b where b.societe_id = s.id and b.active) as "boites_actives!",
                  (select count(*) from utilisateur u
                    where u.societe_id = s.id and u.actif and u.role = 'responsable') as "responsables!"
           from societe s where not s.archivee order by s.raison_sociale"#
    )
    .fetch_all(&etat.pool)
    .await?;
    let mut etapes = Vec::new();

    // 1. Sociétés.
    let n = societes.len() as i64;
    etapes.push(Etape {
        cle: "societes",
        fait: n > 0,
        resume: if n == 0 {
            "Aucune société".into()
        } else {
            format!(
                "{} : {}",
                pluriel(n, "société", "sociétés"),
                societes
                    .iter()
                    .map(|s| s.raison_sociale.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
        manques: if n == 0 {
            vec!["Ajouter au moins une société".into()]
        } else {
            Vec::new()
        },
    });

    // 2. Boîtes mail.
    let boites = sqlx::query!(
        r#"select libelle, active, pause_auto, (dernier_test->>'ok')::bool as test_ok
           from boite_mail order by libelle"#
    )
    .fetch_all(&etat.pool)
    .await?;
    let actives = boites.iter().filter(|b| b.active).count() as i64;
    let mut manques = Vec::new();
    for s in societes.iter().filter(|s| s.boites_actives == 0) {
        manques.push(format!(
            "{} : aucune boîte mail surveillée",
            s.raison_sociale
        ));
    }
    for b in &boites {
        if b.pause_auto {
            manques.push(format!(
                "{} : en pause après des refus du mot de passe",
                b.libelle
            ));
        } else if !b.active && b.test_ok != Some(true) {
            manques.push(format!(
                "{} : connexion pas encore réussie, boîte pas surveillée",
                b.libelle
            ));
        }
    }
    if boites.is_empty() && societes.is_empty() {
        manques.push("Ajouter une boîte mail".into());
    }
    etapes.push(Etape {
        cle: "boites",
        fait: actives > 0 && manques.is_empty(),
        resume: format!(
            "{} sur {}",
            pluriel(actives, "boîte surveillée", "boîtes surveillées"),
            boites.len()
        ),
        manques,
    });

    // 3. Claude.
    let claude = parametre::lire(&etat.pool, parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or_else(|| json!({ "etat": "absent" }));
    let e = claude
        .get("etat")
        .and_then(Value::as_str)
        .unwrap_or("absent");
    let message = claude
        .get("message")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty());
    let (fait, resume, manques) = match e {
        "valide" => (true, "Connecté à Claude".to_string(), Vec::new()),
        "en_test" => (
            false,
            "Connexion en cours de test".to_string(),
            vec!["Attendre la fin du test de la connexion".to_string()],
        ),
        "erreur" => (
            false,
            "Connexion refusée".to_string(),
            vec![format!(
                "Se reconnecter à Claude{}",
                message.map(|m| format!(" ({m})")).unwrap_or_default()
            )],
        ),
        _ => (
            false,
            "Pas connecté".to_string(),
            vec!["Se connecter avec le compte Claude".to_string()],
        ),
    };
    etapes.push(Etape {
        cle: "claude",
        fait,
        resume,
        manques,
    });

    // 4. Envoi vers Dext/Yooz.
    let exp = parametre::lire(&etat.pool, expedition::CLE)
        .await?
        .and_then(|v| serde_json::from_value::<expedition::Expedition>(v).ok());
    let test = parametre::lire(&etat.pool, expedition::CLE_TEST).await?;
    let mut manques = Vec::new();
    if exp.is_none() {
        manques.push("Configurer l'adresse d'envoi vers Dext/Yooz".to_string());
    } else if test
        .as_ref()
        .and_then(|t| t.get("ok"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        manques.push("Le dernier essai de l'adresse d'envoi a échoué".to_string());
    }
    for s in &societes {
        if s.adresse_depot
            .as_deref()
            .is_none_or(|a| a.trim().is_empty())
        {
            manques.push(format!(
                "{} : adresse Dext/Yooz manquante",
                s.raison_sociale
            ));
        }
    }
    let avec_adresse = societes
        .iter()
        .filter(|s| {
            s.adresse_depot
                .as_deref()
                .is_some_and(|a| !a.trim().is_empty())
        })
        .count() as i64;
    etapes.push(Etape {
        cle: "envoi",
        fait: exp.is_some() && n > 0 && manques.is_empty(),
        resume: match &exp {
            Some(x) => format!(
                "Envoi depuis {} ; {} sur {n}",
                x.adresse,
                pluriel(
                    avec_adresse,
                    "société avec son adresse Dext/Yooz",
                    "sociétés avec leur adresse Dext/Yooz"
                )
            ),
            None => "Adresse d'envoi pas configurée".into(),
        },
        manques,
    });

    // 5. Utilisateurs : un responsable actif par société.
    let comptes = sqlx::query!(
        r#"select count(*) filter (where role = 'admin') as "admins!",
                  count(*) filter (where role = 'responsable') as "responsables!"
           from utilisateur where actif"#
    )
    .fetch_one(&etat.pool)
    .await?;
    let manques: Vec<String> = societes
        .iter()
        .filter(|s| s.responsables == 0)
        .map(|s| format!("{} : aucun compte de responsable", s.raison_sociale))
        .collect();
    etapes.push(Etape {
        cle: "utilisateurs",
        fait: n > 0 && manques.is_empty(),
        resume: format!(
            "{}, {}",
            pluriel(comptes.admins, "administrateur", "administrateurs"),
            pluriel(comptes.responsables, "responsable", "responsables")
        ),
        manques,
    });

    let terminee = etapes.iter().all(|e| e.fait);
    Ok(Json(MiseEnRoute { etapes, terminee }))
}

#[derive(Deserialize)]
pub struct DemandeEssai {
    /// Boîte existante : les champs absents (et le mot de passe) sont pris dans la boîte.
    #[serde(default)]
    boite_id: Option<i64>,
    #[serde(default)]
    serveur: Option<String>,
    #[serde(default)]
    port: Option<i32>,
    #[serde(default)]
    identifiant: Option<String>,
    #[serde(default)]
    mot_de_passe: Option<String>,
    /// Faux : SMTP seulement.
    #[serde(default = "vrai")]
    imap: bool,
    #[serde(default)]
    smtp_serveur: Option<String>,
    #[serde(default)]
    smtp_port: Option<i32>,
    #[serde(default)]
    smtp_securite: Option<String>,
}

fn vrai() -> bool {
    true
}

fn net(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn port(p: i32) -> Resultat<u16> {
    u16::try_from(p)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| ErreurApi::Donnees("port invalide".into()))
}

#[derive(Serialize)]
pub struct EssaiCree {
    id: Uuid,
}

/// Enregistre la demande (mot de passe chiffré) et pousse la tâche du worker.
async fn lancer(
    etat: &Etat,
    admin_id: i64,
    essai: Essai,
    mdp_chiffre: Vec<u8>,
) -> Resultat<Json<EssaiCree>> {
    let id = Uuid::new_v4();
    let cle = essai_boite::cle(id);
    let mut tx = etat.pool.begin().await?;
    sqlx::query!(
        "delete from parametre where cle like 'essai_boite:%'
         and modifie_le < now() - make_interval(mins => $1)",
        essai_boite::DUREE_MINUTES
    )
    .execute(&mut *tx)
    .await?;
    parametre::ecrire(
        &mut *tx,
        &cle,
        &serde_json::to_value(&essai).map_err(|e| ErreurApi::Interne(e.into()))?,
    )
    .await?;
    parametre::ecrire_secret(&mut *tx, &cle, &mdp_chiffre).await?;
    journal::ecrire(
        &mut *tx,
        Some(admin_id),
        "essai",
        "boite_mail",
        essai.boite_id.map(|b| b.to_string()),
        json!({ "essai": id, "identifiant": essai.identifiant, "imap": essai.imap, "smtp": essai.smtp }),
    )
    .await?;
    tx.commit().await?;
    gmflow_core::file::pousser_essai_boite(&etat.pool, id)
        .await
        .map_err(ErreurApi::Interne)?;
    Ok(Json(EssaiCree { id }))
}

fn securite_permise(s: &str) -> bool {
    expedition::SECURITES.contains(&s) || (cfg!(feature = "test-utils") && s == "clair")
}

fn param_smtp(
    serveur: Option<String>,
    port_smtp: Option<i32>,
    securite: Option<String>,
) -> Resultat<Option<ParamSmtp>> {
    let Some(serveur) = serveur else {
        return Ok(None);
    };
    let securite = securite.unwrap_or_else(|| "tls".into());
    if !securite_permise(&securite) {
        return Err(ErreurApi::Donnees("sécurité SMTP : tls ou starttls".into()));
    }
    let defaut = if securite == "starttls" { 587 } else { 465 };
    Ok(Some(ParamSmtp {
        serveur,
        port: port(port_smtp.unwrap_or(defaut))?,
        securite,
    }))
}

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// `POST /api/admin/boites/tester` : essai de paramètres pas encore enregistrés (assistant
/// d'ajout, ou fiche d'une boîte en cours de modification avec `boite_id`). 202 + id de l'essai.
pub async fn tester(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Json(d): Json<DemandeEssai>,
) -> Resultat<(StatusCode, Json<EssaiCree>)> {
    let boite = match d.boite_id {
        Some(id) => Some(
            sqlx::query!(
                "select serveur, port, identifiant, mot_de_passe_chiffre, smtp_serveur, smtp_port,
                        smtp_securite
                 from boite_mail where id = $1",
                id
            )
            .fetch_optional(&etat.pool)
            .await?
            .ok_or(ErreurApi::Absent)?,
        ),
        None => None,
    };
    let serveur = net(d.serveur).or_else(|| boite.as_ref().map(|b| b.serveur.clone()));
    let identifiant = net(d.identifiant).or_else(|| boite.as_ref().map(|b| b.identifiant.clone()));
    let Some(identifiant) = identifiant else {
        return Err(ErreurApi::Donnees("identifiant obligatoire".into()));
    };
    let mdp_chiffre = match d.mot_de_passe.filter(|m| !m.is_empty()) {
        Some(m) => etat.chiffreur.chiffrer(m.as_bytes()),
        None => match &boite {
            Some(b) => b.mot_de_passe_chiffre.clone(),
            None => return Err(ErreurApi::Donnees("mot de passe obligatoire".into())),
        },
    };
    let imap = if d.imap {
        let Some(serveur) = serveur else {
            return Err(ErreurApi::Donnees("serveur IMAP obligatoire".into()));
        };
        let p = d
            .port
            .or_else(|| boite.as_ref().map(|b| b.port))
            .unwrap_or(993);
        Some(ParamImap {
            serveur,
            port: port(p)?,
        })
    } else {
        None
    };
    let smtp = param_smtp(
        net(d.smtp_serveur).or_else(|| boite.as_ref().and_then(|b| b.smtp_serveur.clone())),
        d.smtp_port
            .or_else(|| boite.as_ref().and_then(|b| b.smtp_port)),
        net(d.smtp_securite).or_else(|| boite.as_ref().and_then(|b| b.smtp_securite.clone())),
    )?;
    if imap.is_none() && smtp.is_none() {
        return Err(ErreurApi::Donnees(
            "rien à essayer : ni IMAP ni SMTP".into(),
        ));
    }
    let essai = Essai {
        etat: "en_cours".into(),
        cree_le: maintenant(),
        boite_id: d.boite_id,
        identifiant,
        imap,
        smtp,
        resultat_imap: None,
        resultat_smtp: None,
        termine_le: None,
    };
    Ok((
        StatusCode::ACCEPTED,
        lancer(&etat, a.utilisateur_id, essai, mdp_chiffre).await?,
    ))
}

/// `POST /api/admin/boites/{id}/tester-smtp` : essai SMTP (envoi des réponses) d'une boîte
/// enregistrée, sans rien envoyer. 202 + id de l'essai.
pub async fn tester_smtp(
    AccesAdmin(a): AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<i64>,
) -> Resultat<(StatusCode, Json<EssaiCree>)> {
    let b = sqlx::query!(
        "select identifiant, mot_de_passe_chiffre, smtp_serveur, smtp_port, smtp_securite
         from boite_mail where id = $1",
        id
    )
    .fetch_optional(&etat.pool)
    .await?
    .ok_or(ErreurApi::Absent)?;
    let Some(smtp) = param_smtp(net(b.smtp_serveur), b.smtp_port, b.smtp_securite)? else {
        return Err(ErreurApi::ConflitCode {
            code: "smtp_absent",
            message: "serveur SMTP de la boîte non configuré".into(),
        });
    };
    let essai = Essai {
        etat: "en_cours".into(),
        cree_le: maintenant(),
        boite_id: Some(id),
        identifiant: b.identifiant,
        imap: None,
        smtp: Some(smtp),
        resultat_imap: None,
        resultat_smtp: None,
        termine_le: None,
    };
    Ok((
        StatusCode::ACCEPTED,
        lancer(&etat, a.utilisateur_id, essai, b.mot_de_passe_chiffre).await?,
    ))
}

/// `GET /api/admin/boites/essais/{id}` : état et résultat d'un essai (à relire toutes les 1 à
/// 2 s tant que `etat` vaut `en_cours`).
pub async fn essai(
    _a: AccesAdmin,
    State(etat): State<Etat>,
    Path(id): Path<Uuid>,
) -> Resultat<Json<Essai>> {
    let v = parametre::lire(&etat.pool, &essai_boite::cle(id))
        .await?
        .ok_or(ErreurApi::Absent)?;
    Ok(Json(
        serde_json::from_value(v).map_err(|e| ErreurApi::Interne(e.into()))?,
    ))
}
