//! Relève d'une boîte : bail en base, puis pour chaque dossier surveillé, lecture des nouveaux
//! messages par paquets de 50 au plus et de 64 Mo cumulés au plus (sans les marquer lus), une
//! transaction par paquet. Chaque échange avec le serveur a un délai : un serveur muet fait
//! échouer la relève (erreur notée, alerte) au lieu de garder la tâche pour toujours.

use std::collections::HashSet;
use std::future::Future;
use std::time::Duration;

use futures::TryStreamExt;
use gmflow_core::eml::{self, MailDecoupe};
use gmflow_core::imap::{self, ErreurImap, Session};
use gmflow_core::{journal, stockage};
use serde_json::json;
use sqlx::{PgConnection, PgPool};

use crate::Contexte;

pub const PAQUET: usize = 50;
/// Taille cumulée d'un paquet (selon `RFC822.SIZE`) ; un message plus gros part seul.
pub const PAQUET_OCTETS: u64 = 64 * 1024 * 1024;
/// UID par demande de tailles.
const TAILLES_PAR_DEMANDE: usize = 500;
pub const PAUSE_APRES: i32 = 3;
/// Délai d'une commande courte (EXAMINE, SEARCH, début d'un FETCH, tailles).
pub const DELAI_COMMANDE: Duration = Duration::from_secs(60);
/// Délai entre deux messages d'un FETCH (un message de 50 Mo à 500 ko/s y tient).
pub const DELAI_MESSAGE: Duration = Duration::from_secs(120);

/// Délai réellement appliqué : `d`, ou `GMFLOW_TEST_DELAI_IMAP` secondes dans un build de test.
fn delai(d: Duration) -> Duration {
    #[cfg(feature = "test-utils")]
    if let Some(s) = std::env::var("GMFLOW_TEST_DELAI_IMAP")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return Duration::from_secs(s);
    }
    d
}

/// Erreur d'un dossier ; `session_perdue` : délai dépassé au milieu d'un échange, la session ne
/// peut plus servir aux dossiers suivants.
struct ErreurDossier {
    erreur: ErreurImap,
    session_perdue: bool,
}

impl From<ErreurImap> for ErreurDossier {
    fn from(erreur: ErreurImap) -> Self {
        Self {
            erreur,
            session_perdue: false,
        }
    }
}

impl From<async_imap::error::Error> for ErreurDossier {
    fn from(e: async_imap::error::Error) -> Self {
        ErreurImap::from(e).into()
    }
}

fn delai_depasse(quoi: &str, d: Duration) -> ErreurDossier {
    ErreurDossier {
        erreur: ErreurImap::Autre(format!(
            "le serveur ne répond plus ({quoi}, {} s)",
            d.as_secs()
        )),
        session_perdue: true,
    }
}

/// Une commande IMAP avec délai.
async fn avec_delai<T>(
    quoi: &str,
    d: Duration,
    f: impl Future<Output = Result<T, async_imap::error::Error>>,
) -> Result<T, ErreurDossier> {
    let d = delai(d);
    match tokio::time::timeout(d, f).await {
        Ok(r) => Ok(r?),
        Err(_) => Err(delai_depasse(quoi, d)),
    }
}

/// Découpe les UID (dans l'ordre) en paquets d'au plus `max_n` messages et `max_octets` cumulés ;
/// un message plus gros que `max_octets` forme un paquet à lui seul. Taille inconnue : la part
/// moyenne d'un paquet.
pub fn paquets(uids: &[(u32, Option<u64>)], max_n: usize, max_octets: u64) -> Vec<Vec<u32>> {
    let defaut = max_octets / max_n.max(1) as u64;
    let mut v: Vec<Vec<u32>> = Vec::new();
    let mut courant: Vec<u32> = Vec::new();
    let mut taille = 0u64;
    for &(uid, t) in uids {
        let t = t.unwrap_or(defaut);
        if !courant.is_empty() && (courant.len() >= max_n || taille + t > max_octets) {
            v.push(std::mem::take(&mut courant));
            taille = 0;
        }
        courant.push(uid);
        taille += t;
    }
    if !courant.is_empty() {
        v.push(courant);
    }
    v
}

fn ensemble(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Tailles (`RFC822.SIZE`) des messages, par demandes de 500 UID.
async fn tailles(
    session: &mut Session,
    uids: &[u32],
) -> Result<Vec<(u32, Option<u64>)>, ErreurDossier> {
    let mut connues = std::collections::HashMap::new();
    for morceau in uids.chunks(TAILLES_PAR_DEMANDE) {
        let flux = avec_delai(
            "FETCH RFC822.SIZE",
            DELAI_COMMANDE,
            session.uid_fetch(ensemble(morceau), "(UID RFC822.SIZE)"),
        )
        .await?;
        let lignes: Vec<_> =
            avec_delai("FETCH RFC822.SIZE", DELAI_COMMANDE, flux.try_collect()).await?;
        for f in &lignes {
            if let (Some(uid), Some(t)) = (f.uid, f.size) {
                connues.insert(uid, u64::from(t));
            }
        }
    }
    Ok(uids.iter().map(|u| (*u, connues.get(u).copied())).collect())
}

/// Corps des messages d'un paquet. Chaque réponse du serveur est copiée puis lâchée aussitôt :
/// le paquet n'est en mémoire qu'une fois.
async fn corps(session: &mut Session, uids: &[u32]) -> Result<Vec<(u32, Vec<u8>)>, ErreurDossier> {
    let flux = avec_delai(
        "FETCH",
        DELAI_COMMANDE,
        session.uid_fetch(ensemble(uids), "(UID BODY.PEEK[])"),
    )
    .await?;
    let mut flux = std::pin::pin!(flux);
    let mut messages = Vec::with_capacity(uids.len());
    loop {
        let f = match tokio::time::timeout(delai(DELAI_MESSAGE), flux.try_next()).await {
            Err(_) => return Err(delai_depasse("FETCH", delai(DELAI_MESSAGE))),
            Ok(r) => r?,
        };
        let Some(f) = f else { break };
        if let (Some(uid), Some(b)) = (f.uid, f.body()) {
            messages.push((uid, b.to_vec()));
        }
    }
    Ok(messages)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    /// Faux si une autre relève tenait déjà le bail : rien n'a été fait.
    pub bail_obtenu: bool,
    /// Messages lus sur le serveur (FETCH).
    pub messages_lus: usize,
    /// Nouvelles lignes `mail`.
    pub nouveaux_mails: usize,
}

struct Boite {
    id: i64,
    serveur: String,
    port: i32,
    identifiant: String,
    mot_de_passe_chiffre: Vec<u8>,
    dossiers: Vec<String>,
    reprise_mois: i32,
    /// Posée par la réinitialisation : la reprise ne remonte pas avant ce jour.
    reprise_depuis: Option<time::Date>,
}

/// Prend le bail (15 min) si personne ne le tient. Ne relève qu'une boîte active et non en pause.
async fn prendre_bail(pool: &PgPool, id: i64) -> sqlx::Result<Option<Boite>> {
    sqlx::query_as!(
        Boite,
        "update boite_mail set releve_jusqu_a = now() + interval '15 minutes'
         where id = $1 and active and not pause_auto
           and (releve_jusqu_a is null or releve_jusqu_a < now())
         returning id, serveur, port, identifiant, mot_de_passe_chiffre, dossiers, reprise_mois, reprise_depuis",
        id
    )
    .fetch_optional(pool)
    .await
}

async fn prolonger_bail(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "update boite_mail set releve_jusqu_a = now() + interval '15 minutes' where id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn relever(ctx: &Contexte, boite_id: i64) -> anyhow::Result<Bilan> {
    let Some(boite) = prendre_bail(&ctx.pool, boite_id).await? else {
        tracing::info!(
            boite_id,
            "relève ignorée : bail déjà pris, boîte inactive ou en pause"
        );
        return Ok(Bilan::default());
    };
    let mut bilan = Bilan {
        bail_obtenu: true,
        ..Default::default()
    };
    let resultat = relever_avec_bail(ctx, &boite, &mut bilan).await;

    match &resultat {
        Ok(()) => {
            sqlx::query!(
                "update boite_mail set derniere_releve = now(), derniere_erreur = null, erreur_depuis = null,
                     echecs_auth = 0, releve_jusqu_a = null
                 where id = $1",
                boite.id
            )
            .execute(&ctx.pool)
            .await?;
        }
        Err(e) => {
            let refus = matches!(e, ErreurImap::RefusAuth(_));
            let message: String = e.to_string().chars().take(500).collect();
            let pause = sqlx::query_scalar!(
                "update boite_mail set derniere_erreur = $2, erreur_depuis = coalesce(erreur_depuis, now()),
                     echecs_auth = case when $3 then echecs_auth + 1 else echecs_auth end,
                     pause_auto = pause_auto or ($3 and echecs_auth + 1 >= $4),
                     releve_jusqu_a = null
                 where id = $1
                 returning pause_auto",
                boite.id,
                message,
                refus,
                PAUSE_APRES
            )
            .fetch_one(&ctx.pool)
            .await?;
            if pause {
                journal::ecrire(
                    &ctx.pool,
                    None,
                    "pause_auto",
                    "boite_mail",
                    Some(boite.id.to_string()),
                    json!({ "message": message }),
                )
                .await?;
            }
            tracing::warn!(boite_id, erreur = %message, "relève en échec");
        }
    }
    resultat?;
    Ok(bilan)
}

async fn relever_avec_bail(
    ctx: &Contexte,
    boite: &Boite,
    bilan: &mut Bilan,
) -> Result<(), ErreurImap> {
    let mdp = ctx
        .chiffreur
        .dechiffrer_texte(&boite.mot_de_passe_chiffre)
        .map_err(|e| ErreurImap::Autre(e.to_string()))?;
    let port = u16::try_from(boite.port).map_err(|_| ErreurImap::Autre("port invalide".into()))?;
    let mut session = imap::connecter(
        &boite.serveur,
        port,
        &boite.identifiant,
        &mdp,
        ctx.transport,
    )
    .await?;

    let mut erreurs = Vec::new();
    for dossier in &boite.dossiers {
        if let Err(e) = relever_dossier(ctx, boite, &mut session, dossier, bilan).await {
            erreurs.push(format!("{dossier} : {}", e.erreur));
            if e.session_perdue {
                // Session dans un état inconnu : les dossiers suivants attendront le tour suivant.
                return Err(ErreurImap::Autre(erreurs.join(" ; ")));
            }
        }
    }
    imap::deconnecter(&mut session).await;
    if erreurs.is_empty() {
        Ok(())
    } else {
        Err(ErreurImap::Autre(erreurs.join(" ; ")))
    }
}

fn erreur_base(e: impl std::fmt::Display) -> ErreurImap {
    ErreurImap::Autre(format!("base : {e}"))
}

/// Aujourd'hui moins `mois` mois.
fn jour_reprise(mois: i32) -> time::Date {
    let d = time::OffsetDateTime::now_utc().date();
    let total = d.year() * 12 + (d.month() as i32 - 1) - mois;
    let (annee, mois_num) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let mois_t = time::Month::try_from(mois_num as u8).expect("mois valide");
    let jour = d.day().min(28);
    time::Date::from_calendar_date(annee, mois_t, jour).expect("date valide")
}

/// Date au format IMAP (`2-Nov-2026`).
pub fn date_imap(date: time::Date) -> String {
    const NOMS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}-{}-{}",
        date.day(),
        NOMS[date.month() as usize - 1],
        date.year()
    )
}

/// Date IMAP (`SINCE`) : aujourd'hui moins `mois` mois.
pub fn date_reprise(mois: i32) -> String {
    date_imap(jour_reprise(mois))
}

/// Début d'une relève sans dossier relevé : `reprise_mois` en arrière, mais jamais avant le
/// jour de la réinitialisation (`reprise_depuis`).
pub fn debut_releve(mois: i32, depuis: Option<time::Date>) -> String {
    let d = jour_reprise(mois);
    date_imap(depuis.map_or(d, |r| r.max(d)))
}

async fn relever_dossier(
    ctx: &Contexte,
    boite: &Boite,
    session: &mut Session,
    dossier: &str,
    bilan: &mut Bilan,
) -> Result<(), ErreurDossier> {
    // EXAMINE : dossier en lecture seule, le serveur ne pose jamais \Seen.
    let infos = avec_delai("EXAMINE", DELAI_COMMANDE, session.examine(dossier)).await?;
    let uidvalidity = infos.uid_validity.map(i64::from).unwrap_or(0);
    let uid_next = infos.uid_next.map(i64::from);

    let etat = sqlx::query!(
        "select uidvalidity, dernier_uid from boite_dossier where boite_id = $1 and nom = $2",
        boite.id,
        dossier
    )
    .fetch_optional(&ctx.pool)
    .await
    .map_err(erreur_base)?;

    let mut dernier = match etat {
        Some(e) if e.uidvalidity == uidvalidity => e.dernier_uid,
        autre => {
            let mut tx = ctx.pool.begin().await.map_err(erreur_base)?;
            if autre.is_some() {
                // UID tous changés : on garde les liens, sans UID, et on relit la période.
                sqlx::query!(
                    "update mail_boite set uid = null where boite_id = $1 and dossier = $2",
                    boite.id,
                    dossier
                )
                .execute(&mut *tx)
                .await
                .map_err(erreur_base)?;
                journal::ecrire(
                    &mut *tx,
                    None,
                    "uidvalidity_change",
                    "boite_mail",
                    Some(boite.id.to_string()),
                    json!({ "dossier": dossier }),
                )
                .await
                .map_err(erreur_base)?;
            }
            sqlx::query!(
                "insert into boite_dossier (boite_id, nom, uidvalidity, dernier_uid) values ($1, $2, $3, 0)
                 on conflict (boite_id, nom) do update set uidvalidity = excluded.uidvalidity, dernier_uid = 0",
                boite.id,
                dossier,
                uidvalidity
            )
            .execute(&mut *tx)
            .await
            .map_err(erreur_base)?;
            tx.commit().await.map_err(erreur_base)?;
            0
        }
    };

    let requete = if dernier == 0 {
        format!(
            "SINCE {}",
            debut_releve(boite.reprise_mois, boite.reprise_depuis)
        )
    } else {
        format!("UID {}:*", dernier + 1)
    };
    let trouves: HashSet<u32> =
        avec_delai("SEARCH", DELAI_COMMANDE, session.uid_search(&requete)).await?;
    let mut uids: Vec<u32> = trouves
        .into_iter()
        .filter(|u| i64::from(*u) > dernier)
        .collect();
    uids.sort_unstable();

    let decoupe = if uids.is_empty() {
        Vec::new()
    } else {
        paquets(&tailles(session, &uids).await?, PAQUET, PAQUET_OCTETS)
    };
    for paquet in &decoupe {
        prolonger_bail(&ctx.pool, boite.id)
            .await
            .map_err(erreur_base)?;
        let messages = corps(session, paquet).await?;
        bilan.messages_lus += messages.len();
        let max = paquet
            .iter()
            .copied()
            .max()
            .map(i64::from)
            .unwrap_or(dernier);
        let nouveaux = enregistrer_paquet(ctx, boite.id, dossier, messages, max)
            .await
            .map_err(erreur_base)?;
        bilan.nouveaux_mails += nouveaux;
        dernier = max;
        if nouveaux > 0 {
            crate::lecture::pompe_apres_releve(&ctx.pool).await;
            crate::tri::pompe_apres_releve(&ctx.pool).await;
        }
    }

    // Dossier vide ou sans message dans la période : tout ce qui existe est considéré comme vu.
    if uids.is_empty()
        && let Some(n) = uid_next
        && n - 1 > dernier
    {
        sqlx::query!(
            "update boite_dossier set dernier_uid = $3 where boite_id = $1 and nom = $2",
            boite.id,
            dossier,
            n - 1
        )
        .execute(&ctx.pool)
        .await
        .map_err(erreur_base)?;
    }
    Ok(())
}

fn decoupe_de_secours(brut: &[u8]) -> MailDecoupe {
    MailDecoupe {
        message_id: format!("sha256:{}", hex::encode(eml::sha256(brut))),
        expediteur: String::new(),
        destinataires: Vec::new(),
        objet: "(message illisible)".into(),
        date: None,
        corps_texte: String::new(),
        pieces: Vec::new(),
        empreinte_contenu: eml::sha256(brut),
    }
}

/// Écrit un paquet dans une transaction et avance `dernier_uid`. Rend le nombre de nouveaux mails.
async fn enregistrer_paquet(
    ctx: &Contexte,
    boite_id: i64,
    dossier: &str,
    messages: Vec<(u32, Vec<u8>)>,
    max_uid: i64,
) -> anyhow::Result<usize> {
    let racine = ctx.config.fichiers.clone();
    let mut tx = ctx.pool.begin().await?;
    let mut nouveaux = 0;
    for (uid, brut) in messages {
        // Le message part dans la tâche d'écriture et en revient : pas de copie.
        let (chemin, sha, brut) = {
            let racine = racine.clone();
            tokio::task::spawn_blocking(move || {
                stockage::ecrire_eml(&racine, &brut).map(|(c, s)| (c, s, brut))
            })
            .await??
        };
        let brut = &brut[..];
        let d = eml::decouper(brut).unwrap_or_else(|_| decoupe_de_secours(brut));
        let (mail_id, nouveau) =
            inserer_mail(&mut tx, &d, &chemin, &sha, brut.len() as i64).await?;
        if nouveau {
            nouveaux += 1;
        }
        rattacher(&mut tx, mail_id, boite_id, dossier, i64::from(uid)).await?;
    }
    sqlx::query!(
        "update boite_dossier set dernier_uid = greatest(dernier_uid, $3) where boite_id = $1 and nom = $2",
        boite_id,
        dossier,
        max_uid
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(nouveaux)
}

async fn inserer_mail(
    tx: &mut PgConnection,
    d: &MailDecoupe,
    chemin: &str,
    sha: &[u8],
    taille: i64,
) -> anyhow::Result<(i64, bool)> {
    let insere = sqlx::query_scalar!(
        "insert into mail (message_id, empreinte_contenu, expediteur, destinataires, objet, date_mail,
                           corps_texte, chemin_eml, sha256, taille)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         on conflict (message_id, empreinte_contenu) do nothing
         returning id",
        d.message_id,
        d.empreinte_contenu,
        d.expediteur,
        &d.destinataires,
        d.objet,
        d.date,
        d.corps_texte.replace('\0', ""),
        chemin,
        sha,
        taille
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = insere else {
        let id = sqlx::query_scalar!(
            "select id from mail where message_id = $1 and empreinte_contenu = $2",
            d.message_id,
            d.empreinte_contenu
        )
        .fetch_one(&mut *tx)
        .await?;
        return Ok((id, false));
    };
    for p in &d.pieces {
        sqlx::query!(
            "insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256) values ($1, $2, $3, $4, $5, $6)",
            id,
            p.rang,
            p.nom,
            p.type_mime,
            p.taille,
            p.sha256
        )
        .execute(&mut *tx)
        .await?;
    }
    let autres = sqlx::query_scalar!(
        "select count(*) as \"n!\" from mail where message_id = $1 and id <> $2",
        d.message_id,
        id
    )
    .fetch_one(&mut *tx)
    .await?;
    if autres > 0 && !d.message_id.starts_with("sha256:") {
        journal::ecrire(
            &mut *tx,
            None,
            "message_id_reutilise",
            "mail",
            Some(id.to_string()),
            json!({ "message_id": d.message_id }),
        )
        .await?;
    }
    Ok((id, true))
}

/// Lien boîte/dossier/UID. Après un changement d'UIDVALIDITY, la ligne sans UID du même mail
/// reçoit le nouvel UID ; sinon une ligne est ajoutée.
async fn rattacher(
    tx: &mut PgConnection,
    mail_id: i64,
    boite_id: i64,
    dossier: &str,
    uid: i64,
) -> anyhow::Result<()> {
    let maj = sqlx::query!(
        "update mail_boite set uid = $4
         where id = (select id from mail_boite
                     where mail_id = $1 and boite_id = $2 and dossier = $3 and uid is null
                     order by id limit 1)",
        mail_id,
        boite_id,
        dossier,
        uid
    )
    .execute(&mut *tx)
    .await?;
    if maj.rows_affected() == 0 {
        sqlx::query!(
            "insert into mail_boite (mail_id, boite_id, dossier, uid) values ($1, $2, $3, $4)
             on conflict (boite_id, dossier, uid) do nothing",
            mail_id,
            boite_id,
            dossier,
            uid
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}
