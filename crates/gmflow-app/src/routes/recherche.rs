//! Recherche unique (barre du haut) sur les documents et les mails. La saisie est découpée :
//! un montant (« 1 250,40 », « 1250.4 », « 60 € »), une date (« 18/11/2026 », « 18/11 »,
//! « 2026-11-18 ») et des mots (fournisseur, numéro, objet, expéditeur, nom du fichier). Un
//! nombre entier seul (« 1250 ») vaut un mot ou un montant.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use super::mails::date;
use crate::acces::Acces;
use crate::erreur::{ErreurApi, Resultat};
use crate::etat::Etat;

time::serde::format_description!(date_iso, Date, "[year]-[month]-[day]");

/// Résultats rendus au plus par groupe (les totaux restent exacts).
pub const LIMITE: i64 = 50;

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct DateCherchee {
    pub jour: i32,
    pub mois: i32,
    /// Absente pour « 18/11 » : le 18 novembre de n'importe quelle année.
    pub annee: Option<i32>,
}

/// Ce que la saisie a été comprise comme.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Interpretation {
    /// Mots cherchés dans les textes (tous doivent se retrouver).
    pub mots: Vec<String>,
    /// Montant (TTC ou HT d'un document), en euros.
    pub montant: Option<f64>,
    /// Nombres entiers seuls : un mot ou un montant.
    pub nombres: Vec<i64>,
    pub date: Option<DateCherchee>,
}

fn chiffres(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// « 1250,40 », « 1250.4 », « 1.250,40 », « 1,250.40 » : montant à décimales.
fn montant_decimal(t: &str) -> Option<f64> {
    let (entier, dec) = match (t.rfind(','), t.rfind('.')) {
        (Some(v), Some(p)) => {
            let (sep_dec, sep_mil) = if v > p { (',', '.') } else { ('.', ',') };
            let (e, d) = t.rsplit_once(sep_dec)?;
            let groupes: Vec<&str> = e.split(sep_mil).collect();
            let ok = groupes[0].len() <= 3
                && chiffres(groupes[0])
                && groupes[1..].iter().all(|g| g.len() == 3 && chiffres(g));
            if !ok {
                return None;
            }
            (groupes.concat(), d.to_string())
        }
        (Some(_), None) => {
            let (e, d) = t.rsplit_once(',')?;
            (e.to_string(), d.to_string())
        }
        (None, Some(_)) => {
            let (e, d) = t.rsplit_once('.')?;
            (e.to_string(), d.to_string())
        }
        (None, None) => return None,
    };
    if !chiffres(&entier) || !chiffres(&dec) || dec.len() > 2 {
        return None;
    }
    format!("{entier}.{dec}").parse().ok()
}

fn date_saisie(t: &str) -> Option<DateCherchee> {
    let valide = |j: i32, m: i32| (1..=31).contains(&j) && (1..=12).contains(&m);
    if let [a, m, j] = t.split('-').collect::<Vec<_>>()[..]
        && a.len() == 4
        && chiffres(a)
        && chiffres(m)
        && chiffres(j)
    {
        let (a, m, j) = (a.parse().ok()?, m.parse().ok()?, j.parse().ok()?);
        return valide(j, m).then_some(DateCherchee {
            jour: j,
            mois: m,
            annee: Some(a),
        });
    }
    let parties: Vec<&str> = t.split(['/', '.']).collect();
    if !(2..=3).contains(&parties.len())
        || !parties.iter().all(|p| chiffres(p) && p.len() <= 4)
        || parties[0].len() > 2
        || parties[1].len() > 2
    {
        return None;
    }
    let (j, m): (i32, i32) = (parties[0].parse().ok()?, parties[1].parse().ok()?);
    let annee = match parties.get(2) {
        None => None,
        Some(a) if a.len() == 4 => Some(a.parse().ok()?),
        Some(a) if a.len() == 2 => Some(2000 + a.parse::<i32>().ok()?),
        Some(_) => return None,
    };
    valide(j, m).then_some(DateCherchee {
        jour: j,
        mois: m,
        annee,
    })
}

/// Découpe la saisie en montant, date, nombres et mots.
pub fn interpreter(q: &str) -> Interpretation {
    let propre: String = q
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    // « 60€ », « 60 € », « 60 EUR » : le symbole marque un montant.
    let propre = propre.replace('€', " € ");
    let jetons: Vec<&str> = propre.split(' ').filter(|t| !t.is_empty()).collect();
    let mut r = Interpretation::default();
    let mut i = 0;
    while i < jetons.len() {
        let t = jetons[i];
        let suivi_euro = jetons
            .get(i + 1)
            .is_some_and(|s| *s == "€" || s.eq_ignore_ascii_case("eur"));
        if t == "€" || t.eq_ignore_ascii_case("eur") {
            i += 1;
            continue;
        }
        if r.date.is_none()
            && let Some(d) = date_saisie(t)
        {
            r.date = Some(d);
            i += 1;
            continue;
        }
        // Milliers séparés par des espaces : « 1 250,40 », « 12 000 ».
        if r.montant.is_none() && t.len() <= 3 && chiffres(t) {
            let mut j = i + 1;
            let mut entier = t.to_string();
            let mut dec: Option<f64> = None;
            while let Some(g) = jetons.get(j) {
                if g.len() == 3 && chiffres(g) {
                    entier.push_str(g);
                    j += 1;
                } else if let Some(v) = montant_decimal(g)
                    && g.len() > 3
                    && chiffres(&g[..3])
                    && matches!(g.as_bytes()[3], b',' | b'.')
                {
                    dec = Some(v - v.trunc());
                    entier.push_str(&g[..3]);
                    j += 1;
                    break;
                } else {
                    break;
                }
            }
            if j > i + 1 {
                let v = entier.parse::<f64>().unwrap_or_default() + dec.unwrap_or_default();
                r.montant = Some((v * 100.0).round() / 100.0);
                i = j;
                continue;
            }
        }
        if r.montant.is_none()
            && let Some(v) = montant_decimal(t)
        {
            r.montant = Some((v * 100.0).round() / 100.0);
            i += 1;
            continue;
        }
        if chiffres(t) && t.len() <= 12 {
            if suivi_euro && r.montant.is_none() {
                r.montant = t.parse().ok();
            } else if let Ok(n) = t.parse() {
                r.nombres.push(n);
            }
            i += 1;
            continue;
        }
        r.mots.push(t.to_string());
        i += 1;
    }
    r
}

/// Motifs ILIKE des mots (caractères spéciaux échappés).
fn motifs(mots: &[String]) -> Vec<String> {
    mots.iter()
        .filter_map(|m| super::mails::motif(&Some(m.clone())))
        .collect()
}

#[derive(Deserialize)]
pub struct Filtres {
    q: Option<String>,
    /// `facture`, `avoir`, `bl` (documents de ce type seulement), `document` (tous les
    /// documents, pas de mails) ou `mail` (mails seulement).
    r#type: Option<String>,
    du: Option<String>,
    au: Option<String>,
    societe: Option<i64>,
}

#[derive(Serialize)]
pub struct DocTrouve {
    id: i64,
    mail_id: i64,
    r#type: Option<String>,
    etat: String,
    statut_depot: Option<String>,
    fournisseur_id: Option<i64>,
    fournisseur: Option<String>,
    numero: Option<String>,
    #[serde(with = "date_iso::option")]
    date_document: Option<time::Date>,
    montant_ht: Option<f64>,
    montant_ttc: Option<f64>,
    societe_id: Option<i64>,
    societe: Option<String>,
    /// Outil de la société : `dext` ou `yooz` (nul si pas encore rangé ou pas réglé).
    outil: Option<String>,
    nom_fichier: String,
    #[serde(with = "time::serde::rfc3339")]
    recu_le: time::OffsetDateTime,
    verifie: bool,
}

#[derive(Serialize)]
pub struct MailTrouve {
    id: i64,
    expediteur: String,
    objet: String,
    #[serde(with = "time::serde::rfc3339::option")]
    date_mail: Option<time::OffsetDateTime>,
    categorie: Option<String>,
    statut_reponse: Option<String>,
    resume: Option<String>,
    nb_pieces: i64,
}

#[derive(Serialize)]
pub struct Groupe<T> {
    total: i64,
    elements: Vec<T>,
}

#[derive(Serialize)]
pub struct Resultats {
    interpretation: Interpretation,
    documents: Groupe<DocTrouve>,
    mails: Groupe<MailTrouve>,
}

/// `GET /api/recherche` : documents (sauf pièces écartées) et mails (publicités comprises)
/// visibles, les plus récents d'abord. Requête vide : les derniers reçus.
pub async fn chercher(
    acces: Acces,
    State(etat): State<Etat>,
    Query(f): Query<Filtres>,
) -> Resultat<Json<Resultats>> {
    let (du, au) = (date(&f.du)?, date(&f.au)?);
    let type_choisi = f.r#type.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if let Some(t) = type_choisi
        && !["facture", "avoir", "bl", "document", "mail"].contains(&t)
    {
        return Err(ErreurApi::Donnees(format!("type inconnu : {t}")));
    }
    let i = interpreter(f.q.as_deref().unwrap_or_default());
    let mots = motifs(&i.mots);
    let nombres_motifs: Vec<String> = i.nombres.iter().map(|n| format!("%{n}%")).collect();
    let nombres: Vec<f64> = i.nombres.iter().map(|n| *n as f64).collect();
    let (jour, mois, annee) = match &i.date {
        Some(d) => (Some(d.jour), Some(d.mois), d.annee),
        None => (None, None, None),
    };
    let perimetre = acces.societe_filtre();
    // `document` : tous les documents, pas de mails ; un type précis : ce type seulement.
    let documents_seuls = type_choisi.is_some_and(|t| t != "mail");
    let type_doc = type_choisi.filter(|t| *t != "mail" && *t != "document");

    let documents = if type_choisi == Some("mail") {
        Groupe {
            total: 0,
            elements: Vec::new(),
        }
    } else {
        let lignes = sqlx::query!(
            r#"select d.id, d.mail_id, d.type, d.etat, d.statut_depot, d.fournisseur_id,
                      coalesce(fo.nom, d.fournisseur_nom) as fournisseur, d.numero, d.date_document,
                      d.montant_ht::float8 as montant_ht, d.montant_ttc::float8 as montant_ttc,
                      d.societe_id, s.raison_sociale as "societe?", s.outil_depot as "outil?",
                      p.nom as nom_fichier,
                      coalesce(m.date_mail, m.recu_le) as "recu_le!",
                      d.verifie_le is not null as "verifie!",
                      count(*) over () as "total!"
               from document d
               join mail m on m.id = d.mail_id
               join piece_jointe p on p.id = d.piece_jointe_id
               left join fournisseur fo on fo.id = d.fournisseur_id
               left join societe s on s.id = d.societe_id
               where d.etat <> 'ignore'
                 and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = d.mail_id and ($1::bigint is null or b.societe_id = $1))
                 and ($1::bigint is null or d.societe_id is null or d.societe_id = $1)
                 and ($2::bigint is null or d.societe_id = $2
                      or (d.societe_id is null
                          and exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                                      where mb.mail_id = d.mail_id and b.societe_id = $2)))
                 and ($3::text is null or d.type = $3)
                 and ($4::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) >= $4)
                 and ($5::date is null or coalesce(d.date_document, (m.date_mail at time zone 'Europe/Paris')::date) <= $5)
                 and not exists (
                       select 1 from unnest($6::text[]) w
                       where not coalesce(d.fournisseur_nom ilike w or fo.nom ilike w or d.numero ilike w
                                  or d.numero_commande ilike w or m.objet ilike w
                                  or m.expediteur ilike w or p.nom ilike w, false))
                 and not exists (
                       select 1 from unnest($7::text[], $8::float8[]) as n(motif, valeur)
                       where not coalesce(d.numero ilike n.motif or d.numero_commande ilike n.motif
                                  or m.objet ilike n.motif or p.nom ilike n.motif
                                  or d.montant_ttc = n.valeur::numeric(14, 2)
                                  or d.montant_ht = n.valeur::numeric(14, 2), false))
                 and ($9::float8 is null or d.montant_ttc = $9::numeric(14, 2)
                      or d.montant_ht = $9::numeric(14, 2) or d.montant_tva = $9::numeric(14, 2))
                 and ($10::int is null
                      or (extract(day from d.date_document)::int = $10
                          and extract(month from d.date_document)::int = $11
                          and ($12::int is null or extract(year from d.date_document)::int = $12))
                      or (extract(day from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $10
                          and extract(month from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $11
                          and ($12::int is null
                               or extract(year from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $12)))
               order by coalesce(m.date_mail, m.recu_le) desc, d.id desc
               limit $13"#,
            perimetre,
            f.societe,
            type_doc,
            du,
            au,
            &mots,
            &nombres_motifs,
            &nombres,
            i.montant,
            jour,
            mois,
            annee,
            LIMITE
        )
        .fetch_all(&etat.pool)
        .await?;
        Groupe {
            total: lignes.first().map(|l| l.total).unwrap_or(0),
            elements: lignes
                .into_iter()
                .map(|l| DocTrouve {
                    id: l.id,
                    mail_id: l.mail_id,
                    r#type: l.r#type,
                    etat: l.etat,
                    statut_depot: l.statut_depot,
                    fournisseur_id: l.fournisseur_id,
                    fournisseur: l.fournisseur,
                    numero: l.numero,
                    date_document: l.date_document,
                    montant_ht: l.montant_ht,
                    montant_ttc: l.montant_ttc,
                    societe_id: l.societe_id,
                    societe: l.societe,
                    outil: l.outil,
                    nom_fichier: l.nom_fichier,
                    recu_le: l.recu_le,
                    verifie: l.verifie,
                })
                .collect(),
        }
    };

    let mails = if documents_seuls {
        Groupe {
            total: 0,
            elements: Vec::new(),
        }
    } else {
        let lignes = sqlx::query!(
            r#"select m.id, m.expediteur, m.objet, m.date_mail, m.categorie, m.statut_reponse,
                      m.resume,
                      (select count(*) from piece_jointe p where p.mail_id = m.id and p.source_id is null) as "nb_pieces!",
                      count(*) over () as "total!"
               from mail m
               where exists (select 1 from mail_boite_vue mb join boite_mail b on b.id = mb.boite_id
                             where mb.mail_id = m.id
                               and ($1::bigint is null or b.societe_id = $1)
                               and ($2::bigint is null or b.societe_id = $2))
                 and ($3::date is null or coalesce(m.date_mail, m.recu_le) >= ($3::date::timestamp at time zone 'Europe/Paris'))
                 and ($4::date is null or coalesce(m.date_mail, m.recu_le) < (($4::date + 1)::timestamp at time zone 'Europe/Paris'))
                 and not exists (
                       select 1 from unnest($5::text[]) w
                       where not coalesce(m.expediteur ilike w or m.objet ilike w
                                  or exists (select 1 from piece_jointe p where p.mail_id = m.id and p.nom ilike w)
                                  or exists (select 1 from document d left join fournisseur fo on fo.id = d.fournisseur_id
                                             where d.mail_id = m.id
                                               and (d.fournisseur_nom ilike w or fo.nom ilike w or d.numero ilike w)), false))
                 and not exists (
                       select 1 from unnest($6::text[], $7::float8[]) as n(motif, valeur)
                       where not coalesce(m.objet ilike n.motif
                                  or exists (select 1 from document d where d.mail_id = m.id
                                             and (d.numero ilike n.motif
                                                  or d.montant_ttc = n.valeur::numeric(14, 2)
                                                  or d.montant_ht = n.valeur::numeric(14, 2))), false))
                 and ($8::float8 is null
                      or exists (select 1 from document d where d.mail_id = m.id
                                 and (d.montant_ttc = $8::numeric(14, 2) or d.montant_ht = $8::numeric(14, 2)
                                      or d.montant_tva = $8::numeric(14, 2))))
                 and ($9::int is null
                      or (extract(day from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $9
                          and extract(month from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $10
                          and ($11::int is null
                               or extract(year from (coalesce(m.date_mail, m.recu_le) at time zone 'Europe/Paris'))::int = $11)))
               order by coalesce(m.date_mail, m.recu_le) desc, m.id desc
               limit $12"#,
            perimetre,
            f.societe,
            du,
            au,
            &mots,
            &nombres_motifs,
            &nombres,
            i.montant,
            jour,
            mois,
            annee,
            LIMITE
        )
        .fetch_all(&etat.pool)
        .await?;
        Groupe {
            total: lignes.first().map(|l| l.total).unwrap_or(0),
            elements: lignes
                .into_iter()
                .map(|l| MailTrouve {
                    id: l.id,
                    expediteur: l.expediteur,
                    objet: l.objet,
                    date_mail: l.date_mail,
                    categorie: l.categorie,
                    statut_reponse: l.statut_reponse,
                    resume: l.resume,
                    nb_pieces: l.nb_pieces,
                })
                .collect(),
        }
    };
    Ok(Json(Resultats {
        interpretation: i,
        documents,
        mails,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn montants() {
        assert_eq!(interpreter("1 250,40").montant, Some(1250.40));
        assert_eq!(interpreter("1250.4").montant, Some(1250.4));
        assert_eq!(interpreter("1.250,40").montant, Some(1250.40));
        assert_eq!(interpreter("1,250.40").montant, Some(1250.40));
        assert_eq!(interpreter("60 €").montant, Some(60.0));
        assert_eq!(interpreter("60€").montant, Some(60.0));
        assert_eq!(interpreter("12 000").montant, Some(12000.0));
        assert_eq!(interpreter("1\u{a0}250,40 €").montant, Some(1250.40));
        let i = interpreter("durand 1250");
        assert_eq!(i.montant, None);
        assert_eq!(i.nombres, vec![1250]);
        assert_eq!(i.mots, vec!["durand"]);
    }

    #[test]
    fn dates() {
        assert_eq!(
            interpreter("18/11/2026").date,
            Some(DateCherchee {
                jour: 18,
                mois: 11,
                annee: Some(2026)
            })
        );
        assert_eq!(
            interpreter("18/11").date,
            Some(DateCherchee {
                jour: 18,
                mois: 11,
                annee: None
            })
        );
        assert_eq!(interpreter("18/11/26").date.unwrap().annee, Some(2026));
        assert_eq!(interpreter("2026-11-18").date.unwrap().jour, 18);
        assert_eq!(interpreter("18.11.2026").date.unwrap().mois, 11);
        // Pas une date : reste un mot.
        let i = interpreter("F-2026/45");
        assert_eq!(i.date, None);
        assert_eq!(i.mots, vec!["F-2026/45"]);
        assert_eq!(interpreter("45/13").date, None);
    }

    #[test]
    fn melange() {
        let i = interpreter("Metro facture 18/11 1 250,40 €");
        assert_eq!(i.mots, vec!["Metro", "facture"]);
        assert_eq!(i.montant, Some(1250.40));
        assert_eq!(i.date.unwrap().jour, 18);
        assert!(i.nombres.is_empty());
    }
}
