//! File « À traiter » de l'accueil : *Bloqué* (envois en échec pas encore relancés, doublons, documents
//! illisibles ; pour un administrateur, boîtes en panne et Claude déconnecté), *À valider*
//! (factures, avoirs et BL lus pas encore validés, par urgence : `rang` de `AValider`) et
//! *À répondre* (vraies questions, par priorité). Un responsable ne voit que sa société : les
//! documents d'une boîte de sa société, rangés dans sa société ou pas encore rangés.
//!
//! Les conditions des trois sections sont écrites plusieurs fois (liste, compteurs, suivant) :
//! les requêtes vérifiées à la compilation ne se composent pas. Toute modification se fait
//! partout à la fois (les tests comparent les compteurs aux listes).

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::acces::Acces;
use crate::erreur::Resultat;
use crate::etat::Etat;

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

/// Éléments rendus au plus par section (les compteurs restent exacts).
pub const LIMITE: i64 = 200;

#[derive(Deserialize)]
pub struct Filtre {
    societe: Option<i64>,
}

#[derive(Serialize)]
pub struct Compteurs {
    bloque: i64,
    a_valider: i64,
    /// Détail de `a_valider` par type : badges de Factures, Avoirs et BL.
    a_valider_types: ParType,
    a_repondre: i64,
    total: i64,
}

#[derive(Serialize)]
pub struct ParType {
    facture: i64,
    avoir: i64,
    bl: i64,
}

#[derive(Serialize)]
pub struct Bloque {
    /// `envoi_erreur`, `doublon`, `illisible`, `boite` ou `claude`.
    genre: String,
    document_id: Option<i64>,
    boite_id: Option<i64>,
    /// Libellé de la boîte, ou nom du fichier du document.
    libelle: Option<String>,
    /// Pour une boîte : `pause_auto` (refus d'authentification répétés) ou `erreur`.
    motif: Option<String>,
    /// Message d'erreur ou alerte, tel qu'enregistré.
    message: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    depuis: Option<time::OffsetDateTime>,
    r#type: Option<String>,
    fournisseur: Option<String>,
    numero: Option<String>,
    societe_id: Option<i64>,
    societe: Option<String>,
}

#[derive(Serialize)]
pub struct AValider {
    id: i64,
    mail_id: i64,
    r#type: String,
    fournisseur_id: Option<i64>,
    /// Fiche fournisseur, à défaut nom lu sur le document.
    fournisseur: Option<String>,
    fournisseur_a_confirmer: bool,
    numero: Option<String>,
    societe_id: Option<i64>,
    societe: Option<String>,
    #[serde(with = "date_iso::option")]
    date_document: Option<time::Date>,
    #[serde(with = "time::serde::rfc3339")]
    recu_le: time::OffsetDateTime,
    montant_ttc: Option<f64>,
    remise_ht: Option<f64>,
    /// Articles en écart avec les BL liés (manquant, en trop, prix changé).
    articles_ecart: i64,
    /// Première alerte en une phrase (contrôle en échec, fournisseur inconnu d'un BL).
    alerte: Option<String>,
    /// Code stable de cette alerte (`doublon`, `montants`, `societe`, `fournisseur`...).
    alerte_code: Option<String>,
    nb_alertes: i64,
    statut_depot: Option<String>,
    /// Déjà vérifié (facture vérifiée mais pas encore envoyée).
    verifie: bool,
    #[serde(with = "date_iso::option")]
    date_echeance: Option<time::Date>,
    /// Urgence (ordre de la file) : 1 facture à payer dans `JOURS_ECHEANCE` jours ou moins
    /// (échéance dépassée comprise), 2 validable d'un clic (aucune alerte), 3 à corriger
    /// (alerte). Dans le rang 1 l'échéance la plus proche d'abord, sinon la plus ancienne reçue.
    rang: i32,
}

/// Échéance proche : une facture due dans `JOURS_ECHEANCE` jours ou moins passe en tête.
pub const JOURS_ECHEANCE: i32 = 7;

#[derive(Serialize)]
pub struct ARepondre {
    id: i64,
    expediteur: String,
    objet: String,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
    resume: Option<String>,
    priorite: Option<String>,
    societes: Vec<String>,
    /// Fiche fournisseur dont un domaine est celui de l'expéditeur (« Sodistra : demande le RIB »).
    fournisseur: Option<String>,
    /// Réponse proposée en cours (`generation`, `propose`, `modifie`, `envoi`).
    brouillon: Option<String>,
}

#[derive(Serialize)]
pub struct File {
    compteurs: Compteurs,
    bloque: Vec<Bloque>,
    a_valider: Vec<AValider>,
    a_repondre: Vec<ARepondre>,
    /// Premier document à valider (bouton « Commencer »).
    suivant: Option<i64>,
}

/// État de Claude vu comme un blocage (administrateurs) : rien si la connexion marche.
async fn claude_bloque(etat: &Etat) -> Resultat<Option<Bloque>> {
    let v = gmflow_core::parametre::lire(&etat.pool, gmflow_core::parametre::CLAUDE_ETAT)
        .await?
        .unwrap_or_else(|| serde_json::json!({ "etat": "absent" }));
    let e = v.get("etat").and_then(Value::as_str).unwrap_or("absent");
    if e != "absent" && e != "erreur" {
        return Ok(None);
    }
    let depuis = v.get("teste_le").and_then(Value::as_str).and_then(|t| {
        time::OffsetDateTime::parse(t, &time::format_description::well_known::Rfc3339).ok()
    });
    // Refus noté pendant une lecture ou un tri (limite d'usage, jeton refusé) : la phrase
    // complète, heure de reprise comprise.
    let refus = v.get("refus").and_then(Value::as_str).map(str::to_string);
    let message = match &refus {
        Some(_) => Some(
            gmflow_core::attente::refus_en_vigueur(&v, time::OffsetDateTime::now_utc())
                .unwrap_or_else(|| {
                    "Claude refusait les appels : nouvel essai au prochain document".into()
                }),
        ),
        None => v
            .get("message")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(str::to_string),
    };
    Ok(Some(Bloque {
        genre: "claude".into(),
        document_id: None,
        boite_id: None,
        libelle: None,
        motif: Some(refus.unwrap_or_else(|| e.to_string())),
        message,
        depuis,
        r#type: None,
        fournisseur: None,
        numero: None,
        societe_id: None,
        societe: None,
    }))
}

async fn compter(etat: &Etat, acces: &Acces, societe: Option<i64>) -> Resultat<Compteurs> {
    let s = acces.societe_filtre();
    let docs = sqlx::query!(
        r#"select
             count(*) filter (where (d.statut_depot = 'erreur'
                                   and not coalesce(d.valide_le > (select max(e.quand) from envoi_depot e
                                                                   where e.document_id = d.id)
                                                    and d.valide_le > now() - interval '30 minutes', false))
                                 or (d.statut_depot = 'doublon' and d.verifie_le is null)
                                 or d.etat = 'erreur') as "bloque!",
             count(*) filter (where d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
                                and ((d.verifie_le is null
                                      and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                     or (d.verifie_le is not null and d.valide_le is null
                                         and d.statut_depot in ('a_verifier', 'pret')))) as "a_valider!",
             count(*) filter (where d.etat = 'lu' and d.type = 'facture'
                                and ((d.verifie_le is null
                                      and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                     or (d.verifie_le is not null and d.valide_le is null
                                         and d.statut_depot in ('a_verifier', 'pret')))) as "facture!",
             count(*) filter (where d.etat = 'lu' and d.type = 'avoir'
                                and ((d.verifie_le is null
                                      and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                     or (d.verifie_le is not null and d.valide_le is null
                                         and d.statut_depot in ('a_verifier', 'pret')))) as "avoir!",
             count(*) filter (where d.etat = 'lu' and d.type = 'bl'
                                and ((d.verifie_le is null
                                      and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                                     or (d.verifie_le is not null and d.valide_le is null
                                         and d.statut_depot in ('a_verifier', 'pret')))) as "bl!"
           from document d
           where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))"#,
        s,
        societe
    )
    .fetch_one(&etat.pool)
    .await?;
    let a_repondre = sqlx::query_scalar!(
        r#"select count(*) as "n!" from mail m
           where m.statut_reponse = 'a_repondre'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2))"#,
        s,
        societe
    )
    .fetch_one(&etat.pool)
    .await?;
    let mut bloque = docs.bloque;
    if acces.est_admin() {
        bloque += sqlx::query_scalar!(
            r#"select count(*) as "n!" from boite_mail
               where ($1::bigint is null or societe_id = $1)
                 and (pause_auto or (active and erreur_depuis < now() - interval '30 minutes'))"#,
            societe
        )
        .fetch_one(&etat.pool)
        .await?;
        if claude_bloque(etat).await?.is_some() {
            bloque += 1;
        }
    }
    Ok(Compteurs {
        bloque,
        a_valider: docs.a_valider,
        a_valider_types: ParType {
            facture: docs.facture,
            avoir: docs.avoir,
            bl: docs.bl,
        },
        a_repondre,
        total: bloque + docs.a_valider + a_repondre,
    })
}

/// `GET /api/a-traiter/compteurs` : badge du menu et pastille d'état.
pub async fn compteurs(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<Compteurs>> {
    Ok(Json(compter(&etat, &acces, f.societe).await?))
}

/// `GET /api/a-traiter` : les trois sections, déjà triées.
pub async fn file(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtre>,
) -> Resultat<Json<File>> {
    let s = acces.societe_filtre();
    let societe = f.societe;
    let mut bloque = sqlx::query_as!(
        Bloque,
        r#"select case when d.statut_depot = 'erreur' then 'envoi_erreur'
                       when d.statut_depot = 'doublon' then 'doublon'
                       else 'illisible' end as "genre!",
                  d.id as "document_id?", null::bigint as boite_id, p.nom as "libelle?",
                  null::text as motif,
                  case when d.statut_depot = 'erreur'
                         then (select e.message from envoi_depot e where e.document_id = d.id
                               order by e.quand desc limit 1)
                       when d.statut_depot = 'doublon'
                         then (select c->>'message' from jsonb_array_elements(d.controles) c
                               where c->>'code' = 'doublon' limit 1)
                       else d.message end as message,
                  case when d.statut_depot = 'erreur'
                         then (select max(e.quand) from envoi_depot e where e.document_id = d.id)
                       when d.statut_depot = 'doublon' then d.lu_le
                       else coalesce(d.pris_le, m.recu_le) end as depuis,
                  d.type, coalesce(fo.nom, d.fournisseur_nom) as fournisseur, d.numero,
                  d.societe_id, s.raison_sociale as "societe?"
           from document d
           join mail m on m.id = d.mail_id
           join piece_jointe p on p.id = d.piece_jointe_id
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where ((d.statut_depot = 'erreur'
                                   and not coalesce(d.valide_le > (select max(e.quand) from envoi_depot e
                                                                   where e.document_id = d.id)
                                                    and d.valide_le > now() - interval '30 minutes', false))
                  or (d.statut_depot = 'doublon' and d.verifie_le is null)
                  or d.etat = 'erreur')
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))
           order by case when d.statut_depot = 'erreur' then 0
                         when d.statut_depot = 'doublon' then 1 else 2 end,
                    coalesce(m.date_mail, m.recu_le), d.id
           limit $3"#,
        s,
        societe,
        LIMITE
    )
    .fetch_all(&etat.pool)
    .await?;
    if acces.est_admin() {
        let boites = sqlx::query_as!(
            Bloque,
            r#"select 'boite' as "genre!", null::bigint as document_id, b.id as "boite_id?",
                      b.libelle as "libelle?",
                      case when b.pause_auto then 'pause_auto' else 'erreur' end as motif,
                      b.derniere_erreur as message, b.erreur_depuis as depuis,
                      null::text as type, null::text as fournisseur, null::text as numero,
                      b.societe_id as "societe_id?", s.raison_sociale as "societe?"
               from boite_mail b join societe s on s.id = b.societe_id
               where ($1::bigint is null or b.societe_id = $1)
                 and (b.pause_auto or (b.active and b.erreur_depuis < now() - interval '30 minutes'))
               order by b.libelle"#,
            societe
        )
        .fetch_all(&etat.pool)
        .await?;
        // Ce qui empêche tout de tourner d'abord : Claude, puis les boîtes.
        let mut tete: Vec<Bloque> = claude_bloque(&etat).await?.into_iter().collect();
        tete.extend(boites);
        tete.append(&mut bloque);
        bloque = tete;
    }
    let a_valider = sqlx::query_as!(
        AValider,
        r#"select d.id, d.mail_id, d.type as "type!", d.fournisseur_id,
                  coalesce(fo.nom, d.fournisseur_nom) as fournisseur,
                  coalesce(fo.statut = 'a_confirmer', false) as "fournisseur_a_confirmer!",
                  d.numero, d.societe_id, s.raison_sociale as "societe?", d.date_document,
                  coalesce(m.date_mail, m.recu_le) as "recu_le!",
                  d.montant_ttc::float8 as montant_ttc, d.remise_ht::float8 as remise_ht,
                  (select count(*) from rapprochement r cross join jsonb_array_elements(r.ecarts) e
                   where not r.rejete and (r.facture_id = d.id or r.bl_id = d.id)) as "articles_ecart!",
                  case when jsonb_array_length(d.controles) > 0 then d.controles->0->>'message'
                       when d.type = 'bl' and d.fournisseur_id is null then 'Fournisseur non identifié'
                  end as alerte,
                  case when jsonb_array_length(d.controles) > 0 then d.controles->0->>'code'
                       when d.type = 'bl' and d.fournisseur_id is null then 'fournisseur'
                  end as alerte_code,
                  (jsonb_array_length(d.controles)
                   + case when d.type = 'bl' and d.fournisseur_id is null then 1 else 0 end)::bigint as "nb_alertes!",
                  d.statut_depot, d.verifie_le is not null as "verifie!", d.date_echeance, u.rang as "rang!"
           from document d
           join mail m on m.id = d.mail_id
           cross join lateral (
               select case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $4::int then 1
                           when not (jsonb_array_length(d.controles) > 0 or (d.type = 'bl' and d.fournisseur_id is null)) then 2
                           else 3 end as rang,
                      case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $4::int
                           then d.date_echeance::timestamp at time zone 'Europe/Paris'
                           else coalesce(m.date_mail, m.recu_le) end as cle
           ) u
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ((d.verifie_le is null
                   and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                  or (d.verifie_le is not null and d.valide_le is null
                      and d.statut_depot in ('a_verifier', 'pret')))
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))
           order by u.rang, u.cle, d.id
           limit $3"#,
        s,
        societe,
        LIMITE,
        JOURS_ECHEANCE
    )
    .fetch_all(&etat.pool)
    .await?;
    let a_repondre = sqlx::query_as!(
        ARepondre,
        r#"select m.id, m.expediteur, m.objet, m.date_mail, m.resume, m.priorite,
               array(select distinct s.raison_sociale from mail_boite_vue mb
                     join boite_mail b on b.id = mb.boite_id join societe s on s.id = b.societe_id
                     where mb.mail_id = m.id and ($1::bigint is null or b.societe_id = $1)
                     order by s.raison_sociale) as "societes!",
               (select f.nom from fournisseur f
                where f.fusionne_dans is null
                  and lower(split_part(m.expediteur, '@', 2)) = any(f.domaines)
                order by f.id limit 1) as fournisseur,
               (select br.statut from brouillon_reponse br
                where br.mail_id = m.id and br.statut in ('generation', 'propose', 'modifie', 'envoi')
                limit 1) as brouillon
           from mail m
           where m.statut_reponse = 'a_repondre'
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = m.id
                           and ($1::bigint is null or b.societe_id = $1)
                           and ($2::bigint is null or b.societe_id = $2))
           order by case coalesce(m.priorite, 'normale') when 'haute' then 0 when 'normale' then 1 else 2 end,
                    coalesce(m.date_mail, m.recu_le), m.id
           limit $3"#,
        s,
        societe,
        LIMITE
    )
    .fetch_all(&etat.pool)
    .await?;
    let compteurs = compter(&etat, &acces, societe).await?;
    let suivant = a_valider.first().map(|d| d.id);
    Ok(Json(File {
        compteurs,
        bloque,
        a_valider,
        a_repondre,
        suivant,
    }))
}

/// Place d'un document dans l'ordre de la file « À valider » : (rang d'urgence, échéance ou
/// réception). Lue avant une validation, pour donner ensuite le document qui le suivait.
pub type Place = (i32, time::OffsetDateTime);

pub async fn place_dans_file(etat: &Etat, id: i64) -> Resultat<Option<Place>> {
    let r = sqlx::query!(
        r#"select case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $2::int then 1
                       when not (jsonb_array_length(d.controles) > 0 or (d.type = 'bl' and d.fournisseur_id is null)) then 2
                       else 3 end as "rang!",
                  case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $2::int
                       then d.date_echeance::timestamp at time zone 'Europe/Paris'
                       else coalesce(m.date_mail, m.recu_le) end as "cle!"
           from document d join mail m on m.id = d.mail_id where d.id = $1"#,
        id,
        JOURS_ECHEANCE
    )
    .fetch_optional(&etat.pool)
    .await?;
    Ok(r.map(|r| (r.rang, r.cle)))
}

/// Document suivant de la file « À valider » après `id` (ordre de la file), en revenant au
/// début s'il n'y en a plus après ; jamais `id` lui-même.
pub async fn suivant(
    etat: &Etat,
    acces: &Acces,
    societe: Option<i64>,
    id: i64,
    place: Option<Place>,
) -> Resultat<Option<i64>> {
    let (rang, cle) = match place {
        Some((r, c)) => (Some(r), Some(c)),
        None => (None, None),
    };
    Ok(sqlx::query_scalar!(
        r#"select d.id from document d
           join mail m on m.id = d.mail_id
           cross join lateral (
               select case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $6::int then 1
                           when not (jsonb_array_length(d.controles) > 0 or (d.type = 'bl' and d.fournisseur_id is null)) then 2
                           else 3 end as rang,
                      case when d.type = 'facture' and d.date_echeance <= (now() at time zone 'Europe/Paris')::date + $6::int
                           then d.date_echeance::timestamp at time zone 'Europe/Paris'
                           else coalesce(m.date_mail, m.recu_le) end as cle
           ) u
           where d.id <> $3
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ((d.verifie_le is null
                   and coalesce(d.statut_depot, '') not in ('en_envoi', 'depose', 'erreur', 'doublon'))
                  or (d.verifie_le is not null and d.valide_le is null
                      and d.statut_depot in ('a_verifier', 'pret')))
             and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                         where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
             and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
             and ($2::bigint is null or d.societe_id = $2
                  or (d.societe_id is null
                      and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                  where mb.mail_id = d.mail_id and b.societe_id = $2)))
           order by ($4::int is not null and (u.rang, u.cle, d.id) > ($4::int, $5::timestamptz, $3)) desc,
                    u.rang, u.cle, d.id
           limit 1"#,
        acces.societe_filtre(),
        societe,
        id,
        rang,
        cle,
        JOURS_ECHEANCE
    )
    .fetch_optional(&etat.pool)
    .await?)
}
