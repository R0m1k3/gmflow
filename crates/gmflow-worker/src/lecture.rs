//! Lecture des pièces jointes : pompe (création des documents, mise en file) et tâche
//! `LireDocument` (aiguillage, lecture par le code, puis un seul appel à Claude si besoin).

use gmflow_core::attente::{self, VOIES_CLAUDE};
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

/// Claude peut-il lire maintenant (jeton présent, pas de refus en cours, quota du jour non
/// atteint) ? Sinon, pourquoi (phrase gardée dans `document.message`).
pub(crate) async fn claude_indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
) -> anyhow::Result<Option<String>> {
    Ok(attente::lecture_indisponible(pool, r, false)
        .await?
        .map(|i| i.message))
}

/// Crée les documents des pièces jointes nouvelles, remet en file les documents bloqués, puis
/// pousse les documents à lire, les plus récents d'abord. Un document qui a déjà une tâche en
/// file n'est pas repoussé ; un mail de moins de 24 h passe devant l'arriéré (priorité apalis).
/// Rend le nombre de tâches poussées.
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
    let en_file = file::documents_en_file(pool).await?;
    let choisis = sqlx::query!(
        r#"with choisis as (
             select d.id, coalesce(m.date_mail, m.recu_le) as recu from document d
             join mail m on m.id = d.mail_id
             where d.etat = 'a_lire'
               and (d.planifie_le is null or d.planifie_le < now() - interval '30 minutes')
               and not ($1 and d.voie is not null and d.voie = any($2))
               and d.id <> all($4)
             order by m.date_mail desc nulls last, d.id desc
             limit $3
             for update of d skip locked
           ), maj as (
             update document set planifie_le = now() where id in (select id from choisis)
           )
           select id as "id!", coalesce(recu > now() - interval '24 hours', false) as "recent!"
           from choisis order by recu desc nulls last, id desc"#,
        sans_claude,
        &voies,
        PAR_TOUR,
        &en_file
    )
    .fetch_all(pool)
    .await?;
    for c in &choisis {
        let priorite = if c.recent {
            file::PRIORITE_RECENT
        } else {
            file::PRIORITE_ARRIERE
        };
        file::pousser_lecture_auto(pool, c.id, priorite).await?;
    }
    Ok(choisis.len())
}

/// La pompe après une relève : une erreur n'arrête pas la relève.
pub async fn pompe_apres_releve(pool: &PgPool) {
    if let Err(e) = pompe(pool).await {
        tracing::warn!(erreur = ?e, "mise en file des lectures impossible");
    }
}

pub async fn societes(pool: &PgPool) -> sqlx::Result<Vec<SocieteRef>> {
    let lignes = sqlx::query!(
        "select id, raison_sociale, autres_noms, siret, tva, adresses from societe where not archivee order by id"
    )
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|l| SocieteRef {
            id: l.id,
            raison_sociale: l.raison_sociale,
            autres_noms: l.autres_noms,
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
    echeance: Option<time::Date>,
    commande: Option<String>,
    ht: Option<f64>,
    tva: Option<f64>,
    ttc: Option<f64>,
    remise_ht: Option<f64>,
    remise_taux: Option<f64>,
    annexes_ht: Option<f64>,
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
        "update document set numeros_bl = $2, lignes_lues = $3, magasin_verifie_le = null,
                             magasin_erreur = null, date_echeance = $4
         where id = $1",
        id,
        &lu.numeros_bl,
        lu.lignes_lues,
        lu.echeance
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update document set etat = $2, voie = $3, message = $4, type = $5, societe_id = $6,
            fournisseur_nom = $7, fournisseur_siret = $8, fournisseur_tva = $9, numero = $10,
            date_document = $11, numero_commande = $12,
            montant_ht = $13::float8::numeric(14, 2), montant_tva = $14::float8::numeric(14, 2),
            montant_ttc = $15::float8::numeric(14, 2), confiance = $16, texte = $17,
            resultat = $18, modele = $19, effort = $20, fournisseur_groupe = $21,
            fournisseur_entite = $22, remise_ht = $23::float8::numeric(14, 2),
            remise_taux = $24::float8::numeric(6, 3),
            annexes_ht = $25::float8::numeric(14, 2), copie_de = null, pris_le = null,
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
        lu.effort,
        lu.fournisseur.groupe,
        lu.fournisseur.entite,
        lu.remise_ht.filter(|r| *r > 0.0),
        lu.remise_taux.filter(|t| *t > 0.0 && *t < 100.0),
        lu.annexes_ht.filter(|a| *a > 0.0)
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
            lignes_lues, fournisseur_groupe, fournisseur_entite, remise_ht, remise_taux,
            annexes_ht, magasin_verifie_le, magasin_erreur, date_echeance) =
          (select s.etat, s.voie, s.message, s.type, s.societe_id, s.fournisseur_nom,
            s.fournisseur_siret, s.fournisseur_tva, s.numero, s.date_document, s.numero_commande,
            s.montant_ht, s.montant_tva, s.montant_ttc, s.confiance, s.texte, s.resultat, s.modele,
            s.effort, s.numeros_bl, s.lignes_lues, s.fournisseur_groupe, s.fournisseur_entite,
            s.remise_ht, s.remise_taux, s.annexes_ht, s.magasin_verifie_le, s.magasin_erreur,
            s.date_echeance
            from document s where s.id = $2),
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
                                     prix_unitaire_ht, montant_ht, gencod, colis, pcb, remise_taux,
                                     taux_tva, deee_ht, ecomob_ht, pv_conseille, magasin)
         select $1, rang, reference, designation, quantite, unite, prix_unitaire_ht, montant_ht,
                gencod, colis, pcb, remise_taux, taux_tva, deee_ht, ecomob_ht, pv_conseille, magasin
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
            ..Default::default()
        },
        numero: d.numero.clone(),
        date: d.date.as_deref().and_then(extraction::date),
        echeance: d.echeance.as_deref().and_then(extraction::date),
        commande: d.numero_commande.clone(),
        ht: d.montant_ht,
        tva: d.montant_tva,
        ttc: d.montant_ttc,
        remise_ht: d.remise_ht.map(f64::abs),
        annexes_ht: d.annexes_ht.map(f64::abs),
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
        echeance: e.echeance.as_deref().and_then(extraction::date),
        commande: e.numero_commande,
        ht: e.montant_ht,
        tva: e.montant_tva,
        ttc: e.montant_ttc,
        remise_ht: e.remise_ht.map(f64::abs),
        remise_taux: e.remise_taux.map(f64::abs),
        annexes_ht: e.annexes_ht.map(f64::abs),
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

/// Lecture gardée malgré des anomalies : elles sont notées dans le message du document.
fn avec_anomalies(mut lu: Lu, anomalies: Vec<String>) -> Lu {
    if lu.etat == "lu" && !anomalies.is_empty() {
        lu.message = Some(format!("Lecture à vérifier : {}.", anomalies.join(" ; ")));
    }
    lu
}

/// Un appel de lecture. `Ok(Err(..))` : réponse reçue mais non conforme.
async fn appel_lecture(
    ctx: &Contexte,
    tache: &str,
    reglage: &reglages::Reglage,
    prompt: &str,
    avec_fichier: Option<&std::path::Path>,
    transcription: bool,
    acces: &claude::Acces,
) -> Result<Result<Extraction, ErreurExtraction>, ErreurClaude> {
    let schema = extraction::schema(transcription).to_string();
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

pub async fn lire(
    ctx: &Contexte,
    document_id: i64,
    forcer: bool,
    approfondie: bool,
    hors_quota: bool,
) -> anyhow::Result<()> {
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
    let resultat = lire_pris(ctx, document_id, forcer, approfondie, hors_quota, doc).await;
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
    if let Err(e) = crate::serveur_api::demander_verification_bl(&ctx.pool, document_id).await {
        tracing::warn!(document = document_id, erreur = ?e, "recherche des articles au magasin impossible");
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

async fn lire_pris(
    ctx: &Contexte,
    id: i64,
    forcer: bool,
    approfondie: bool,
    hors_quota: bool,
    e: Entree,
) -> anyhow::Result<()> {
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
            let src = Source {
                dossier: None,
                texte: Some(&texte),
                pdf: (voie == Voie::PdfTexte).then_some(contenu.as_slice()),
                pages: None,
                approfondie,
                hors_quota,
            };
            match lire_par_claude(ctx, id, voie, reglages::EXTRACTION_TEXTE, &prompt, src).await? {
                Lecture::Faite(ex, reglage, an) => {
                    let lu = lu_depuis_extraction(voie, ex, societe_code, Some(texte), &reglage);
                    avec_anomalies(lu, an)
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
                Source {
                    dossier: Some(&dossier.0),
                    texte: None,
                    pdf: None,
                    pages: Some(pages),
                    approfondie,
                    hors_quota,
                },
            )
            .await?
            {
                Lecture::Faite(ex, reglage, an) => {
                    let societe_code = ex
                        .texte
                        .as_deref()
                        .and_then(|t| societe::dans_texte(t, &soc));
                    let lu = lu_depuis_extraction(Voie::Scan, ex, societe_code, None, &reglage);
                    avec_anomalies(lu, an)
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

/// Phrase gardée sur un document remis en attente après un appel en échec : l'explication du
/// refus notée par `claude::lancer` (heure de reprise comprise), sinon la panne.
pub(crate) async fn message_echec(pool: &PgPool, e: &ErreurClaude) -> String {
    if claude::refus(e).is_some()
        && let Ok(Some(i)) = attente::refus_actif(pool).await
    {
        return i.message;
    }
    format!("Claude n’a pas répondu ({e}) : nouvel essai dans 30 minutes")
}

enum Lecture {
    /// Lecture faite ; les anomalies restantes (vide si aucune) vont dans le message.
    Faite(Extraction, reglages::Reglage, Vec<String>),
    /// Claude indisponible : le document attend (état `a_lire`).
    Reportee,
    /// Réponse non conforme même après le nouvel essai.
    Echec(Lu),
}

/// Ce que l'appel de lecture a sous la main, en plus du prompt.
struct Source<'a> {
    /// Dossier du fichier à lire par Claude (scans), transcription demandée.
    dossier: Option<&'a std::path::Path>,
    /// Texte lu par le code, pour vérifier les montants.
    texte: Option<&'a str>,
    /// PDF d'origine d'un PDF texte, joint au nouvel essai.
    pdf: Option<&'a [u8]>,
    /// Pages du fichier à lire (scans) ; celles du PDF joint sont comptées au nouvel essai.
    pages: Option<usize>,
    /// Relecture en détail demandée par un humain : pas de premier essai au réglage de la tâche.
    approfondie: bool,
    /// Demande d'un humain sur ce document : le quota du jour ne compte pas.
    hors_quota: bool,
}

/// Lecture par Claude en deux temps au plus : le réglage de la tâche, puis le nouvel essai
/// (modèle plus fort) si la réponse est non conforme ou incomplète (`extraction::anomalies` :
/// TTC absent, HT + TVA ≠ TTC, TTC absent du texte, BL sans lignes). Le nouvel essai d'un PDF
/// texte reçoit aussi le fichier ; on garde la lecture qui a le moins d'anomalies.
async fn lire_par_claude(
    ctx: &Contexte,
    id: i64,
    voie: Voie,
    tache: &str,
    prompt: &str,
    src: Source<'_>,
) -> anyhow::Result<Lecture> {
    let pool = &ctx.pool;
    let r = reglages::lire(pool).await?;
    if let Some(i) = attente::lecture_indisponible(pool, &r, src.hors_quota).await? {
        remettre(pool, id, voie, &i.message).await?;
        return Ok(Lecture::Reportee);
    }
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        remettre(
            pool,
            id,
            voie,
            &attente::lecture_indisponible(pool, &r, true)
                .await?
                .map(|i| i.message)
                .unwrap_or_else(|| "Claude n’est pas connecté".into()),
        )
        .await?;
        return Ok(Lecture::Reportee);
    };
    let transcription = src.dossier.is_some();

    // Premier essai (sauf relecture en détail : directement le nouvel essai).
    let reglage = r.pour(tache).clone();
    let premiere = if src.approfondie {
        None
    } else {
        match appel_lecture(
            ctx,
            tache,
            &reglage,
            prompt,
            src.dossier,
            transcription,
            &acces,
        )
        .await
        {
            Ok(Ok(ex)) => {
                noter_utilisation(pool).await;
                let an = extraction::anomalies(&ex, src.texte);
                if an.is_empty() {
                    return Ok(Lecture::Faite(ex, reglage, an));
                }
                tracing::info!(document = id, anomalies = ?an, "lecture incomplète, nouvel essai");
                Some((ex, reglage, an))
            }
            Ok(Err(e)) => {
                tracing::warn!(document = id, erreur = %e, "réponse de lecture non conforme");
                None
            }
            Err(ErreurClaude::JsonInvalide) => {
                tracing::warn!(document = id, "réponse de lecture : JSON invalide");
                None
            }
            Err(e) => {
                // Panne, délai, jeton refusé : nouvel essai par la pompe dans 30 minutes.
                remettre(pool, id, voie, &message_echec(pool, &e).await).await?;
                return Ok(Lecture::Reportee);
            }
        }
    };

    // Nouvel essai : réglage plus fort, ce qui manquait, et le PDF d'un PDF texte.
    if let Some(i) = attente::lecture_indisponible(pool, &r, src.hors_quota).await? {
        if let Some((ex, reglage, an)) = premiere {
            return Ok(Lecture::Faite(ex, reglage, an));
        }
        remettre(pool, id, voie, &i.message).await?;
        return Ok(Lecture::Reportee);
    }
    let mut _garde = None;
    let (dossier, fichier) = match (src.dossier, src.pdf) {
        (Some(d), _) => (Some(d.to_path_buf()), None),
        (None, Some(pdf)) => {
            let d = DossierTemp::creer("gmflow-lecture")?;
            tokio::fs::write(d.0.join("document.pdf"), pdf).await?;
            let chemin = d.0.clone();
            _garde = Some(d);
            (Some(chemin), Some("document.pdf"))
        }
        (None, None) => (None, None),
    };
    let pages = match src.pdf {
        Some(pdf) => Some(lect::pdf::pages(pdf)),
        None => src.pages,
    };
    let mut prompt2 = prompt.to_string();
    match &premiere {
        Some((_, _, an)) => {
            prompt2.push_str(&extraction::consigne_nouvel_essai(an, fichier, pages))
        }
        None if src.approfondie => prompt2.push_str(&extraction::consigne_nouvel_essai(
            &[
                "un humain a demandé cette relecture : la lecture précédente était incomplète \
               ou fausse (montants, numéro, date ou lignes)"
                    .to_string(),
            ],
            fichier,
            pages,
        )),
        None => {
            if let Some(f) = fichier {
                prompt2.push_str(&extraction::consigne_nouvel_essai(&[], Some(f), pages));
            }
        }
    }
    let reglage2 = r.pour(reglages::NOUVEL_ESSAI).clone();
    let seconde = appel_lecture(
        ctx,
        reglages::NOUVEL_ESSAI,
        &reglage2,
        &prompt2,
        dossier.as_deref(),
        transcription,
        &acces,
    )
    .await;
    let derniere_erreur = match seconde {
        Ok(Ok(ex)) => {
            noter_utilisation(pool).await;
            let an = extraction::anomalies(&ex, src.texte);
            if let Some((ex1, reglage1, an1)) = premiere
                && an1.len() < an.len()
            {
                return Ok(Lecture::Faite(ex1, reglage1, an1));
            }
            return Ok(Lecture::Faite(ex, reglage2, an));
        }
        Ok(Err(e)) => {
            tracing::warn!(document = id, erreur = %e, "réponse de lecture non conforme au nouvel essai");
            e.to_string()
        }
        Err(e) => {
            tracing::warn!(document = id, erreur = %e, "nouvel essai de lecture impossible");
            if premiere.is_none() && !matches!(e, ErreurClaude::JsonInvalide) {
                remettre(pool, id, voie, &message_echec(pool, &e).await).await?;
                return Ok(Lecture::Reportee);
            }
            e.to_string()
        }
    };
    if let Some((ex, reglage, an)) = premiere {
        return Ok(Lecture::Faite(ex, reglage, an));
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
