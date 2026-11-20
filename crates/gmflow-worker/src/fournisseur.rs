//! Rattachement d'un document lu à sa fiche fournisseur, et exemples de corrections passées
//! donnés à Claude pour ce fournisseur.

use gmflow_core::claude::{self, Appel};
use gmflow_core::fournisseurs::{self, Candidat, Identification, Indices};
use gmflow_core::{journal, reglages};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::Contexte;

/// Corrections récentes montrées à Claude pour un fournisseur.
const EXEMPLES_MAX: i64 = 3;

const PROMPT_SYSTEME: &str = "Tu rapproches le fournisseur d'un document des fiches d'un \
référentiel. Tu réponds uniquement avec la sortie structurée demandée. fournisseur_id est l'id \
de la fiche qui désigne la même entreprise (même société, autre écriture du nom), ou null si \
aucune ne correspond. Dans le doute, null.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": { "fournisseur_id": { "type": ["integer", "null"] } },
        "required": ["fournisseur_id"],
        "additionalProperties": false
    })
}

/// Fiche probable du fournisseur avant lecture : par le domaine de l'expéditeur, sinon par un
/// SIREN ou un n° de TVA connu présent dans le texte.
pub async fn pre_identifier(
    pool: &PgPool,
    expediteur: &str,
    texte: Option<&str>,
) -> sqlx::Result<Option<i64>> {
    let exclus = fournisseurs::domaines_exclus(pool).await?;
    if let Some(d) = fournisseurs::domaine_utile(expediteur, &exclus)
        && let Some(id) = fournisseurs::par_domaine(pool, &d).await?
    {
        return Ok(Some(id));
    }
    match texte {
        Some(t) => fournisseurs::dans_texte(pool, t).await,
        None => Ok(None),
    }
}

/// Bloc de prompt : les dernières corrections humaines sur des documents de ce fournisseur.
pub async fn exemples(pool: &PgPool, fournisseur_id: i64) -> sqlx::Result<String> {
    let lignes = sqlx::query!(
        "select d.corrections, p.nom from document d join piece_jointe p on p.id = d.piece_jointe_id
         where d.fournisseur_id = $1 and d.corrige_le is not null
         order by d.corrige_le desc limit $2",
        fournisseur_id,
        EXEMPLES_MAX
    )
    .fetch_all(pool)
    .await?;
    let mut s = String::new();
    for l in lignes {
        let Some(champs) = l.corrections.as_object() else {
            continue;
        };
        let details: Vec<String> = champs
            .iter()
            .filter(|(champ, _)| champ.as_str() != "fournisseur_id")
            .map(|(champ, v)| format!("{champ} lu {} corrigé en {}", v["avant"], v["apres"]))
            .collect();
        if !details.is_empty() {
            s.push_str(&format!("- {} : {}\n", l.nom, details.join(" ; ")));
        }
    }
    if s.is_empty() {
        return Ok(s);
    }
    Ok(format!(
        "\nCorrections faites à la main sur des documents récents de ce même fournisseur (à \
         prendre en compte pour ce document) :\n{s}"
    ))
}

/// Claude choisit parmi les candidates. Rien si Claude est indisponible ou hésite.
async fn choisir(
    ctx: &Contexte,
    document_id: i64,
    ind: &Indices,
    expediteur: &str,
    candidats: &[Candidat],
) -> anyhow::Result<Option<i64>> {
    let pool = &ctx.pool;
    let r = reglages::lire(pool).await?;
    if crate::lecture::claude_indisponible(pool, &r)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let Some(acces) = gmflow_core::claude::acces(pool, &ctx.chiffreur).await? else {
        return Ok(None);
    };
    let mut prompt = format!(
        "Fournisseur du document : nom {} ; SIRET {} ; TVA {} ; expéditeur du mail : {}\n\nFiches existantes :\n",
        ind.nom.as_deref().unwrap_or("?"),
        ind.siret.as_deref().unwrap_or("?"),
        ind.tva.as_deref().unwrap_or("?"),
        expediteur
    );
    for c in candidats {
        prompt.push_str(&format!(
            "- {} : {} ; SIRET {} ; TVA {} ; domaines {}\n",
            c.id,
            c.nom,
            c.siret.as_deref().unwrap_or("?"),
            c.tva.as_deref().unwrap_or("?"),
            if c.domaines.is_empty() {
                "?".to_string()
            } else {
                c.domaines.join(", ")
            }
        ));
    }
    let reglage = r.pour(reglages::IDENTIFICATION_FOURNISSEUR).clone();
    let schema = schema().to_string();
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
    let mut a = Appel::avec_acces(
        reglages::IDENTIFICATION_FOURNISSEUR,
        &reglage.modele,
        &prompt,
        &acces,
    );
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    let sortie = match claude::lancer(pool, &ctx.config.claude_bin, &a).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(document = document_id, erreur = %e, "identification du fournisseur par Claude impossible");
            return Ok(None);
        }
    };
    let valeur = match sortie.json.get("structured_output") {
        Some(v) if v.is_object() => v.clone(),
        _ => sortie
            .json
            .get("result")
            .and_then(Value::as_str)
            .and_then(|r| serde_json::from_str(r.trim()).ok())
            .unwrap_or(Value::Null),
    };
    let choisi = valeur.get("fournisseur_id").and_then(Value::as_i64);
    Ok(choisi.filter(|id| candidats.iter().any(|c| c.id == *id)))
}

/// Rattache un document lu à une fiche fournisseur (trouvée, choisie par Claude ou créée « à
/// confirmer »). Applique la structure par défaut du fournisseur si aucune n'est reconnue.
pub async fn rattacher(ctx: &Contexte, document_id: i64) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some(d) = sqlx::query!(
        r#"select d.fournisseur_nom, d.fournisseur_siret, d.fournisseur_tva, d.societe_id,
                  d.corrections ? 'fournisseur_id' as "fixe!", m.expediteur
           from document d join mail m on m.id = d.mail_id
           where d.id = $1 and d.etat = 'lu'"#,
        document_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    if d.fixe {
        return Ok(());
    }
    let exclus = fournisseurs::domaines_exclus(pool).await?;
    let ind = Indices {
        nom: d.fournisseur_nom.clone(),
        siret: d.fournisseur_siret.clone(),
        tva: d.fournisseur_tva.clone(),
        domaine: fournisseurs::domaine_utile(&d.expediteur, &exclus),
    };
    let (id, comment) = match fournisseurs::identifier(pool, &ind).await? {
        Identification::Trouve { id, par } => {
            fournisseurs::apprendre(pool, id, &ind, par == "identifiant").await?;
            (Some(id), par)
        }
        Identification::Candidats(c) => {
            match choisir(ctx, document_id, &ind, &d.expediteur, &c).await? {
                Some(id) => {
                    fournisseurs::apprendre(pool, id, &ind, false).await?;
                    (Some(id), "claude")
                }
                None => (fournisseurs::creer(pool, &ind).await?, "nouvelle fiche"),
            }
        }
        Identification::Aucun => (fournisseurs::creer(pool, &ind).await?, "nouvelle fiche"),
    };
    let Some(id) = id else {
        return Ok(());
    };
    let defaut = sqlx::query_scalar!(
        "select societe_defaut_id from fournisseur where id = $1",
        id
    )
    .fetch_one(pool)
    .await?;
    let societe_regle = d.societe_id.is_none() && defaut.is_some();
    sqlx::query!(
        "update document set fournisseur_id = $2,
            societe_id = case when $3 then $4 else societe_id end,
            confiance = case when $3 then confiance || '{\"societe\": 0.9, \"societe_par_regle\": true}'::jsonb
                             else confiance end
         where id = $1",
        document_id,
        id,
        societe_regle,
        defaut
    )
    .execute(pool)
    .await?;
    journal::ecrire(
        pool,
        None,
        "rattachement_fournisseur",
        "document",
        Some(document_id.to_string()),
        json!({ "fournisseur_id": id, "par": comment, "societe_par_regle": societe_regle }),
    )
    .await?;
    Ok(())
}
