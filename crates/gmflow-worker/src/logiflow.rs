//! Logiflow : essai de connexion, et après l'envoi d'une facture à Dext/Yooz, écriture de la
//! facture (référence, HT, TTC) sur la livraison de chacun de ses BL dans Logiflow. Le résultat
//! est noté dans `logiflow_rapprochement` ; un échec n'empêche jamais le dépôt.

use std::collections::BTreeSet;

use gmflow_core::logiflow::{self, Choix, Client, LigneResultat, Reperes, bl_comparable};
use gmflow_core::parametre;
use serde_json::json;

use crate::Contexte;

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
                  fo.id as "fournisseur_id?", fo.nom as "fournisseur?",
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
    let lies = sqlx::query_scalar!(
        r#"select b.numero as "numero!" from rapprochement r join document b on b.id = r.bl_id
           where r.facture_id = $1 and not r.rejete and b.numero is not null"#,
        document_id
    )
    .fetch_all(pool)
    .await?;
    let mut vus = BTreeSet::new();
    let bls: Vec<String> = d
        .numeros_bl
        .iter()
        .chain(lies.iter())
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

    // Codes BO Phénix : la fiche, son groupe et les marques du groupe.
    let codes = match d.fournisseur_id {
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
    let reperes = Reperes {
        codes,
        noms: [d.fournisseur.clone(), d.fournisseur_nom.clone()]
            .into_iter()
            .flatten()
            .filter(|n| !n.trim().is_empty())
            .collect(),
        societe: d
            .societe
            .iter()
            .cloned()
            .chain(d.autres_noms.clone().unwrap_or_default())
            .collect(),
    };

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
    let une_seule = trouvees.len() == 1 && lignes.is_empty();
    let corps = logiflow::corps_facture(&numero, d.ht, d.ttc, une_seule);
    let mut completees = 0;
    let mut erreurs = 0;
    for (bl, l) in trouvees {
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
        match client.ecrire_facture(l.id, &corps).await {
            Ok(_) => {
                completees += 1;
                lignes.push(ligne(
                    if meme { "deja_fait" } else { "complete" },
                    if une_seule {
                        "référence et montants de la facture écrits".into()
                    } else {
                        "référence de la facture écrite (montants à saisir : la facture couvre plusieurs BL)".into()
                    },
                ));
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
