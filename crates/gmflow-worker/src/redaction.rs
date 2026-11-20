//! Brouillon de réponse, généré à la demande (tâche `GenererBrouillon`). Claude reçoit le mail
//! et, pour le ton, les dernières réponses envoyées par la même boîte : celles de GMFlow, plus
//! celles du dossier des envoyés quand il est renseigné. Rien n'est envoyé ici.

use gmflow_core::claude::{self, Appel};
use gmflow_core::{imap, reglages, tri};
use mail_parser::MessageParser;
use serde_json::{Value, json};

use crate::Contexte;

/// Exemples de ton : au plus ce nombre de réponses, chacune tronquée.
const EXEMPLES: usize = 5;
const EXEMPLE_MAX: usize = 1200;
const CORPS_MAX: usize = 6000;

pub const PROMPT_SYSTEME: &str = "Tu rédiges des réponses à des mails pour un commerce. Tu \
réponds uniquement avec la sortie structurée demandée. Le champ « texte » contient seulement le \
texte de la réponse, en français : pas d'objet, pas de citation du message reçu. Tu reprends le \
ton des réponses déjà envoyées données en exemple (formules d'appel et de politesse, vouvoiement \
ou tutoiement, signature). Tu n'inventes aucun fait, aucun prix, aucune date et aucun \
engagement : quand une information manque, écris [à compléter].";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": { "texte": { "type": "string" } },
        "required": ["texte"],
        "additionalProperties": false
    })
}

struct Demande {
    boite_id: i64,
    expediteur: String,
    objet: String,
    corps_texte: String,
    raison_sociale: String,
    identifiant: String,
    serveur: String,
    port: i32,
    mot_de_passe_chiffre: Vec<u8>,
    dossier_envoyes: Option<String>,
}

/// Réponses récentes du dossier des envoyés (texte sans historique). Erreurs ignorées : les
/// exemples ne sont qu'un plus.
async fn exemples_imap(ctx: &Contexte, d: &Demande, dossier: &str) -> Vec<String> {
    let essai = async {
        let mdp = ctx.chiffreur.dechiffrer_texte(&d.mot_de_passe_chiffre)?;
        let port = u16::try_from(d.port)?;
        let mut s = imap::connecter(&d.serveur, port, &d.identifiant, &mdp, ctx.transport).await?;
        let bruts = imap::derniers_messages(&mut s, dossier, EXEMPLES as u32).await;
        let _ = s.logout().await;
        anyhow::Ok(bruts?)
    };
    match essai.await {
        Ok(bruts) => bruts
            .iter()
            .rev()
            .filter_map(|b| {
                MessageParser::default()
                    .parse(b)
                    .and_then(|m| m.body_text(0).map(|t| t.into_owned()))
            })
            .map(|t| tri::sans_citation(&t))
            .filter(|t| !t.is_empty())
            .collect(),
        Err(e) => {
            tracing::warn!(boite = d.boite_id, erreur = %e, "exemples du dossier des envoyés illisibles");
            Vec::new()
        }
    }
}

pub fn prompt(
    raison_sociale: &str,
    adresse: &str,
    exemples: &[String],
    expediteur: &str,
    objet: &str,
    corps: &str,
) -> String {
    let mut p = format!("Tu réponds au nom de {raison_sociale}, depuis l'adresse {adresse}.\n\n");
    if exemples.is_empty() {
        p.push_str(
            "Aucune réponse précédente en exemple : ton professionnel et cordial, vouvoiement.\n\n",
        );
    } else {
        p.push_str("Réponses déjà envoyées depuis cette adresse (exemples de ton) :\n");
        for (i, e) in exemples.iter().enumerate() {
            p.push_str(&format!(
                "--- Exemple {} ---\n{}\n",
                i + 1,
                tri::tronquer(e, EXEMPLE_MAX)
            ));
        }
        p.push('\n');
    }
    let corps = tri::sans_citation(corps);
    p.push_str(&format!(
        "Mail auquel répondre :\nDe : {expediteur}\nObjet : {objet}\n<<<\n{}\n>>>\n",
        tri::tronquer(&corps, CORPS_MAX)
    ));
    p
}

async fn noter(
    ctx: &Contexte,
    id: i64,
    texte: Option<&str>,
    reglage: Option<&reglages::Reglage>,
    erreur: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "update brouillon_reponse set
            texte = coalesce($2, texte), statut = 'propose', modele = coalesce($3, modele),
            effort = coalesce($4, effort), erreur = $5, modifie_le = now()
         where id = $1 and statut = 'generation'",
        id,
        texte,
        reglage.map(|r| r.modele.as_str()),
        reglage.map(|r| r.effort.as_str()),
        erreur
    )
    .execute(&ctx.pool)
    .await?;
    Ok(())
}

/// Tâche `GenererBrouillon`.
pub async fn generer(ctx: &Contexte, brouillon_id: i64) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some(d) = sqlx::query_as!(
        Demande,
        "select b.boite_id, m.expediteur, m.objet, m.corps_texte, s.raison_sociale,
                bm.identifiant, bm.serveur, bm.port, bm.mot_de_passe_chiffre, bm.dossier_envoyes
         from brouillon_reponse b
         join mail m on m.id = b.mail_id
         join boite_mail bm on bm.id = b.boite_id
         join societe s on s.id = bm.societe_id
         where b.id = $1 and b.statut = 'generation'",
        brouillon_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        return noter(ctx, brouillon_id, None, None, Some("jeton Claude absent")).await;
    };
    let mut exemples: Vec<String> = sqlx::query_scalar!(
        "select texte from brouillon_reponse
         where boite_id = $1 and statut = 'envoye' and texte <> ''
         order by envoye_le desc limit $2",
        d.boite_id,
        EXEMPLES as i64
    )
    .fetch_all(pool)
    .await?;
    if let Some(dossier) = d.dossier_envoyes.as_deref().filter(|s| !s.is_empty()) {
        let manque = EXEMPLES.saturating_sub(exemples.len()).max(2);
        exemples.extend(
            exemples_imap(ctx, &d, dossier)
                .await
                .into_iter()
                .take(manque),
        );
    }
    let prompt = prompt(
        &d.raison_sociale,
        &d.identifiant,
        &exemples,
        &d.expediteur,
        &d.objet,
        &d.corps_texte,
    );
    let reglage = reglages::lire(pool).await?.brouillon_reponse;
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
        reglages::BROUILLON_REPONSE,
        &reglage.modele,
        &prompt,
        &acces,
    );
    a.effort = Some(&reglage.effort);
    a.args_en_plus = &args;
    match claude::lancer(pool, &ctx.config.claude_bin, &a).await {
        Ok(sortie) => {
            let texte = claude::sortie_structuree(&sortie.json)
                .and_then(|v| v.get("texte").and_then(Value::as_str).map(str::to_string))
                .or_else(|| {
                    sortie
                        .json
                        .get("result")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
            match texte {
                Some(t) => {
                    crate::lecture::noter_utilisation(pool).await;
                    noter(ctx, brouillon_id, Some(&t), Some(&reglage), None).await
                }
                None => {
                    noter(
                        ctx,
                        brouillon_id,
                        None,
                        None,
                        Some("réponse de Claude vide"),
                    )
                    .await
                }
            }
        }
        Err(e) => {
            noter(
                ctx,
                brouillon_id,
                None,
                None,
                Some(&format!("Claude : {e}")),
            )
            .await
        }
    }
}
