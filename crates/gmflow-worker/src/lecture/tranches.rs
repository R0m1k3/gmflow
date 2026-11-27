//! Lecture par tranches des très gros PDF (texte, ou scan lu par l'OCR local) : un appel pour
//! l'en-tête et les totaux, puis un appel par tranche de pages pour les lignes, l'une après
//! l'autre, et la fusion par le code (`gmflow_core::lecture::tranches`). L'avancement est gardé
//! dans `lecture_tranche` : une tâche relancée (panne, quota du jour atteint, redémarrage)
//! reprend là où elle s'était arrêtée. Une tranche en échec ne perd pas les autres : elle est
//! réessayée seule (modèle du nouvel essai, PDF joint), puis notée en échec, et la pièce est
//! alors marquée « lecture incomplète » avec les pages non lues.

use std::path::{Path, PathBuf};

use gmflow_core::claude::{self, Appel, DossierTemp, ErreurClaude};
use gmflow_core::lecture::extraction::{self, Contenu, ContexteMail, Extraction};
use gmflow_core::lecture::ocr::TexteOcr;
use gmflow_core::lecture::societe::{self, SocieteRef};
use gmflow_core::lecture::tranches::{self as tr, Annonces, Tranche};
use gmflow_core::lecture::{self as lect, Aiguillage, Voie};
use gmflow_core::{attente, reglages};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::{Entree, Lecture, Lu};
use crate::Contexte;

/// Nom du PDF dans le dossier de travail du CLI.
const FICHIER: &str = "document.pdf";

/// Ce que la lecture par tranches sait du document.
struct Doc<'a> {
    id: i64,
    voie: Voie,
    /// Texte de chaque page (couche texte ou OCR).
    textes: Vec<String>,
    /// Texte complet, pour la recherche de la société et le contrôle des montants.
    texte: &'a str,
    /// Scan lu par l'OCR : le PDF est joint à chaque appel ; sinon seulement aux nouveaux essais.
    ocr: Option<&'a TexteOcr>,
    contenu: &'a [u8],
}

/// Lit le document en plusieurs parties s'il ne tient pas en un appel. `None` : lecture
/// normale (document court, ou pas un PDF lu par le code) ; `Some(None)` : lecture reportée
/// (Claude indisponible, quota du jour), l'avancement est gardé ; `Some(Some(lu))` : à
/// enregistrer.
#[allow(clippy::too_many_arguments)]
pub(super) async fn essayer(
    ctx: &Contexte,
    id: i64,
    e: &Entree,
    contenu: &[u8],
    aiguillage: &Aiguillage,
    ocr: Option<&(Voie, TexteOcr)>,
    soc: &[SocieteRef],
    forcer: bool,
    approfondie: bool,
    hors_quota: bool,
) -> anyhow::Result<Option<Option<Lu>>> {
    let doc = match (ocr, aiguillage) {
        (Some((voie, t)), _) => Doc {
            id,
            voie: *voie,
            textes: t.textes_pages.clone(),
            texte: &t.texte,
            ocr: Some(t),
            contenu,
        },
        (
            None,
            Aiguillage::Texte {
                voie: Voie::PdfTexte,
                texte,
                ..
            },
        ) => {
            // Le texte par page n'est calculé que pour un texte assez long.
            if texte.chars().count() <= extraction::TEXTE_MAX
                && texte.lines().count() <= extraction::LIGNES_MAX
            {
                return Ok(None);
            }
            let c = contenu.to_vec();
            let Some(textes) =
                tokio::task::spawn_blocking(move || lect::pdf::texte_par_page(&c)).await?
            else {
                return Ok(None);
            };
            Doc {
                id,
                voie: Voie::PdfTexte,
                textes,
                texte,
                ocr: None,
                contenu,
            }
        }
        _ => return Ok(None),
    };
    if !tr::a_decouper(&doc.textes) {
        return Ok(None);
    }
    if forcer || approfondie {
        effacer(&ctx.pool, id).await?;
    }
    lire(ctx, &doc, e, soc, approfondie, hors_quota)
        .await
        .map(Some)
}

/// Avancement effacé (lecture terminée, relecture demandée).
async fn effacer(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query!("delete from lecture_tranche where document_id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Une ligne de `lecture_tranche`.
struct Etape {
    rang: i32,
    debut: i32,
    fin: i32,
    etat: String,
    reponse: Option<Value>,
    modele: Option<String>,
}

/// Plan gardé, ou nouveau plan (l'ancien est remplacé s'il ne correspond plus aux pages).
async fn preparer(
    pool: &PgPool,
    id: i64,
    plan: &[Tranche],
    pages: usize,
) -> sqlx::Result<Vec<Etape>> {
    let mut voulu = vec![(0, 1, pages as i32)];
    voulu.extend(
        plan.iter()
            .map(|t| (t.rang as i32, t.debut as i32, t.fin as i32)),
    );
    let lire = || {
        sqlx::query_as!(
            Etape,
            "select rang, page_debut as debut, page_fin as fin, etat, reponse, modele
             from lecture_tranche where document_id = $1 order by rang",
            id
        )
        .fetch_all(pool)
    };
    let gardees = lire().await?;
    let meme_plan = gardees.len() == voulu.len()
        && gardees
            .iter()
            .zip(&voulu)
            .all(|(g, v)| (g.rang, g.debut, g.fin) == *v);
    if meme_plan {
        return Ok(gardees);
    }
    let mut tx = pool.begin().await?;
    sqlx::query!("delete from lecture_tranche where document_id = $1", id)
        .execute(&mut *tx)
        .await?;
    let (rangs, debuts, fins): (Vec<i32>, Vec<i32>, Vec<i32>) = voulu.iter().fold(
        (Vec::new(), Vec::new(), Vec::new()),
        |(mut r, mut d, mut f), (a, b, c)| {
            r.push(*a);
            d.push(*b);
            f.push(*c);
            (r, d, f)
        },
    );
    sqlx::query!(
        "insert into lecture_tranche (document_id, rang, page_debut, page_fin)
         select $1, * from unnest($2::int[], $3::int[], $4::int[])",
        id,
        &rangs,
        &debuts,
        &fins
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    lire().await
}

async fn noter(
    pool: &PgPool,
    id: i64,
    rang: i32,
    etat: &str,
    reponse: Option<&Value>,
    modele: &str,
    message: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "update lecture_tranche set etat = $3, reponse = coalesce($4, reponse), modele = $5,
                essais = essais + 1, message = $6, maj_le = now()
         where document_id = $1 and rang = $2",
        id,
        rang,
        etat,
        reponse,
        modele,
        message
    )
    .execute(pool)
    .await?;
    // La lecture avance : le document n'est pas « bloqué » (pompe, `attente::expliquer`).
    sqlx::query!(
        "update document set pris_le = now() where id = $1 and etat = 'en_cours'",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Dossier de travail avec le PDF, créé au premier besoin.
struct Fichier<'a> {
    contenu: &'a [u8],
    dossier: Option<DossierTemp>,
}

impl Fichier<'_> {
    async fn chemin(&mut self) -> anyhow::Result<PathBuf> {
        if self.dossier.is_none() {
            let d = DossierTemp::creer("gmflow-lecture")?;
            tokio::fs::write(d.0.join(FICHIER), self.contenu).await?;
            self.dossier = Some(d);
        }
        Ok(self
            .dossier
            .as_ref()
            .map(|d| d.0.clone())
            .unwrap_or_default())
    }
}

/// Un appel au CLI avec ce schéma ; rend la sortie JSON brute.
#[allow(clippy::too_many_arguments)]
async fn appeler(
    ctx: &Contexte,
    id: i64,
    tache: &str,
    reglage: &reglages::Reglage,
    systeme: &str,
    schema: &Value,
    prompt: &str,
    dossier: Option<&Path>,
    acces: &claude::Acces,
) -> Result<Value, ErreurClaude> {
    let schema = schema.to_string();
    let mut args: Vec<&str> = vec![
        "--json-schema",
        &schema,
        "--system-prompt",
        systeme,
        "--strict-mcp-config",
        "--disable-slash-commands",
    ];
    if dossier.is_some() {
        // Mêmes garde-fous que `appel_lecture` : Read limité au dossier jetable de la pièce.
        args.extend([
            "--tools",
            "Read",
            "--allowedTools",
            "Read(./**)",
            "--permission-mode",
            "dontAsk",
            "--settings",
            super::REGLAGES_LECTURE,
        ]);
    } else {
        args.extend(["--tools", ""]);
    }
    let mut a = Appel::avec_acces(tache, &reglage.modele, prompt, acces);
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    a.dossier = dossier;
    a.delai = claude::DELAI_LECTURE;
    a.document_id = Some(id);
    let sortie = claude::lancer(&ctx.pool, &ctx.config.claude_bin, &a).await?;
    super::noter_utilisation(&ctx.pool).await;
    Ok(sortie.json)
}

/// Remet le document en attente, avancement gardé.
async fn reporter(pool: &PgPool, d: &Doc<'_>, raison: &str) -> anyhow::Result<Option<Lu>> {
    let (lues, total) = avancement(pool, d.id).await?;
    let message = format!(
        "Lecture en {total} parties : {lues} déjà lue{s}, gardée{s} pour la suite. {raison}",
        s = if lues > 1 { "s" } else { "" }
    );
    super::remettre(pool, d.id, d.voie, &message).await?;
    Ok(None)
}

/// (parties lues, parties du plan) ; l'appel d'en-tête est la première partie.
async fn avancement(pool: &PgPool, id: i64) -> sqlx::Result<(i64, i64)> {
    let r = sqlx::query!(
        r#"select count(*) filter (where etat = 'lue') as "lues!", count(*) as "total!"
           from lecture_tranche where document_id = $1"#,
        id
    )
    .fetch_one(pool)
    .await?;
    Ok((r.lues, r.total))
}

/// Raison d'attendre avant le prochain appel (accès absent, refus en cours, quota du jour).
async fn indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
    hors_quota: bool,
) -> anyhow::Result<Option<String>> {
    Ok(attente::lecture_indisponible(pool, r, hors_quota)
        .await?
        .map(|i| i.message))
}

async fn lire(
    ctx: &Contexte,
    d: &Doc<'_>,
    e: &Entree,
    soc: &[SocieteRef],
    approfondie: bool,
    hors_quota: bool,
) -> anyhow::Result<Option<Lu>> {
    let pool = &ctx.pool;
    let pages = d.textes.len();
    let plan_complet = tr::plan(&d.textes);
    let hors_plafond = tr::hors_plafond(&plan_complet);
    let plan = &plan_complet[..plan_complet.len().min(tr::TRANCHES_MAX)];
    let etapes = preparer(pool, d.id, plan, pages).await?;
    let r = reglages::lire(pool).await?;
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        return reporter(pool, d, "Claude n’est pas connecté.").await;
    };
    // Le moins cher qui suffit : le réglage de la lecture (haiku par défaut) ; celui du nouvel
    // essai pour une relecture en détail demandée par un humain.
    let tache = if d.voie == Voie::Scan {
        reglages::OCR_SCAN
    } else {
        reglages::EXTRACTION_TEXTE
    };
    let (tache, reglage) = if approfondie {
        (reglages::NOUVEL_ESSAI, r.nouvel_essai.clone())
    } else {
        (tache, r.pour(tache).clone())
    };
    let reglage_fort = r.nouvel_essai.clone();
    // PDF joint dès le premier appel pour un scan lu par l'OCR (le fichier fait foi) et pour une
    // relecture en détail ; sinon seulement aux nouveaux essais.
    let joindre = d.ocr.is_some() || approfondie;
    let mut fichier = Fichier {
        contenu: d.contenu,
        dossier: None,
    };
    let mail = ContexteMail {
        expediteur: &e.expediteur,
        objet: &e.objet,
        corps: &e.corps,
        nom_fichier: &e.nom,
    };

    // 1. En-tête et totaux (gardés d'une tâche à l'autre).
    let entete = etapes.iter().find(|x| x.rang == 0);
    let mut prompt_entete =
        extraction::prompt(&mail, soc, &Contenu::Texte(&tr::texte_entete(&d.textes)));
    prompt_entete.push_str(&super::exemples_pour(pool, d.id, &e.expediteur, Some(d.texte)).await);
    let (sortie_entete, mut modele_entete) = match entete {
        Some(x) if x.etat == "lue" && x.reponse.is_some() => (
            x.reponse.clone().unwrap_or_default(),
            x.modele.clone().unwrap_or_else(|| reglage.modele.clone()),
        ),
        _ => {
            if let Some(raison) = indisponible(pool, &r, hors_quota).await? {
                return reporter(pool, d, &raison).await;
            }
            let dossier = match joindre {
                true => Some(fichier.chemin().await?),
                false => None,
            };
            let mut p = prompt_entete.clone();
            p.push_str(&tr::consigne_entete(
                pages,
                dossier.as_ref().map(|_| FICHIER),
            ));
            let premiere = appeler(
                ctx,
                d.id,
                tache,
                &reglage,
                extraction::PROMPT_SYSTEME,
                &tr::schema_entete(),
                &p,
                dossier.as_deref(),
                &acces,
            )
            .await;
            let lue = match premiere {
                Ok(j) if extraction::depuis_reponse(&j, soc).is_ok() => {
                    Some((j, reglage.modele.clone()))
                }
                Ok(_) | Err(ErreurClaude::JsonInvalide) => None,
                Err(err) => return panne(pool, d, &err).await,
            };
            let lue = match lue {
                Some(l) => l,
                None => {
                    // Réponse inexploitable : nouvel essai, modèle plus fort et PDF joint.
                    if let Some(raison) = indisponible(pool, &r, hors_quota).await? {
                        return reporter(pool, d, &raison).await;
                    }
                    let dossier = fichier.chemin().await?;
                    let mut p = prompt_entete.clone();
                    p.push_str(&tr::consigne_entete(pages, Some(FICHIER)));
                    match appeler(
                        ctx,
                        d.id,
                        reglages::NOUVEL_ESSAI,
                        &reglage_fort,
                        extraction::PROMPT_SYSTEME,
                        &tr::schema_entete(),
                        &p,
                        Some(&dossier),
                        &acces,
                    )
                    .await
                    {
                        Ok(j) if extraction::depuis_reponse(&j, soc).is_ok() => {
                            (j, reglage_fort.modele.clone())
                        }
                        Ok(_) | Err(ErreurClaude::JsonInvalide) => {
                            effacer(pool, d.id).await?;
                            return Ok(Some(echec(
                                d.voie,
                                "Claude n’a pas réussi à lire l’en-tête de ce long document, même \
                                 au second essai.",
                            )));
                        }
                        Err(err) => return panne(pool, d, &err).await,
                    }
                }
            };
            let structure = claude::sortie_structuree(&lue.0).unwrap_or_default();
            noter(pool, d.id, 0, "lue", Some(&structure), &lue.1, None).await?;
            (structure, lue.1)
        }
    };
    let enveloppe = |s: &Value| json!({ "structured_output": s });
    let mut ex = extraction::depuis_reponse(&enveloppe(&sortie_entete), soc)
        .map_err(|err| anyhow::anyhow!("en-tête gardé illisible : {err}"))?;
    // Ni facture, ni avoir, ni BL (tarif, catalogue, relevé) : les lignes ne servent pas.
    if !matches!(ex.r#type.as_str(), "facture" | "avoir" | "bl") {
        effacer(pool, d.id).await?;
        return Ok(Some(finir(
            d,
            ex,
            soc,
            &modele_entete,
            &reglage,
            None,
            Vec::new(),
        )));
    }

    // 2. Lignes, tranche par tranche, l'une après l'autre.
    let type_doc = Some(ex.r#type.clone());
    let fournisseur = ex.fournisseur.nom.clone();
    for t in plan {
        let deja = etapes
            .iter()
            .find(|x| x.rang == t.rang as i32)
            .is_some_and(|x| x.etat == "lue");
        if deja {
            continue;
        }
        if let Some(raison) = indisponible(pool, &r, hors_quota).await? {
            return reporter(pool, d, &raison).await;
        }
        let rang = t.rang as i32;
        let mut erreur = String::new();
        let mut lue = None;
        // Premier essai au réglage de la lecture (PDF joint pour un scan), puis nouvel essai
        // de la tranche seule, modèle plus fort et PDF joint.
        for essai in 0..2 {
            if essai == 1
                && let Some(raison) = indisponible(pool, &r, hors_quota).await?
            {
                return reporter(pool, d, &raison).await;
            }
            let reglage_t = if essai == 0 { &reglage } else { &reglage_fort };
            let dossier = if essai == 0 && !joindre {
                None
            } else {
                Some(fichier.chemin().await?)
            };
            let prompt = tr::prompt_tranche(
                &d.textes,
                t,
                type_doc.as_deref(),
                fournisseur.as_deref(),
                dossier.as_ref().map(|_| FICHIER),
            );
            match appeler(
                ctx,
                d.id,
                reglages::LIGNES_TRANCHE,
                reglage_t,
                tr::PROMPT_SYSTEME,
                &tr::schema_lignes(),
                &prompt,
                dossier.as_deref(),
                &acces,
            )
            .await
            {
                Ok(j) => match tr::lignes_de_reponse(&j) {
                    Some(l) if !vide_suspecte(&l, &d.textes, t) => {
                        lue = Some((l, reglage_t.modele.clone()));
                        break;
                    }
                    Some(_) => erreur = "aucune ligne lue sur des pages chiffrées".into(),
                    None => erreur = "réponse sans lignes conformes au schéma".into(),
                },
                Err(err) if claude::refus(&err).is_some() => {
                    // Limite d'usage, jeton refusé : reprise plus tard, sans perdre l'avancement.
                    let raison = super::message_echec(pool, &err).await;
                    return reporter(pool, d, &raison).await;
                }
                Err(err) => erreur = err.to_string(),
            }
            tracing::info!(document = d.id, tranche = rang, essai, erreur = %erreur, "tranche de lignes à relire");
        }
        match lue {
            Some((l, modele)) => {
                noter(
                    pool,
                    d.id,
                    rang,
                    "lue",
                    Some(&json!({ "lignes": l })),
                    &modele,
                    None,
                )
                .await?
            }
            None => {
                tracing::warn!(document = d.id, tranche = rang, erreur = %erreur, "tranche de lignes non lue");
                noter(
                    pool,
                    d.id,
                    rang,
                    "echec",
                    None,
                    &reglage_fort.modele,
                    Some(&erreur),
                )
                .await?
            }
        }
    }

    // 3. Fusion et contrôle par le code.
    let etapes = preparer(pool, d.id, plan, pages).await?;
    let mut parties: Vec<Vec<extraction::Ligne>> = Vec::new();
    let mut non_lues: Vec<usize> = Vec::new();
    for x in etapes.iter().filter(|x| x.rang > 0) {
        let lignes = (x.etat == "lue")
            .then_some(x.reponse.as_ref())
            .flatten()
            .and_then(|v| tr::lignes_de_reponse(&enveloppe(v)));
        match lignes {
            Some(l) => parties.push(l),
            None => non_lues.extend(x.debut as usize..=x.fin as usize),
        }
    }
    if let Some((a, b)) = hors_plafond {
        non_lues.extend(a..=b);
    }
    let (lignes, doublons) = tr::fusionner(&parties);
    let annonces: Annonces = tr::annonces(&enveloppe(&sortie_entete));
    let controle_de = |ex: &Extraction, lignes: &[extraction::Ligne]| {
        tr::controler(
            lignes,
            tr::Totaux {
                ht: ex.montant_ht,
                annexes_ht: ex.annexes_ht,
                remise_ht: ex.remise_ht,
            },
            annonces,
            &non_lues,
        )
    };
    ex.lignes = lignes.clone();
    let mut an = extraction::anomalies(&ex, Some(d.texte));
    let mut controle = controle_de(&ex, &lignes);

    // En-tête incohérent (TTC absent, HT + TVA ≠ TTC, montant absent du texte) ou totaux qui
    // ne tombent pas juste avec des lignes complètes : l'en-tête est relu une fois, modèle plus
    // fort et PDF joint. La lecture qui a le moins d'anomalies est gardée.
    let somme_fausse = non_lues.is_empty() && !controle.complet;
    if (!an.is_empty() || somme_fausse) && modele_entete != reglage_fort.modele {
        if let Some(raison) = indisponible(pool, &r, hors_quota).await? {
            return reporter(pool, d, &raison).await;
        }
        let dossier = fichier.chemin().await?;
        let mut p = prompt_entete.clone();
        let mut motifs = an.clone();
        motifs.extend(controle.raisons.iter().cloned());
        p.push_str(&extraction::consigne_nouvel_essai(&motifs, None, None));
        p.push_str(&tr::consigne_entete(pages, Some(FICHIER)));
        match appeler(
            ctx,
            d.id,
            reglages::NOUVEL_ESSAI,
            &reglage_fort,
            extraction::PROMPT_SYSTEME,
            &tr::schema_entete(),
            &p,
            Some(&dossier),
            &acces,
        )
        .await
        {
            Ok(j) => {
                if let Ok(mut ex2) = extraction::depuis_reponse(&j, soc) {
                    ex2.lignes = lignes.clone();
                    let an2 = extraction::anomalies(&ex2, Some(d.texte));
                    let c2 = controle_de(&ex2, &lignes);
                    let score = |a: &[String], c: &tr::Controle| a.len() + c.raisons.len();
                    if score(&an2, &c2) < score(&an, &controle) {
                        (ex, an, controle) = (ex2, an2, c2);
                        modele_entete = reglage_fort.modele.clone();
                    }
                }
            }
            Err(err) if claude::refus(&err).is_some() => {
                let raison = super::message_echec(pool, &err).await;
                return reporter(pool, d, &raison).await;
            }
            Err(err) => {
                tracing::warn!(document = d.id, erreur = %err, "nouvel essai de l'en-tête impossible");
            }
        }
    }
    ex.lignes_tronquees = !controle.complet;
    an.extend(tr::phrase(etapes.len(), &controle));
    let bilan = json!({
        "parties": etapes.len(),
        "pages": pages,
        "lues": parties.len(),
        "pages_non_lues": non_lues,
        "doublons_retires": doublons,
        "complet": controle.complet,
        "verifie": controle.verifie,
        "raisons": controle.raisons,
    });
    effacer(pool, d.id).await?;
    Ok(Some(finir(
        d,
        ex,
        soc,
        &modele_entete,
        &reglage,
        Some(bilan),
        an,
    )))
}

/// Réponse vide alors que les pages ont des lignes chiffrées : sans doute une lecture ratée.
fn vide_suspecte(lignes: &[extraction::Ligne], textes: &[String], t: &Tranche) -> bool {
    if lignes.iter().any(|l| !l.vide()) {
        return false;
    }
    let chiffrees: usize = (t.debut..=t.fin)
        .filter_map(|p| textes.get(p - 1))
        .map(|s| {
            s.lines()
                .filter(|l| l.chars().filter(char::is_ascii_digit).count() >= 3)
                .count()
        })
        .sum();
    chiffrees >= 5
}

/// Panne technique de l'appel d'en-tête : même traitement qu'une lecture en un appel
/// (`apres_panne` : refus = attente, panne = un essai de plus, erreur au troisième).
async fn panne(pool: &PgPool, d: &Doc<'_>, err: &ErreurClaude) -> anyhow::Result<Option<Lu>> {
    match super::apres_panne(pool, d.id, d.voie, err).await? {
        Lecture::Echec(lu) => {
            effacer(pool, d.id).await?;
            Ok(Some(lu))
        }
        _ => Ok(None),
    }
}

fn echec(voie: Voie, message: &str) -> Lu {
    Lu {
        etat: "erreur",
        voie: Some(voie.code()),
        message: Some(message.into()),
        confiance: json!({}),
        ..Default::default()
    }
}

/// Lecture prête à enregistrer, avec le bilan des tranches dans `confiance`.
fn finir(
    d: &Doc<'_>,
    ex: Extraction,
    soc: &[SocieteRef],
    modele: &str,
    reglage: &reglages::Reglage,
    bilan: Option<Value>,
    anomalies: Vec<String>,
) -> Lu {
    let societe_code = societe::dans_texte(d.texte, soc);
    let reglage = reglages::Reglage {
        modele: modele.to_string(),
        effort: reglage.effort.clone(),
    };
    let mut lu = super::lu_depuis_extraction(
        d.voie,
        ex,
        societe_code,
        Some(d.texte.to_string()),
        &reglage,
    );
    if let Some(c) = lu.confiance.as_object_mut() {
        if let Some(t) = d.ocr
            && let Some(m) = t.confiance().as_object()
        {
            c.extend(m.clone());
        }
        if let Some(b) = bilan {
            c.insert(tr::CLE_CONFIANCE.into(), b);
        }
    }
    super::avec_anomalies(lu, anomalies)
}
