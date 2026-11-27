//! Réglages > Traitements (administrateurs) : où en est chaque étape du courrier (relève,
//! lecture, tri, rapprochement, dépôt), ce qui attend et pourquoi, et les relances. L'app ne
//! parle au worker que par la base : battements des files apalis, `parametre.orchestre_etat`
//! écrit par l'orchestre, demandes déposées dans `parametre.orchestre_demandes`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use gmflow_core::traitements::{self, BATTEMENT_FILE_MAX, ETAPES, RETARD_MIN};
use gmflow_core::{attente, file, journal, parametre, reglages};
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;

use crate::acces::AccesAdmin;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;
#[cfg(feature = "ts")]
use gmflow_core::vocabulaire as v;

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Traitements {
    /// Le worker tourne-t-il (l'orchestre a écrit son état il y a moins de 3 minutes) ?
    service: Service,
    /// Pourquoi Claude ne lit pas en ce moment (jeton, refus, quota), sinon rien.
    claude: Option<String>,
    etapes: Vec<EtapeTraitement>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Service {
    en_marche: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    vu_le: Option<OffsetDateTime>,
    /// Files hors étapes (connexion Claude, rédaction des réponses) arrêtées.
    autres_arretees: Vec<&'static str>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EtapeTraitement {
    #[cfg_attr(feature = "ts", ts(as = "v::CleEtapeTraitement"))]
    cle: &'static str,
    nom: &'static str,
    /// `ok`, `en_retard`, `bloque` (elle ne peut pas avancer et du travail attend) ou
    /// `arrete` (sa file ne bat plus).
    #[cfg_attr(feature = "ts", ts(as = "v::EtatTraitement"))]
    etat: &'static str,
    /// Une phrase qui résume l'étape.
    resume: String,
    /// Pourquoi elle ne peut pas avancer (étape `bloque`).
    blocage: Option<String>,
    en_attente: i64,
    plus_vieille: Option<PlusVieille>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    dernier_passage: Option<OffsetDateTime>,
    file: FileEtat,
    /// Relance demandée, pas encore prise par le worker.
    relance_demandee: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlusVieille {
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    depuis: OffsetDateTime,
    raison: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FileEtat {
    nom: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    battement: Option<OffsetDateTime>,
    vivante: bool,
}

fn file_etat(b: &HashMap<String, OffsetDateTime>, nom: &'static str) -> FileEtat {
    let battement = b.get(nom).copied();
    FileEtat {
        nom,
        battement,
        vivante: battement.is_some_and(|t| OffsetDateTime::now_utc() - t <= BATTEMENT_FILE_MAX),
    }
}

fn etat_de(file: &FileEtat, blocage: &Option<String>, en_retard: bool) -> &'static str {
    if !file.vivante {
        "arrete"
    } else if blocage.is_some() {
        "bloque"
    } else if en_retard {
        "en_retard"
    } else {
        "ok"
    }
}

fn pluriel(n: i64, un: &str, plusieurs: &str) -> String {
    if n == 1 {
        format!("1 {un}")
    } else {
        format!("{n} {plusieurs}")
    }
}

fn depuis_trop(t: Option<OffsetDateTime>) -> bool {
    t.is_some_and(|t| OffsetDateTime::now_utc() - t > time::Duration::minutes(RETARD_MIN))
}

pub async fn lister(_a: AccesAdmin, State(etat): State<Etat>) -> Resultat<Json<Traitements>> {
    let pool = &etat.pool;
    let b = traitements::battements(pool).await?;
    let demandes = traitements::demandes(pool).await?;
    let vu_le = parametre::lire(pool, traitements::ORCHESTRE_ETAT)
        .await?
        .and_then(|v| v.get("vu_le").and_then(|q| q.as_str()).map(str::to_string))
        .and_then(|q| {
            OffsetDateTime::parse(&q, &time::format_description::well_known::Rfc3339).ok()
        });
    let service = Service {
        en_marche: vu_le.is_some_and(|t| OffsetDateTime::now_utc() - t <= BATTEMENT_FILE_MAX),
        vu_le,
        autres_arretees: [file::FILE_CLAUDE, file::FILE_REDACTION]
            .into_iter()
            .filter(|f| b.contains_key(*f) && !file_etat(&b, f).vivante)
            .collect(),
    };
    let r = reglages::lire(pool).await?;
    let claude = attente::lecture_indisponible(pool, &r, false)
        .await
        .map_err(ErreurApi::Interne)?
        .map(|i| i.message);
    let mut etapes = vec![
        releve(&etat, &b).await?,
        lecture(&etat, &b, &r, claude.clone()).await?,
        tri(&etat, &b, &r).await?,
        rapprochement(&etat, &b).await?,
        depot(&etat, &b).await?,
    ];
    for e in &mut etapes {
        e.relance_demandee = demandes.contains_key(e.cle);
    }
    Ok(Json(Traitements {
        service,
        claude,
        etapes,
    }))
}

async fn releve(etat: &Etat, b: &HashMap<String, OffsetDateTime>) -> Resultat<EtapeTraitement> {
    let boites = sqlx::query!(
        r#"select libelle, pause_auto, derniere_releve, derniere_erreur, erreur_depuis
           from boite_mail where active order by id"#
    )
    .fetch_all(&etat.pool)
    .await?;
    let file = file_etat(b, file::FILE_BOITES);
    let mut en_retard = 0;
    let mut plus_vieille: Option<PlusVieille> = None;
    for x in &boites {
        let raison = if x.pause_auto {
            Some(format!(
                "Boîte « {} » arrêtée après des échecs répétés : {}",
                x.libelle,
                x.derniere_erreur.as_deref().unwrap_or("relève impossible")
            ))
        } else if let Some(err) = &x.derniere_erreur {
            Some(format!("Boîte « {} » : {err}", x.libelle))
        } else if x.derniere_releve.is_none() || depuis_trop(x.derniere_releve) {
            Some(format!(
                "Boîte « {} » pas relevée depuis un moment",
                x.libelle
            ))
        } else {
            None
        };
        let Some(raison) = raison else { continue };
        en_retard += 1;
        let depuis = x
            .erreur_depuis
            .or(x.derniere_releve)
            .unwrap_or_else(OffsetDateTime::now_utc);
        if plus_vieille.as_ref().is_none_or(|p| depuis < p.depuis) {
            plus_vieille = Some(PlusVieille { depuis, raison });
        }
    }
    let n = boites.len() as i64;
    let resume = if n == 0 {
        "Aucune boîte active".into()
    } else if en_retard == 0 {
        format!(
            "{} toutes les 5 minutes",
            pluriel(n, "boîte relevée", "boîtes relevées")
        )
    } else {
        format!(
            "{} sur {n} en difficulté",
            pluriel(en_retard, "boîte", "boîtes")
        )
    };
    Ok(EtapeTraitement {
        cle: "releve",
        nom: "Relève des boîtes mail",
        etat: etat_de(&file, &None, en_retard > 0),
        resume,
        blocage: None,
        en_attente: en_retard,
        plus_vieille,
        dernier_passage: boites.iter().filter_map(|x| x.derniere_releve).max(),
        file,
        relance_demandee: false,
    })
}

async fn lecture(
    etat: &Etat,
    b: &HashMap<String, OffsetDateTime>,
    r: &reglages::Reglages,
    claude: Option<String>,
) -> Resultat<EtapeTraitement> {
    let pool = &etat.pool;
    let c = sqlx::query!(
        r#"select count(*) filter (where etat = 'a_lire') as "a_lire!",
                  count(*) filter (where etat = 'en_cours') as "en_cours!",
                  max(lu_le) as dernier
           from document"#
    )
    .fetch_one(pool)
    .await?;
    let vieux = sqlx::query!(
        r#"select id, greatest(cree_le, coalesce(lu_le, cree_le)) as "depuis!" from document
           where etat = 'a_lire' order by 2, id limit 1"#
    )
    .fetch_optional(pool)
    .await?;
    let plus_vieille = match vieux {
        Some(v) => attente::expliquer(pool, v.id)
            .await
            .map_err(ErreurApi::Interne)?
            .map(|a| PlusVieille {
                depuis: v.depuis,
                raison: a.raison,
            }),
        None => None,
    };
    let file = file_etat(b, file::FILE_LECTURE);
    // Du travail attend et Claude ne peut pas lire (jeton, refus, limite du jour) ou la
    // lecture automatique est coupée : l'étape est bloquée, pas seulement en retard.
    let blocage = if c.a_lire == 0 {
        None
    } else if !r.lecture_auto {
        Some("La lecture automatique est coupée (Réglages > Claude)".to_string())
    } else {
        claude
    };
    let en_retard = plus_vieille
        .as_ref()
        .is_some_and(|p| depuis_trop(Some(p.depuis)));
    let resume = match (c.a_lire, c.en_cours) {
        (0, 0) => "Rien à lire".into(),
        (0, e) => format!(
            "{} en cours de lecture",
            pluriel(e, "document", "documents")
        ),
        (a, 0) => format!("{} à lire", pluriel(a, "document", "documents")),
        (a, e) => format!(
            "{} à lire, {e} en cours",
            pluriel(a, "document", "documents")
        ),
    };
    Ok(EtapeTraitement {
        cle: "lecture",
        nom: "Lecture des pièces jointes",
        etat: etat_de(&file, &blocage, en_retard),
        resume,
        blocage,
        en_attente: c.a_lire + c.en_cours,
        plus_vieille,
        dernier_passage: c.dernier,
        file,
        relance_demandee: false,
    })
}

async fn tri(
    etat: &Etat,
    b: &HashMap<String, OffsetDateTime>,
    r: &reglages::Reglages,
) -> Resultat<EtapeTraitement> {
    let pool = &etat.pool;
    let c = sqlx::query!(
        r#"select count(*) filter (where tri_etat in ('a_trier', 'en_cours')) as "en_attente!",
                  max(trie_le) as dernier
           from mail"#
    )
    .fetch_one(pool)
    .await?;
    let vieux = sqlx::query!(
        r#"select recu_le, tri_message from mail
           where tri_etat in ('a_trier', 'en_cours') order by recu_le, id limit 1"#
    )
    .fetch_optional(pool)
    .await?;
    let indispo = attente::tri_indisponible(pool, r)
        .await
        .map_err(ErreurApi::Interne)?
        .map(|i| i.message);
    let plus_vieille = vieux.map(|v| PlusVieille {
        depuis: v.recu_le,
        raison: indispo
            .clone()
            .or(v.tri_message.filter(|m| !m.is_empty()))
            .unwrap_or_else(|| "Dans la file de tri".into()),
    });
    let file = file_etat(b, file::FILE_TRI);
    let blocage = if c.en_attente == 0 {
        None
    } else if !r.tri_auto {
        Some("Le tri automatique est coupé (Réglages > Claude)".to_string())
    } else {
        indispo.clone()
    };
    let en_retard = plus_vieille
        .as_ref()
        .is_some_and(|p| depuis_trop(Some(p.depuis)));
    let resume = if !r.tri_auto {
        "Tri automatique coupé (Réglages > Claude)".into()
    } else if c.en_attente == 0 {
        "Tous les mails sont triés".into()
    } else {
        format!("{} à trier", pluriel(c.en_attente, "mail", "mails"))
    };
    Ok(EtapeTraitement {
        cle: "tri",
        nom: "Tri des mails",
        etat: etat_de(&file, &blocage, en_retard),
        resume,
        blocage,
        en_attente: c.en_attente,
        plus_vieille,
        dernier_passage: c.dernier,
        file,
        relance_demandee: false,
    })
}

async fn rapprochement(
    etat: &Etat,
    b: &HashMap<String, OffsetDateTime>,
) -> Resultat<EtapeTraitement> {
    let pool = &etat.pool;
    let c = sqlx::query!(
        r#"select count(*) as "n!", min(d.lu_le) as plus_vieux
           from document d
           where d.type = 'bl' and d.etat = 'lu' and not d.rapprochement_ignore
             and not exists (select 1 from rapprochement r where r.bl_id = d.id and not r.rejete)"#
    )
    .fetch_one(pool)
    .await?;
    let dernier = sqlx::query_scalar!("select max(cree_le) from rapprochement")
        .fetch_one(pool)
        .await?;
    // Calculé juste après chaque lecture : l'étape suit la file de lecture.
    let file = file_etat(b, file::FILE_LECTURE);
    let resume = if c.n == 0 {
        "Chaque bon de livraison a sa facture".into()
    } else {
        format!(
            "{} sans facture pour l’instant (normal tant que la facture n’est pas arrivée)",
            pluriel(c.n, "bon de livraison", "bons de livraison")
        )
    };
    Ok(EtapeTraitement {
        cle: "rapprochement",
        nom: "Rapprochement BL et factures",
        etat: etat_de(&file, &None, false),
        resume,
        blocage: None,
        en_attente: c.n,
        plus_vieille: c.plus_vieux.map(|depuis| PlusVieille {
            depuis,
            raison: "Bon de livraison sans facture".into(),
        }),
        dernier_passage: dernier,
        file,
        relance_demandee: false,
    })
}

async fn depot(etat: &Etat, b: &HashMap<String, OffsetDateTime>) -> Resultat<EtapeTraitement> {
    let pool = &etat.pool;
    let c = sqlx::query!(
        r#"select count(*) filter (where (d.valide_le is not null and d.statut_depot in ('a_verifier', 'pret'))
                                     or (d.valide_le is null and d.statut_depot = 'pret'
                                         and coalesce(s.depot_auto, false))) as "a_deposer!",
                  min(coalesce(d.valide_le, d.lu_le)) filter (
                      where (d.valide_le is not null and d.statut_depot in ('a_verifier', 'pret'))
                         or (d.valide_le is null and d.statut_depot = 'pret'
                             and coalesce(s.depot_auto, false))) as plus_vieux,
                  count(*) filter (where s.adresse_depot is null
                                     and ((d.valide_le is not null and d.statut_depot in ('a_verifier', 'pret'))
                                          or (d.valide_le is null and d.statut_depot = 'pret'
                                              and coalesce(s.depot_auto, false)))) as "sans_adresse!",
                  count(*) filter (where d.statut_depot = 'erreur') as "en_erreur!"
           from document d left join societe s on s.id = d.societe_id
           where d.etat = 'lu'"#
    )
    .fetch_one(pool)
    .await?;
    let dernier = sqlx::query_scalar!("select max(quand) from envoi_depot")
        .fetch_one(pool)
        .await?;
    let file = file_etat(b, file::FILE_ENVOI);
    let smtp = parametre::lire(pool, gmflow_core::expedition::CLE)
        .await?
        .is_some();
    let blocage = if c.a_deposer == 0 {
        None
    } else if !smtp {
        Some(
            "L’adresse d’envoi de GMFlow n’est pas réglée (Réglages > Envoi vers Dext / Yooz)"
                .to_string(),
        )
    } else if c.sans_adresse > 0 {
        Some(format!(
            "{} sans adresse de collecte (Réglages > Sociétés)",
            pluriel(c.sans_adresse, "facture", "factures")
        ))
    } else {
        None
    };
    let en_retard = c.en_erreur > 0 || depuis_trop(c.plus_vieux);
    let mut resume = if c.a_deposer == 0 {
        "Rien à déposer".to_string()
    } else {
        format!("{} à déposer", pluriel(c.a_deposer, "facture", "factures"))
    };
    if c.en_erreur > 0 {
        resume.push_str(&format!(
            " ; {} : à revoir dans À valider",
            pluriel(c.en_erreur, "dépôt en échec", "dépôts en échec")
        ));
    }
    Ok(EtapeTraitement {
        cle: "depot",
        nom: "Dépôt vers Dext ou Yooz",
        etat: etat_de(&file, &blocage, en_retard),
        resume,
        blocage,
        en_attente: c.a_deposer,
        plus_vieille: c.plus_vieux.map(|depuis| PlusVieille {
            depuis,
            raison: "Facture validée, pas encore déposée".into(),
        }),
        dernier_passage: dernier,
        file,
        relance_demandee: false,
    })
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DemandeRelance {
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    demandee_le: OffsetDateTime,
}

/// Demande au worker de relancer une étape tout de suite ; il la prend dans les secondes qui
/// suivent.
pub async fn relancer(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
    Path(etape): Path<String>,
) -> Resultat<(StatusCode, Json<DemandeRelance>)> {
    if !ETAPES.contains(&etape.as_str()) {
        return Err(ErreurApi::Absent);
    }
    let demandee_le = traitements::demander(&etat.pool, &etape).await?;
    journal::ecrire(
        &etat.pool,
        Some(acces.utilisateur_id),
        "traitement_relance_demandee",
        "traitement",
        Some(etape),
        json!({}),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(DemandeRelance { demandee_le })))
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToutRelire {
    documents: usize,
}

/// Remet en lecture tous les documents en attente ou illisibles qui n'ont pas déjà une tâche
/// en file (les documents écartés ne bougent pas). Priorité de l'arriéré, limite du jour
/// comprise : les mails récents et les demandes « Lire maintenant » passent devant.
pub async fn tout_relire(
    AccesAdmin(acces): AccesAdmin,
    State(etat): State<Etat>,
) -> Resultat<(StatusCode, Json<ToutRelire>)> {
    let pool = &etat.pool;
    let en_file = file::documents_en_file(pool).await?;
    let ids = sqlx::query_scalar!(
        "update document set etat = 'a_lire', planifie_le = now(), message = null
         where etat in ('a_lire', 'erreur') and id <> all($1)
           and coalesce(statut_depot, '') <> 'en_envoi'
         returning id",
        &en_file
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        file::pousser_lecture_auto(pool, *id, file::PRIORITE_ARRIERE)
            .await
            .map_err(ErreurApi::Interne)?;
    }
    journal::ecrire(
        pool,
        Some(acces.utilisateur_id),
        "tout_relire",
        "traitement",
        Some("lecture".into()),
        json!({ "documents": ids.len() }),
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ToutRelire {
            documents: ids.len(),
        }),
    ))
}
