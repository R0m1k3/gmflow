//! Plusieurs pièces dans un même PDF (L19) : découpage décidé par le code
//! (`gmflow_core::lecture::decoupage::analyser`) ou, en cas de doute, par un petit appel à
//! Claude ; chaque pièce devient une pièce jointe « partie » (`piece_jointe.source_id`) avec son
//! document, lu par la chaîne normale.

use gmflow_core::claude::{self, Appel, DossierTemp};
use gmflow_core::lecture::decoupage::{self as dc, Plage, Verdict};
use gmflow_core::{attente, eml, file, reglages};
use sqlx::PgPool;

use crate::Contexte;

/// Pages au plus d'un scan sans texte soumis à Claude pour le découpage.
const PAGES_MAX_SANS_TEXTE: usize = 20;

/// Pièce jointe d'origine à examiner.
pub(crate) struct Origine<'a> {
    pub document_id: i64,
    pub piece_id: i64,
    pub mail_id: i64,
    pub rang: i32,
    pub nom: &'a str,
    pub contenu: &'a [u8],
    /// Texte de chaque page (couche texte ou OCR), s'il y en a.
    pub textes: Option<Vec<String>>,
    pub hors_quota: bool,
}

/// Plages des pièces si le fichier en regroupe plusieurs (parties créées et mises en lecture,
/// ou déjà créées par une lecture précédente) ; rien sinon : lecture normale. Un document déjà
/// validé ou déposé n'est jamais découpé.
pub(crate) async fn decouper(ctx: &Contexte, o: Origine<'_>) -> anyhow::Result<Option<Vec<Plage>>> {
    let pool = &ctx.pool;
    let libre = sqlx::query_scalar!(
        r#"select d.valide_le is null
                  and not exists (select 1 from envoi_depot e
                                  where e.document_id = d.id and e.etat = 'envoye') as "libre!"
           from document d where d.id = $1"#,
        o.document_id
    )
    .fetch_one(pool)
    .await?;
    if !libre {
        return Ok(None);
    }
    // Déjà découpé (relecture) : mêmes parties, rien de recréé.
    let deja = sqlx::query!(
        r#"select page_debut as "debut!", page_fin as "fin!" from piece_jointe
           where source_id = $1 and page_debut is not null and page_fin is not null
           order by page_debut"#,
        o.piece_id
    )
    .fetch_all(pool)
    .await?;
    if !deja.is_empty() {
        return Ok(Some(
            deja.into_iter()
                .map(|p| Plage {
                    debut: p.debut as usize,
                    fin: p.fin as usize,
                    r#type: None,
                })
                .collect(),
        ));
    }
    let pages = gmflow_core::lecture::pdf::pages(o.contenu);
    if !(2..=dc::PAGES_MAX).contains(&pages) {
        return Ok(None);
    }
    let textes = o
        .textes
        .clone()
        .filter(|t| t.len() == pages && t.iter().any(|p| !p.trim().is_empty()));
    let verdict = match &textes {
        Some(t) => match dc::analyser(t) {
            Verdict::Doute => demander(ctx, &o, pages, Some(t)).await?,
            v => v,
        },
        None if pages <= PAGES_MAX_SANS_TEXTE => demander(ctx, &o, pages, None).await?,
        None => Verdict::Une,
    };
    let Verdict::Plusieurs(plages) = verdict else {
        return Ok(None);
    };
    let (pdf, p2) = (o.contenu.to_vec(), plages.clone());
    let fichiers = tokio::task::spawn_blocking(move || {
        p2.iter()
            .map(|p| dc::extraire_pages(&pdf, p.debut, p.fin))
            .collect::<Option<Vec<Vec<u8>>>>()
    })
    .await?;
    let Some(fichiers) = fichiers else {
        tracing::warn!(
            document = o.document_id,
            "découpage du PDF impossible : lu en entier"
        );
        return Ok(None);
    };
    let documents = creer_parties(pool, &o, &plages, &fichiers).await?;
    for d in documents {
        file::pousser_lecture_auto(pool, d, file::PRIORITE_RECENT).await?;
    }
    tracing::info!(
        document = o.document_id,
        pieces = plages.len(),
        "PDF découpé en plusieurs pièces"
    );
    Ok(Some(plages))
}

/// Pièces jointes « parties » et leurs documents (à lire). Rend les documents créés.
async fn creer_parties(
    pool: &PgPool,
    o: &Origine<'_>,
    plages: &[Plage],
    fichiers: &[Vec<u8>],
) -> anyhow::Result<Vec<i64>> {
    let mut tx = pool.begin().await?;
    let mut documents = Vec::new();
    for (k, (p, f)) in plages.iter().zip(fichiers).enumerate() {
        let rang = -(o.rang * 100 + k as i32 + 1);
        let sha = eml::sha256(f);
        let Some(piece) = sqlx::query_scalar!(
            "insert into piece_jointe (mail_id, rang, nom, type_mime, taille, sha256, source_id,
                                       page_debut, page_fin)
             values ($1, $2, $3, 'application/pdf', $4, $5, $6, $7, $8)
             on conflict (mail_id, rang) do nothing
             returning id",
            o.mail_id,
            rang,
            dc::nom_partie(o.nom, p),
            f.len() as i64,
            &sha,
            o.piece_id,
            p.debut as i32,
            p.fin as i32
        )
        .fetch_optional(&mut *tx)
        .await?
        else {
            continue;
        };
        if let Some(d) = sqlx::query_scalar!(
            "insert into document (piece_jointe_id, mail_id, sha256) values ($1, $2, $3)
             on conflict (piece_jointe_id) do nothing returning id",
            piece,
            o.mail_id,
            &sha
        )
        .fetch_optional(&mut *tx)
        .await?
        {
            documents.push(d);
        }
    }
    tx.commit().await?;
    Ok(documents)
}

/// Demande le découpage à Claude (texte des pages, sinon le fichier) ; une seule pièce s'il est
/// indisponible, en échec ou si sa réponse ne tient pas.
async fn demander(
    ctx: &Contexte,
    o: &Origine<'_>,
    pages: usize,
    textes: Option<&Vec<String>>,
) -> anyhow::Result<Verdict> {
    let pool = &ctx.pool;
    let r = reglages::lire(pool).await?;
    if attente::lecture_indisponible(pool, &r, o.hors_quota)
        .await?
        .is_some()
    {
        return Ok(Verdict::Une);
    }
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        return Ok(Verdict::Une);
    };
    let reglage = r.pour(reglages::DECOUPAGE_PDF).clone();
    let schema = dc::schema().to_string();
    let mut args = vec![
        "--json-schema",
        &schema,
        "--system-prompt",
        dc::PROMPT_SYSTEME,
        "--strict-mcp-config",
        "--disable-slash-commands",
    ];
    let dossier;
    let prompt = match textes {
        Some(t) => {
            args.extend(["--tools", ""]);
            dossier = None;
            dc::prompt_textes(t)
        }
        None => {
            let d = DossierTemp::creer("gmflow-decoupage")?;
            tokio::fs::write(d.0.join("document.pdf"), o.contenu).await?;
            args.extend([
                "--tools",
                "Read",
                "--allowedTools",
                "Read(./**)",
                "--permission-mode",
                "dontAsk",
                "--settings",
                crate::lecture::REGLAGES_LECTURE,
            ]);
            dossier = Some(d);
            dc::prompt_fichier("document.pdf", pages)
        }
    };
    let mut a = Appel::avec_acces(reglages::DECOUPAGE_PDF, &reglage.modele, &prompt, &acces);
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    a.dossier = dossier.as_ref().map(|d| d.0.as_path());
    if dossier.is_some() {
        a.delai = claude::DELAI_LECTURE;
    }
    match claude::lancer(pool, &ctx.config.claude_bin, &a).await {
        Ok(s) => Ok(claude::sortie_structuree(&s.json)
            .map_or(Verdict::Une, |v| dc::depuis_reponse(&v, pages))),
        Err(e) => {
            tracing::warn!(document = o.document_id, erreur = %e, "découpage par Claude impossible : lu en entier");
            Ok(Verdict::Une)
        }
    }
}

/// Phrase du document d'origine découpé.
pub(crate) fn message(plages: &[Plage]) -> String {
    let liste: Vec<String> = plages
        .iter()
        .map(|p| {
            if p.debut == p.fin {
                format!("p. {}", p.debut)
            } else {
                format!("p. {}-{}", p.debut, p.fin)
            }
        })
        .collect();
    format!(
        "Ce fichier regroupe {} pièces, lues chacune à part ({}).",
        plages.len(),
        liste.join(", ")
    )
}
