//! Tableurs : lignes d'articles lues dans les cellules (`gmflow_core::lecture::tableau_articles`),
//! correspondance des colonnes demandée une fois à Claude quand l'en-tête n'est pas reconnu,
//! puis gardée sur la fiche fournisseur.

use gmflow_core::claude::{self, Appel};
use gmflow_core::lecture::tableau_articles::{self as ta, Apprises, TableauLu};
use gmflow_core::{attente, lecture as lect, reglages};
use serde_json::Value;
use sqlx::PgPool;

use crate::Contexte;

const PROMPT_SYSTEME: &str = "Tu repères le tableau des articles d'une feuille de tableur et \
la colonne de chaque champ. Tu réponds uniquement avec la sortie structurée demandée. Tu \
n'inventes rien : null pour un champ sans colonne, ligne_entete null s'il n'y a pas de tableau \
d'articles.";

/// Lignes d'un tableur lues dans les cellules, et le texte complet des feuilles. Rien si le
/// fichier n'est pas un tableur, si aucun tableau d'articles n'est trouvé ou si la
/// correspondance de Claude ne tient pas : la voie texte reprend alors, inchangée.
pub(crate) async fn preparer(
    ctx: &Contexte,
    document_id: i64,
    expediteur: &str,
    nom: &str,
    mime: &str,
    contenu: &[u8],
    hors_quota: bool,
) -> anyhow::Result<Option<(TableauLu, String)>> {
    let (nom, mime, octets) = (nom.to_string(), mime.to_string(), contenu.to_vec());
    let Some(grilles) =
        tokio::task::spawn_blocking(move || lect::grilles_tableur(&nom, &mime, &octets)).await?
    else {
        return Ok(None);
    };
    if grilles.is_empty() {
        return Ok(None);
    }
    let texte = ta::texte_complet(&grilles);
    let pool = &ctx.pool;
    let fournisseur = crate::fournisseur::pre_identifier(pool, expediteur, Some(&texte)).await?;
    let apprises = apprises(pool, &ta::empreintes(&grilles), fournisseur).await?;
    if let Some(lu) = ta::lire(&grilles, &apprises) {
        return Ok(Some((lu, texte)));
    }
    let Some(i) = ta::feuille_a_faire_designer(&grilles) else {
        return Ok(None);
    };
    let Some(valeur) = demander(ctx, document_id, &grilles[i], hors_quota).await? else {
        return Ok(None);
    };
    let lu = ta::depuis_reponse(&grilles[i], &valeur)
        .and_then(|c| ta::avec_correspondance(&grilles, i, c, &apprises));
    if lu.is_none() {
        tracing::info!(
            document = document_id,
            "correspondance des colonnes refusée : lecture du tableur en texte"
        );
    }
    Ok(lu.map(|lu| (lu, texte)))
}

/// Correspondances gardées sous ces empreintes : celles du fournisseur probable d'abord, puis
/// celles des autres fiches (mêmes intitulés à la même place : mêmes colonnes).
async fn apprises(
    pool: &PgPool,
    empreintes: &[String],
    fournisseur: Option<i64>,
) -> sqlx::Result<Apprises> {
    if empreintes.is_empty() {
        return Ok(Apprises::new());
    }
    let fiches = sqlx::query!(
        "select colonnes_tableur from fournisseur
         where colonnes_tableur ?| $1::text[]
         order by (id = $2) desc, id",
        empreintes,
        fournisseur
    )
    .fetch_all(pool)
    .await?;
    let mut v = Apprises::new();
    for f in fiches {
        let Some(o) = f.colonnes_tableur.as_object() else {
            continue;
        };
        for e in empreintes {
            if v.contains_key(e) {
                continue;
            }
            if let Some(cols) = o
                .get(e)
                .and_then(|c| serde_json::from_value(c.clone()).ok())
            {
                v.insert(e.clone(), cols);
            }
        }
    }
    Ok(v)
}

/// Pose la question des colonnes à Claude : la sortie structurée, ou rien (Claude
/// indisponible, en échec, réponse illisible).
async fn demander(
    ctx: &Contexte,
    document_id: i64,
    grille: &ta::Grille,
    hors_quota: bool,
) -> anyhow::Result<Option<Value>> {
    let pool = &ctx.pool;
    let r = reglages::lire(pool).await?;
    if attente::lecture_indisponible(pool, &r, hors_quota)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        return Ok(None);
    };
    let reglage = r.pour(reglages::COLONNES_TABLEUR).clone();
    let schema = ta::schema_correspondance().to_string();
    let args = [
        "--json-schema",
        &schema,
        "--system-prompt",
        PROMPT_SYSTEME,
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--tools",
        "",
    ];
    let prompt = ta::prompt_correspondance(grille);
    let mut a = Appel::avec_acces(reglages::COLONNES_TABLEUR, &reglage.modele, &prompt, &acces);
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    a.document_id = Some(document_id);
    match claude::lancer(pool, &ctx.config.claude_bin, &a).await {
        Ok(s) => Ok(claude::sortie_structuree(&s.json)),
        Err(e) => {
            tracing::warn!(document = document_id, erreur = %e, "correspondance des colonnes par Claude impossible");
            Ok(None)
        }
    }
}

/// Après le rattachement du document à sa fiche fournisseur : la correspondance donnée par
/// Claude (`document.resultat`) est gardée sur la fiche, sous l'empreinte de l'en-tête.
pub(crate) async fn garder_correspondance(pool: &PgPool, document_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "update fournisseur f
         set colonnes_tableur = f.colonnes_tableur
             || jsonb_build_object(d.resultat #>> '{colonnes_tableur,empreinte}',
                                   d.resultat #> '{colonnes_tableur,colonnes}')
         from document d
         where d.id = $1 and f.id = d.fournisseur_id
           and d.resultat #>> '{colonnes_tableur,empreinte}' is not null
           and jsonb_typeof(d.resultat #> '{colonnes_tableur,colonnes}') = 'object'",
        document_id
    )
    .execute(pool)
    .await?;
    Ok(())
}
