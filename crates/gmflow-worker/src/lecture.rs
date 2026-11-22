//! Lecture des pièces jointes : pompe (création des documents, mise en file) et tâche
//! `LireDocument` (aiguillage, lecture par le code, puis un seul appel à Claude si besoin).

use gmflow_core::claude::{self, Appel, DossierTemp, ErreurClaude};
use gmflow_core::lecture::extraction::{self, Contenu, ContexteMail, ErreurExtraction, Extraction};
use gmflow_core::lecture::societe::{self, SocieteRef};
use gmflow_core::lecture::{self as lect, Aiguillage, Voie};
use gmflow_core::{eml, file, journal, parametre, reglages, stockage};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::Contexte;

/// Documents mis en file par tour de pompe.
const PAR_TOUR: i64 = 200;
/// Texte conservé en base.
const TEXTE_CONSERVE: usize = 200_000;
const VOIES_CLAUDE: &[&str] = &["pdf_texte", "tableur", "scan"];

/// Claude peut-il lire maintenant (jeton présent, quota du jour non atteint) ? Sinon, pourquoi.
pub(crate) async fn claude_indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
) -> anyhow::Result<Option<String>> {
    if !claude::acces_configure(pool).await? {
        return Ok(Some("jeton Claude absent".into()));
    }
    if reglages::appels_lecture_du_jour(pool).await? >= r.quota_lecture_jour {
        return Ok(Some(format!(
            "quota du jour atteint ({} appels)",
            r.quota_lecture_jour
        )));
    }
    Ok(None)
}

/// Crée les documents des pièces jointes nouvelles, remet en file les documents bloqués, puis
/// pousse les documents à lire, les plus récents d'abord. Rend le nombre de tâches poussées.
pub async fn pompe(pool: &PgPool) -> anyhow::Result<usize> {
    sqlx::query!(
        "insert into document (piece_jointe_id, mail_id, sha256)
         select p.id, p.mail_id, p.sha256 from piece_jointe p
         where not exists (select 1 from document d where d.piece_jointe_id = p.id)
         on conflict (piece_jointe_id) do nothing"
    )
    .execute(pool)
    .await?;
    sqlx::query!(
        "update document set etat = 'a_lire', pris_le = null, planifie_le = null
         where etat = 'en_cours' and pris_le < now() - interval '30 minutes'"
    )
    .execute(pool)
    .await?;
    let r = reglages::lire(pool).await?;
    if !r.lecture_auto {
        return Ok(0);
    }
    // Sans Claude disponible, seuls les documents pas encore aiguillés ou lisibles par le code
    // partent : les autres attendraient pour rien.
    let sans_claude = claude_indisponible(pool, &r).await?.is_some();
    let voies: Vec<String> = VOIES_CLAUDE.iter().map(|v| v.to_string()).collect();
    let ids = sqlx::query_scalar!(
        r#"with choisis as (
             select d.id, m.date_mail from document d join mail m on m.id = d.mail_id
             where d.etat = 'a_lire'
               and (d.planifie_le is null or d.planifie_le < now() - interval '30 minutes')
               and not ($1 and d.voie is not null and d.voie = any($2))
             order by m.date_mail desc nulls last, d.id desc
             limit $3
             for update of d skip locked
           ), maj as (
             update document set planifie_le = now() where id in (select id from choisis)
           )
           select id as "id!" from choisis order by date_mail desc nulls last, id desc"#,
        sans_claude,
        &voies,
        PAR_TOUR
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        file::pousser_lecture(pool, *id, false).await?;
    }
    Ok(ids.len())
}

/// La pompe après une relève : une erreur n'arrête pas la relève.
pub async fn pompe_apres_releve(pool: &PgPool) {
    if let Err(e) = pompe(pool).await {
        tracing::warn!(erreur = ?e, "mise en file des lectures impossible");
    }
}

pub async fn societes(pool: &PgPool) -> sqlx::Result<Vec<SocieteRef>> {
    let lignes = sqlx::query!(
        "select id, raison_sociale, siret, tva, adresses from societe where not archivee order by id"
    )
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|l| SocieteRef {
            id: l.id,
            raison_sociale: l.raison_sociale,
            siret: l.siret,
            tva: l.tva,
            adresses: l
                .adresses
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// Résultat d'une lecture, prêt à enregistrer.
#[derive(Debug, Default)]
struct Lu {
    etat: &'static str,
    voie: Option<&'static str>,
    message: Option<String>,
    type_doc: Option<String>,
    societe_id: Option<i64>,
    fournisseur: extraction::Fournisseur,
    numero: Option<String>,
    date: Option<time::Date>,
    commande: Option<String>,
    ht: Option<f64>,
    tva: Option<f64>,
    ttc: Option<f64>,
    confiance: Value,
    /// Lignes articles et n° de BL cités ; `lignes_lues` : les lignes ont été demandées.
    lignes: Vec<extraction::Ligne>,
    numeros_bl: Vec<String>,
    lignes_lues: bool,
    texte: Option<String>,
    resultat: Option<Value>,
    modele: Option<String>,
    effort: Option<String>,
}

fn tronquer(s: String, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => s[..i].to_string(),
        None => s,
    }
    .replace('\0', "")
}

async fn enregistrer(pool: &PgPool, id: i64, lu: Lu) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    gmflow_core::rapprochement::ecrire_lignes(&mut tx, id, &lu.lignes).await?;
    sqlx::query!(
        "update document set numeros_bl = $2, lignes_lues = $3 where id = $1",
        id,
        &lu.numeros_bl,
        lu.lignes_lues
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update document set etat = $2, voie = $3, message = $4, type = $5, societe_id = $6,
            fournisseur_nom = $7, fournisseur_siret = $8, fournisseur_tva = $9, numero = $10,
            date_document = $11, numero_commande = $12,
            montant_ht = $13::float8::numeric(14, 2), montant_tva = $14::float8::numeric(14, 2),
            montant_ttc = $15::float8::numeric(14, 2), confiance = $16, texte = $17,
            resultat = $18, modele = $19, effort = $20, copie_de = null, pris_le = null,
            lu_le = case when $2 in ('lu', 'ignore', 'erreur') then now() else lu_le end
         where id = $1",
        id,
        lu.etat,
        lu.voie,
        lu.message,
        lu.type_doc,
        lu.societe_id,
        lu.fournisseur.nom,
        lu.fournisseur.siret,
        lu.fournisseur.tva,
        lu.numero,
        lu.date,
        lu.commande,
        lu.ht,
        lu.tva,
        lu.ttc,
        lu.confiance,
        lu.texte.map(|t| tronquer(t, TEXTE_CONSERVE)),
        lu.resultat,
        lu.modele,
        lu.effort
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Document remis en attente (Claude indisponible) : la voie est gardée pour que la pompe
/// ne le renvoie pas avant que Claude soit de nouveau disponible.
async fn remettre(pool: &PgPool, id: i64, voie: Voie, message: &str) -> anyhow::Result<()> {
    sqlx::query!(
        "update document set etat = 'a_lire', voie = $2, message = $3, pris_le = null where id = $1",
        id,
        voie.code(),
        message
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Reprend le résultat d'un document au contenu identique déjà lu. Rend vrai si c'est fait.
async fn reprendre_identique(pool: &PgPool, id: i64, sha: &[u8]) -> anyhow::Result<bool> {
    let Some(source) = sqlx::query_scalar!(
        "select id from document where sha256 = $1 and id <> $2 and etat in ('lu', 'ignore')
           and copie_de is null order by lu_le limit 1",
        sha,
        id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "update document d set (etat, voie, message, type, societe_id, fournisseur_nom,
            fournisseur_siret, fournisseur_tva, numero, date_document, numero_commande, montant_ht,
            montant_tva, montant_ttc, confiance, texte, resultat, modele, effort, numeros_bl,
            lignes_lues) =
          (select s.etat, s.voie, s.message, s.type, s.societe_id, s.fournisseur_nom,
            s.fournisseur_siret, s.fournisseur_tva, s.numero, s.date_document, s.numero_commande,
            s.montant_ht, s.montant_tva, s.montant_ttc, s.confiance, s.texte, s.resultat, s.modele,
            s.effort, s.numeros_bl, s.lignes_lues from document s where s.id = $2),
          copie_de = $2, pris_le = null, lu_le = now()
         where d.id = $1",
        id,
        source
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("delete from ligne_document where document_id = $1", id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "insert into ligne_document (document_id, rang, reference, designation, quantite, unite,
                                     prix_unitaire_ht, montant_ht)
         select $1, rang, reference, designation, quantite, unite, prix_unitaire_ht, montant_ht
         from ligne_document where document_id = $2",
        id,
        source
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

fn confiance_xml(societe_trouvee: bool) -> Value {
    json!({
        "type": 1.0, "societe": if societe_trouvee { 1.0 } else { 0.0 }, "fournisseur": 1.0,
        "numero": 1.0, "date": 1.0, "montants": 1.0
    })
}

fn lu_depuis_xml(
    voie: Voie,
    d: lect::xml::DonneesXml,
    texte: Option<String>,
    soc: &[SocieteRef],
) -> Lu {
    let societe_id = societe::pour_acheteur(&d.acheteur, soc)
        .or_else(|| texte.as_deref().and_then(|t| societe::dans_texte(t, soc)));
    Lu {
        etat: "lu",
        voie: Some(voie.code()),
        type_doc: Some(d.type_doc.clone()),
        societe_id,
        fournisseur: extraction::Fournisseur {
            nom: d.vendeur.nom.clone(),
            siret: d.vendeur.siret.clone(),
            tva: d.vendeur.tva.clone(),
        },
        numero: d.numero.clone(),
        date: d.date.as_deref().and_then(extraction::date),
        commande: d.numero_commande.clone(),
        ht: d.montant_ht,
        tva: d.montant_tva,
        ttc: d.montant_ttc,
        confiance: confiance_xml(societe_id.is_some()),
        lignes: d.lignes.clone(),
        numeros_bl: d.numeros_bl.clone(),
        lignes_lues: true,
        texte,
        resultat: serde_json::to_value(&d).ok(),
        ..Default::default()
    }
}

fn lu_depuis_extraction(
    voie: Voie,
    e: Extraction,
    societe_code: Option<i64>,
    texte_code: Option<String>,
    reglage: &reglages::Reglage,
) -> Lu {
    let mut confiance = serde_json::to_value(&e.confiance).unwrap_or_default();
    if societe_code.is_some() {
        confiance["societe"] = json!(1.0);
    }
    // Les lignes sont gardées dans leur table, pas en double dans le résultat brut.
    let resultat = serde_json::to_value(&e).ok().map(|mut v| {
        v["lignes"] = json!(e.lignes.len());
        v
    });
    // Seuls les factures, avoirs et BL servent : relance, devis, autre sont ignorés.
    let garde = matches!(e.r#type.as_str(), "facture" | "avoir" | "bl");
    Lu {
        etat: if garde { "lu" } else { "ignore" },
        message: (!garde).then(|| {
            format!(
                "document de type « {} » : seuls les factures, avoirs et bons de livraison sont gardés",
                e.r#type
            )
        }),
        voie: Some(voie.code()),
        type_doc: Some(e.r#type.clone()),
        societe_id: societe_code.or(e.societe_id),
        numero: e.numero,
        date: e.date.as_deref().and_then(extraction::date),
        commande: e.numero_commande,
        ht: e.montant_ht,
        tva: e.montant_tva,
        ttc: e.montant_ttc,
        fournisseur: e.fournisseur,
        confiance,
        lignes: e.lignes,
        numeros_bl: e.numeros_bl,
        lignes_lues: true,
        texte: texte_code.or(e.texte),
        resultat,
        modele: Some(reglage.modele.clone()),
        effort: Some(reglage.effort.clone()),
    }
}

/// Un appel de lecture. `Ok(Err(..))` : réponse reçue mais non conforme.
async fn appel_lecture(
    ctx: &Contexte,
    tache: &str,
    reglage: &reglages::Reglage,
    prompt: &str,
    avec_fichier: Option<&std::path::Path>,
    acces: &claude::Acces,
) -> Result<Result<Extraction, ErreurExtraction>, ErreurClaude> {
    let schema = extraction::schema(avec_fichier.is_some()).to_string();
    let mut args: Vec<&str> = vec![
        "--json-schema",
        &schema,
        "--system-prompt",
        extraction::PROMPT_SYSTEME,
        "--strict-mcp-config",
        "--disable-slash-commands",
    ];
    if avec_fichier.is_some() {
        args.extend(["--tools", "Read", "--allowedTools", "Read"]);
    } else {
        args.extend(["--tools", ""]);
    }
    let mut a = Appel::avec_acces(tache, &reglage.modele, prompt, acces);
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    a.dossier = avec_fichier;
    a.delai = claude::DELAI_LECTURE;
    let sortie = claude::lancer(&ctx.pool, &ctx.config.claude_bin, &a).await?;
    let societes = societes(&ctx.pool).await.unwrap_or_default();
    Ok(extraction::depuis_reponse(&sortie.json, &societes))
}

pub(crate) async fn noter_utilisation(pool: &PgPool) {
    let quand = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let _ = sqlx::query!(
        "update parametre set valeur = jsonb_set(valeur, '{utilise_le}', to_jsonb($1::text))
         where cle = $2 and jsonb_typeof(valeur) = 'object'",
        quand,
        parametre::CLAUDE_ETAT
    )
    .execute(pool)
    .await;
}

pub async fn lire(ctx: &Contexte, document_id: i64, forcer: bool) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some(doc) = sqlx::query_as!(
        Entree,
        "update document d set etat = 'en_cours', pris_le = now()
         from piece_jointe p, mail m
         where d.id = $1 and d.etat = 'a_lire' and p.id = d.piece_jointe_id and m.id = d.mail_id
         returning d.sha256, p.nom, p.type_mime, p.rang, m.chemin_eml, m.expediteur, m.objet,
                   m.corps_texte as corps",
        document_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let resultat = lire_pris(ctx, document_id, forcer, doc).await;
    if resultat.is_ok() {
        apres_lecture(ctx, document_id).await;
    }
    if let Err(e) = &resultat {
        // Erreur imprévue (fichier absent, base) : le document est marqué en erreur, pas bloqué.
        let _ = sqlx::query!(
            "update document set etat = 'erreur', message = $2, pris_le = null, lu_le = now()
             where id = $1 and etat = 'en_cours'",
            document_id,
            format!("{e:#}")
        )
        .execute(pool)
        .await;
    }
    resultat
}

/// Après une lecture : fiche fournisseur, contrôles et statut de dépôt, puis rapprochement
/// des BL et des factures. Une erreur ici ne remet pas la lecture en cause : elle est
/// journalisée, et la fiche du document permet de relancer les contrôles.
pub async fn apres_lecture(ctx: &Contexte, document_id: i64) {
    let etat = sqlx::query_scalar!("select etat from document where id = $1", document_id)
        .fetch_optional(&ctx.pool)
        .await;
    if !matches!(etat, Ok(Some(ref e)) if e == "lu") {
        // Relu en erreur ou ignoré : ses liens automatiques tombent.
        if let Err(e) = gmflow_core::rapprochement::recalculer(&ctx.pool, document_id).await {
            tracing::warn!(document = document_id, erreur = ?e, "rapprochement impossible");
        }
        return;
    }
    if let Err(e) = crate::fournisseur::rattacher(ctx, document_id).await {
        tracing::warn!(document = document_id, erreur = ?e, "rattachement au fournisseur impossible");
    }
    if let Err(e) = gmflow_core::depot::evaluer_et_deposer(&ctx.pool, document_id).await {
        tracing::warn!(document = document_id, erreur = ?e, "contrôles avant dépôt impossibles");
    }
    if let Err(e) = gmflow_core::rapprochement::recalculer(&ctx.pool, document_id).await {
        tracing::warn!(document = document_id, erreur = ?e, "rapprochement impossible");
    }
}

/// Exemples de corrections du fournisseur probable, et format habituel de ses BL, à ajouter
/// au prompt.
async fn exemples_pour(pool: &PgPool, expediteur: &str, texte: Option<&str>) -> String {
    match crate::fournisseur::pre_identifier(pool, expediteur, texte).await {
        Ok(Some(f)) => {
            let mut s = crate::fournisseur::exemples(pool, f)
                .await
                .unwrap_or_default();
            if let Ok(Some(format)) =
                sqlx::query_scalar!("select format_bl from fournisseur where id = $1", f)
                    .fetch_one(pool)
                    .await
                && !format.trim().is_empty()
            {
                s.push_str(&format!(
                    "\nFormat habituel des bons de livraison de ce fournisseur : {}\n",
                    format.trim()
                ));
            }
            s
        }
        _ => String::new(),
    }
}

struct Entree {
    sha256: Vec<u8>,
    nom: String,
    type_mime: String,
    rang: i32,
    chemin_eml: String,
    expediteur: String,
    objet: String,
    corps: String,
}

async fn lire_pris(ctx: &Contexte, id: i64, forcer: bool, e: Entree) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    if !forcer && reprendre_identique(pool, id, &e.sha256).await? {
        return Ok(());
    }
    let brut =
        tokio::fs::read(stockage::chemin_absolu(&ctx.config.fichiers, &e.chemin_eml)).await?;
    let (contenu, aiguillage) = {
        let (nom, mime, rang) = (e.nom.clone(), e.type_mime.clone(), e.rang);
        tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<u8>, Aiguillage)> {
            let contenu = eml::extraire_piece(&brut, rang)?;
            let liste = eml::liste_diffusion(&brut);
            let a = lect::aiguiller(&nom, &mime, &contenu, liste);
            Ok((contenu, a))
        })
        .await??
    };
    let soc = societes(pool).await?;
    let lu = match aiguillage {
        Aiguillage::Ignore(raison) => Lu {
            etat: "ignore",
            message: Some(raison),
            confiance: json!({}),
            ..Default::default()
        },
        Aiguillage::Structure {
            voie,
            donnees,
            texte,
        } => lu_depuis_xml(voie, *donnees, texte, &soc),
        Aiguillage::Texte { voie, texte } => {
            let societe_code = societe::dans_texte(&texte, &soc);
            let mail = ContexteMail {
                expediteur: &e.expediteur,
                objet: &e.objet,
                corps: &e.corps,
                nom_fichier: &e.nom,
            };
            let mut prompt = extraction::prompt(&mail, &soc, &Contenu::Texte(&texte));
            prompt.push_str(&exemples_pour(pool, &e.expediteur, Some(&texte)).await);
            match lire_par_claude(ctx, id, voie, reglages::EXTRACTION_TEXTE, &prompt, None).await? {
                Lecture::Faite(ex, reglage) => {
                    lu_depuis_extraction(voie, ex, societe_code, Some(texte), &reglage)
                }
                Lecture::Reportee => return Ok(()),
                Lecture::Echec(lu) => lu,
            }
        }
        Aiguillage::Scan { extension, pages } => {
            let dossier = DossierTemp::creer("gmflow-lecture")?;
            let nom_fichier = format!("document.{extension}");
            tokio::fs::write(dossier.0.join(&nom_fichier), &contenu).await?;
            let mail = ContexteMail {
                expediteur: &e.expediteur,
                objet: &e.objet,
                corps: &e.corps,
                nom_fichier: &e.nom,
            };
            let mut prompt = extraction::prompt(
                &mail,
                &soc,
                &Contenu::Fichier {
                    nom: &nom_fichier,
                    pages,
                },
            );
            prompt.push_str(&exemples_pour(pool, &e.expediteur, None).await);
            match lire_par_claude(
                ctx,
                id,
                Voie::Scan,
                reglages::OCR_SCAN,
                &prompt,
                Some(&dossier.0),
            )
            .await?
            {
                Lecture::Faite(ex, reglage) => {
                    let societe_code = ex
                        .texte
                        .as_deref()
                        .and_then(|t| societe::dans_texte(t, &soc));
                    lu_depuis_extraction(Voie::Scan, ex, societe_code, None, &reglage)
                }
                Lecture::Reportee => return Ok(()),
                Lecture::Echec(lu) => lu,
            }
        }
    };
    let details =
        json!({ "etat": lu.etat, "voie": lu.voie, "type": lu.type_doc, "message": lu.message });
    enregistrer(pool, id, lu).await?;
    journal::ecrire(
        pool,
        None,
        "lecture",
        "document",
        Some(id.to_string()),
        details,
    )
    .await?;
    Ok(())
}

enum Lecture {
    Faite(Extraction, reglages::Reglage),
    /// Claude indisponible : le document attend (état `a_lire`).
    Reportee,
    /// Réponse non conforme même après le nouvel essai.
    Echec(Lu),
}

async fn lire_par_claude(
    ctx: &Contexte,
    id: i64,
    voie: Voie,
    tache: &str,
    prompt: &str,
    dossier: Option<&std::path::Path>,
) -> anyhow::Result<Lecture> {
    let pool = &ctx.pool;
    let r = reglages::lire(pool).await?;
    if let Some(raison) = claude_indisponible(pool, &r).await? {
        remettre(pool, id, voie, &raison).await?;
        return Ok(Lecture::Reportee);
    }
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        remettre(pool, id, voie, "jeton Claude absent").await?;
        return Ok(Lecture::Reportee);
    };
    let mut derniere_erreur = String::new();
    for (essai, tache) in [tache, reglages::NOUVEL_ESSAI].into_iter().enumerate() {
        let reglage = r.pour(tache).clone();
        match appel_lecture(ctx, tache, &reglage, prompt, dossier, &acces).await {
            Ok(Ok(ex)) => {
                noter_utilisation(pool).await;
                return Ok(Lecture::Faite(ex, reglage));
            }
            Ok(Err(e)) => {
                tracing::warn!(document = id, essai, erreur = %e, "réponse de lecture non conforme");
                derniere_erreur = e.to_string();
            }
            Err(ErreurClaude::JsonInvalide) => {
                derniere_erreur = ErreurClaude::JsonInvalide.to_string();
            }
            Err(e) => {
                // Panne, délai, jeton refusé : nouvel essai par la pompe dans 30 minutes.
                remettre(pool, id, voie, &format!("Claude : {e}")).await?;
                return Ok(Lecture::Reportee);
            }
        }
    }
    // Le détail technique va au journal ; la file À traiter montre une phrase lisible.
    tracing::warn!(document = id, erreur = %derniere_erreur, "lecture abandonnée après deux essais");
    Ok(Lecture::Echec(Lu {
        etat: "erreur",
        voie: Some(voie.code()),
        message: Some("Claude n’a pas réussi à le lire, même au second essai.".into()),
        confiance: json!({}),
        ..Default::default()
    }))
}
