//! Logiflow : essai de connexion ; après l'envoi d'une facture à Dext/Yooz, écriture de la
//! facture (référence, HT, TTC, échéance) sur la livraison de chacun de ses BL dans Logiflow,
//! noté dans `logiflow_rapprochement` (un échec n'empêche jamais le dépôt) ; et toutes les
//! heures, [`suivre`] : livraison Logiflow de chaque BL de GMFlow (`logiflow_livraison`), et
//! nouvel essai pour les factures envoyées dont un BL n'était pas encore dans Logiflow.

use std::collections::{BTreeSet, HashMap};

use gmflow_core::logiflow::{self, Choix, Client, LigneResultat, Reperes, bl_comparable};
use gmflow_core::parametre;
use serde_json::json;

use crate::Contexte;

fn euros(x: f64) -> String {
    format!("{x:.2} €").replace('.', ",")
}

/// Après une écriture, la livraison mise à jour remplace celle retenue pour ses BL.
async fn retenir_ecriture(pool: &sqlx::PgPool, l: &logiflow::Livraison) -> anyhow::Result<()> {
    sqlx::query!(
        "update logiflow_livraison
         set facture = $2, facture_ht = $3::float8::numeric(14, 2),
             facture_ttc = $4::float8::numeric(14, 2), echeance = $5::text::date,
             rapprochee = $6, vu_le = now()
         where livraison_id = $1",
        l.id,
        l.facture,
        l.facture_ht,
        l.facture_ttc,
        l.echeance,
        l.rapprochee
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn maintenant() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub async fn tester(ctx: &Contexte) -> anyhow::Result<()> {
    let resultat = match logiflow::lire(&ctx.pool, &ctx.chiffreur).await? {
        None => Err("adresse de Logiflow non configurée".to_string()),
        Some((s, jeton)) => match Client::new(&s.adresse, jeton.as_deref()) {
            Ok(c) => c.essayer().await,
            Err(e) => Err(e),
        },
    };
    let (ok, message) = match resultat {
        Ok(m) => (true, m),
        Err(m) => (false, m),
    };
    parametre::ecrire(
        &ctx.pool,
        logiflow::CLE_TEST,
        &json!({ "quand": maintenant(), "ok": ok, "message": message }),
    )
    .await?;
    Ok(())
}

async fn noter(
    ctx: &Contexte,
    document_id: i64,
    ok: bool,
    message: &str,
    lignes: &[LigneResultat],
) -> anyhow::Result<()> {
    sqlx::query!(
        "insert into logiflow_rapprochement (document_id, quand, ok, message, livraisons)
         values ($1, now(), $2, $3, $4)
         on conflict (document_id) do update
         set quand = now(), ok = excluded.ok, message = excluded.message, livraisons = excluded.livraisons",
        document_id,
        ok,
        message,
        serde_json::to_value(lignes)?
    )
    .execute(&ctx.pool)
    .await?;
    Ok(())
}

/// Repères d'un document pour reconnaître ses livraisons : codes BO Phénix de la fiche
/// fournisseur, de son groupe et des marques du groupe ; noms du fournisseur ; noms de la société.
async fn reperes(
    pool: &sqlx::PgPool,
    fournisseur_id: Option<i64>,
    noms: [Option<String>; 2],
    societe: Option<String>,
    autres_noms: Option<Vec<String>>,
) -> anyhow::Result<Reperes> {
    let codes = match fournisseur_id {
        Some(f) => {
            sqlx::query_scalar!(
                r#"select distinct x.code_gestion as "code!" from fournisseur x
               where x.code_gestion is not null and x.code_gestion <> ''
                 and (x.id = $1
                      or x.id = (select groupe_id from fournisseur where id = $1)
                      or x.groupe_id = $1
                      or x.groupe_id = (select groupe_id from fournisseur where id = $1))"#,
                f
            )
            .fetch_all(pool)
            .await?
        }
        None => vec![],
    };
    Ok(Reperes {
        codes,
        noms: noms
            .into_iter()
            .flatten()
            .filter(|n| !n.trim().is_empty())
            .collect(),
        societe: societe
            .into_iter()
            .chain(autres_noms.unwrap_or_default())
            .collect(),
    })
}

/// Facture envoyée : retrouve dans Logiflow la livraison de chacun de ses BL (n° cités par la
/// facture et BL rapprochés dans GMFlow) et y écrit la facture. Sans Logiflow configuré, ou
/// pour un autre document qu'une facture envoyée, ne fait rien.
pub async fn rapprocher(ctx: &Contexte, document_id: i64) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some((serveur, jeton)) = logiflow::lire(pool, &ctx.chiffreur).await? else {
        return Ok(());
    };
    let Some(d) = sqlx::query!(
        r#"select d.type, d.statut_depot, d.numero, d.numeros_bl,
                  d.montant_ht::float8 as ht, d.montant_ttc::float8 as ttc, d.fournisseur_nom,
                  d.date_echeance, fo.id as "fournisseur_id?", fo.nom as "fournisseur?",
                  s.raison_sociale as "societe?", s.autres_noms as "autres_noms?"
           from document d
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.id = $1"#,
        document_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    if d.r#type.as_deref() != Some("facture") || d.statut_depot.as_deref() != Some("depose") {
        return Ok(());
    }
    let Some(numero) = d.numero.clone().filter(|n| !n.trim().is_empty()) else {
        return noter(
            ctx,
            document_id,
            false,
            "facture sans numéro : rien à écrire dans Logiflow",
            &[],
        )
        .await;
    };

    // N° de BL : cités par la facture, puis ceux des BL rapprochés dans GMFlow.
    // Montant HT de chaque BL rattaché, pour répartir la facture entre ses BL.
    let lies = sqlx::query!(
        r#"select b.numero as "numero!", b.montant_ht::float8 as ht
           from rapprochement r join document b on b.id = r.bl_id
           where r.facture_id = $1 and not r.rejete and b.numero is not null"#,
        document_id
    )
    .fetch_all(pool)
    .await?;
    let montants_gmflow: HashMap<String, f64> = lies
        .iter()
        .filter_map(|b| Some((bl_comparable(&b.numero), b.ht?)))
        .collect();
    let mut vus = BTreeSet::new();
    let bls: Vec<String> = d
        .numeros_bl
        .iter()
        .chain(lies.iter().map(|b| &b.numero))
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty() && vus.insert(bl_comparable(b)))
        .collect();
    if bls.is_empty() {
        return noter(
            ctx,
            document_id,
            false,
            "aucun n° de BL sur la facture ni BL rattaché : rien à chercher dans Logiflow",
            &[],
        )
        .await;
    }

    let reperes = reperes(
        pool,
        d.fournisseur_id,
        [d.fournisseur.clone(), d.fournisseur_nom.clone()],
        d.societe.clone(),
        d.autres_noms.clone(),
    )
    .await?;

    let client = match Client::new(&serveur.adresse, jeton.as_deref()) {
        Ok(c) => c,
        Err(e) => return noter(ctx, document_id, false, &e, &[]).await,
    };
    // Repli quand le n° exact n'existe pas : livraisons sans facture des fournisseurs de mêmes
    // codes, comparées sur le n° de BL normalisé. Chargées une fois, au premier besoin.
    let mut sans_facture: Option<Vec<logiflow::Livraison>> = None;

    let mut trouvees = Vec::new();
    let mut lignes = Vec::new();
    for bl in &bls {
        let mut candidates = match client.livraisons_du_bl(bl).await {
            Ok(l) => l,
            Err(e) => return noter(ctx, document_id, false, &e, &[]).await,
        };
        if candidates.is_empty() && !reperes.codes.is_empty() {
            if sans_facture.is_none() {
                let mut toutes = Vec::new();
                let fournisseurs = match client.fournisseurs().await {
                    Ok(f) => f,
                    Err(e) => return noter(ctx, document_id, false, &e, &[]).await,
                };
                for (id, code, _) in fournisseurs {
                    if code
                        .is_some_and(|c| reperes.codes.iter().any(|x| x.eq_ignore_ascii_case(&c)))
                    {
                        match client.livraisons_sans_facture(id).await {
                            Ok(l) => toutes.extend(l),
                            Err(e) => return noter(ctx, document_id, false, &e, &[]).await,
                        }
                    }
                }
                sans_facture = Some(toutes);
            }
            let cle = bl_comparable(bl);
            candidates = sans_facture
                .iter()
                .flatten()
                .filter(|l| l.bl.as_deref().is_some_and(|b| bl_comparable(b) == cle))
                .cloned()
                .collect();
        }
        match logiflow::choisir(candidates, &reperes) {
            Choix::Une(l) => trouvees.push((bl.clone(), l)),
            Choix::Aucune => lignes.push(LigneResultat {
                bl: bl.clone(),
                livraison_id: None,
                magasin: None,
                etat: "introuvable".into(),
                message: "aucune livraison livrée avec ce n° de BL pour ce fournisseur".into(),
            }),
            Choix::Plusieurs(n) => lignes.push(LigneResultat {
                bl: bl.clone(),
                livraison_id: None,
                magasin: None,
                etat: "ambigu".into(),
                message: format!("{n} livraisons possibles : à rapprocher dans Logiflow"),
            }),
        }
    }

    // La même livraison peut venir de deux n° (« BL-12 » cité, « 12 » rattaché).
    let mut ids = BTreeSet::new();
    trouvees.retain(|(_, l)| ids.insert(l.id));
    // Montants : la part de la facture qui revient à chaque BL, seulement quand toutes ses
    // livraisons sont trouvées (sinon la répartition serait fausse) ; montant du BL lu dans
    // GMFlow, sinon celui saisi dans Logiflow.
    let parts = if lignes.is_empty() {
        let montants: Vec<Option<f64>> = trouvees
            .iter()
            .map(|(bl, l)| {
                montants_gmflow
                    .get(&bl_comparable(bl))
                    .copied()
                    .or(l.montant_bl)
            })
            .collect();
        logiflow::repartir(d.ht, d.ttc, &montants)
    } else {
        None
    };
    let echeance = d.date_echeance.map(|e| e.to_string());
    let mut completees = 0;
    let mut erreurs = 0;
    for (i, (bl, l)) in trouvees.into_iter().enumerate() {
        let ligne = |etat: &str, message: String| LigneResultat {
            bl: bl.clone(),
            livraison_id: Some(l.id),
            magasin: l.magasin.clone(),
            etat: etat.into(),
            message,
        };
        let meme = l
            .facture
            .as_deref()
            .is_some_and(|f| f.trim().eq_ignore_ascii_case(numero.trim()));
        if let Some(autre) = l.facture.as_deref().filter(|_| !meme) {
            lignes.push(ligne(
                "autre_facture",
                format!("déjà liée à la facture {autre} : rien n'est changé"),
            ));
            continue;
        }
        if l.rapprochee && !l.automatique && meme {
            completees += 1;
            lignes.push(ligne(
                "deja_fait",
                "déjà rapprochée avec cette facture dans Logiflow".into(),
            ));
            continue;
        }
        if l.rapprochee && !l.automatique {
            lignes.push(ligne(
                "deja_rapproche",
                "livraison déjà rapprochée dans Logiflow : rien n'est changé".into(),
            ));
            continue;
        }
        let part = parts.as_ref().map(|p| p[i]);
        let corps = logiflow::corps_facture(&numero, part, echeance.as_deref());
        match client.ecrire_facture(l.id, &corps).await {
            Ok(apres) => {
                completees += 1;
                retenir_ecriture(pool, &apres).await?;
                let mut m = match part {
                    Some((h, t)) => format!(
                        "référence et montants écrits (HT {}, TTC {})",
                        euros(h),
                        euros(t)
                    ),
                    None => "référence de la facture écrite (montants à saisir : BL de la facture \
                             pas tous trouvés, ou montants inconnus)"
                        .into(),
                };
                if let Some(e) = &echeance {
                    m.push_str(&format!(", échéance {e}"));
                }
                lignes.push(ligne(if meme { "deja_fait" } else { "complete" }, m));
            }
            Err(e) => {
                erreurs += 1;
                lignes.push(ligne("erreur", e));
            }
        }
    }
    lignes.sort_by(|a, b| a.bl.cmp(&b.bl));
    let total = lignes.len();
    let ok = completees > 0 && erreurs == 0 && completees == total;
    let message = if ok {
        format!(
            "{completees} livraison{} complétée{} dans Logiflow",
            if completees > 1 { "s" } else { "" },
            if completees > 1 { "s" } else { "" }
        )
    } else {
        format!(
            "{completees} BL sur {total} complété{} dans Logiflow",
            if completees > 1 { "s" } else { "" }
        )
    };
    noter(ctx, document_id, ok, &message, &lignes).await
}

/// Vérification régulière (toutes les heures, voir `planif`) :
/// 1. les livraisons livrées de Logiflow des derniers jours, lues en une fois ;
/// 2. chaque BL de GMFlow reçu depuis `SUIVI_JOURS` retrouve la sienne par n° de BL (même
///    fournisseur, puis même magasin), notée dans `logiflow_livraison` (page BL) ;
/// 3. les factures envoyées à Dext/Yooz dont un BL n'était pas encore dans Logiflow (ou en
///    erreur) sont rapprochées à nouveau : référence, montants et échéance écrits.
pub async fn suivre(ctx: &Contexte) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some((serveur, jeton)) = logiflow::lire(pool, &ctx.chiffreur).await? else {
        return Ok(());
    };
    // Noté dès le départ : la planification ne relance pas un passage encore en cours.
    parametre::ecrire(
        pool,
        logiflow::CLE_SUIVI,
        &json!({ "quand": maintenant(), "ok": null, "message": "vérification en cours" }),
    )
    .await?;
    let resultat = suivre_avec(ctx, &serveur.adresse, jeton.as_deref()).await;
    let (ok, message) = match &resultat {
        Ok(m) => (true, m.clone()),
        Err(e) => (false, e.to_string()),
    };
    parametre::ecrire(
        pool,
        logiflow::CLE_SUIVI,
        &json!({ "quand": maintenant(), "ok": ok, "message": message }),
    )
    .await?;
    resultat.map(|_| ())
}

async fn suivre_avec(ctx: &Contexte, adresse: &str, jeton: Option<&str>) -> anyhow::Result<String> {
    let pool = &ctx.pool;
    let client = Client::new(adresse, jeton).map_err(anyhow::Error::msg)?;
    // Une livraison précède souvent le mail de son BL : 30 jours de marge.
    let depuis = (time::OffsetDateTime::now_utc().date()
        - time::Duration::days(i64::from(logiflow::SUIVI_JOURS) + 30))
    .to_string();
    let livraisons = client
        .livraisons_livrees(&depuis)
        .await
        .map_err(anyhow::Error::msg)?;
    let mut par_bl: HashMap<String, Vec<logiflow::Livraison>> = HashMap::new();
    for l in livraisons {
        if let Some(b) = l.bl.as_deref().map(bl_comparable).filter(|b| !b.is_empty()) {
            par_bl.entry(b).or_default().push(l);
        }
    }

    let bls = sqlx::query!(
        r#"select d.id, d.numero as "numero!", d.fournisseur_nom,
                  fo.id as "fournisseur_id?", fo.nom as "fournisseur?",
                  s.raison_sociale as "societe?", s.autres_noms as "autres_noms?"
           from document d
           left join fournisseur fo on fo.id = d.fournisseur_id
           left join societe s on s.id = d.societe_id
           where d.type = 'bl' and d.etat = 'lu' and d.numero is not null and d.numero <> ''
             and d.lu_le > now() - make_interval(days => $1)"#,
        logiflow::SUIVI_JOURS
    )
    .fetch_all(pool)
    .await?;
    let (mut trouves, mut ambigus) = (0, 0);
    for b in &bls {
        let candidates = par_bl
            .get(&bl_comparable(&b.numero))
            .cloned()
            .unwrap_or_default();
        let choix = if candidates.is_empty() {
            Choix::Aucune
        } else {
            let r = reperes(
                pool,
                b.fournisseur_id,
                [b.fournisseur.clone(), b.fournisseur_nom.clone()],
                b.societe.clone(),
                b.autres_noms.clone(),
            )
            .await?;
            logiflow::choisir(candidates, &r)
        };
        match choix {
            Choix::Une(l) => {
                trouves += 1;
                retenir(pool, b.id, "trouve", 1, Some(&l)).await?;
            }
            Choix::Plusieurs(n) => {
                ambigus += 1;
                retenir(pool, b.id, "ambigu", n as i32, None).await?;
            }
            Choix::Aucune => {
                sqlx::query!("delete from logiflow_livraison where bl_id = $1", b.id)
                    .execute(pool)
                    .await?;
            }
        }
    }

    // Factures envoyées sans rapprochement Logiflow, ou dont un BL manquait / a échoué.
    let factures = sqlx::query_scalar!(
        r#"select d.id from document d
           left join logiflow_rapprochement lr on lr.document_id = d.id
           where d.type = 'facture' and d.statut_depot = 'depose'
             and coalesce(d.valide_le, d.lu_le) > now() - make_interval(days => $1)
             and (lr.document_id is null
                  or (not lr.ok
                      and (lr.livraisons = '[]'::jsonb
                           or exists (select 1 from jsonb_array_elements(lr.livraisons) e
                                      where e->>'etat' in ('introuvable', 'ambigu', 'erreur')))))
           order by d.id"#,
        logiflow::SUIVI_JOURS
    )
    .fetch_all(pool)
    .await?;
    for id in &factures {
        if let Err(e) = rapprocher(ctx, *id).await {
            tracing::warn!(document = id, erreur = ?e, "rapprochement Logiflow impossible");
        }
    }
    Ok(format!(
        "{trouves} BL sur {} trouvés dans Logiflow{}, {} facture{} vérifiée{}",
        bls.len(),
        if ambigus > 0 {
            format!(" ({ambigus} à plusieurs livraisons possibles)")
        } else {
            String::new()
        },
        factures.len(),
        if factures.len() > 1 { "s" } else { "" },
        if factures.len() > 1 { "s" } else { "" },
    ))
}

async fn retenir(
    pool: &sqlx::PgPool,
    bl_id: i64,
    etat: &str,
    possibles: i32,
    l: Option<&logiflow::Livraison>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "insert into logiflow_livraison (bl_id, vu_le, etat, possibles, livraison_id, magasin,
                                         fournisseur, bl, livree_le, montant_bl, facture,
                                         facture_ht, facture_ttc, echeance, rapprochee)
         values ($1, now(), $2, $3, $4, $5, $6, $7, $8::text::date, $9::float8::numeric(14, 2),
                 $10, $11::float8::numeric(14, 2), $12::float8::numeric(14, 2), $13::text::date,
                 $14)
         on conflict (bl_id) do update
         set vu_le = now(), etat = excluded.etat, possibles = excluded.possibles,
             livraison_id = excluded.livraison_id, magasin = excluded.magasin,
             fournisseur = excluded.fournisseur, bl = excluded.bl, livree_le = excluded.livree_le,
             montant_bl = excluded.montant_bl, facture = excluded.facture,
             facture_ht = excluded.facture_ht, facture_ttc = excluded.facture_ttc,
             echeance = excluded.echeance, rapprochee = excluded.rapprochee",
        bl_id,
        etat,
        possibles,
        l.map(|l| l.id),
        l.and_then(|l| l.magasin.clone()),
        l.and_then(|l| l.fournisseur.clone()),
        l.and_then(|l| l.bl.clone()),
        l.and_then(|l| l.livree_le.clone()),
        l.and_then(|l| l.montant_bl),
        l.and_then(|l| l.facture.clone()),
        l.and_then(|l| l.facture_ht),
        l.and_then(|l| l.facture_ttc),
        l.and_then(|l| l.echeance.clone()),
        l.is_some_and(|l| l.rapprochee)
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Vérification due : Logiflow configuré et dernier passage plus vieux que `SUIVI_MINUTES`.
pub async fn suivi_du(pool: &sqlx::PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"select exists (select 1 from parametre where cle = $1)
              and not exists (
                  select 1 from parametre
                  where cle = $2
                    and (valeur->>'quand')::timestamptz > now() - make_interval(mins => $3)
              ) as "d!""#,
        logiflow::CLE,
        logiflow::CLE_SUIVI,
        logiflow::SUIVI_MINUTES - 2
    )
    .fetch_one(pool)
    .await
}
