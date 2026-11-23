//! Tri des mails (lot 5). Une pompe marque « à trier » les mails récents jamais triés et pousse
//! une tâche par mail ; la tâche applique les règles sans IA, puis appelle Claude seulement si
//! aucune règle ne suffit, dans la limite du quota de tri du jour.

use gmflow_core::claude::{self, Appel, ErreurClaude};
use gmflow_core::tri::{self, ContexteRegle, ErreurTri, Marque, PieceVue, Tri};
use gmflow_core::{file, reglages, stockage};
use sqlx::PgPool;

use crate::Contexte;

const PAR_TOUR: i64 = 200;

/// Claude peut-il trier maintenant (jeton présent, pas de refus en cours, quota de tri non
/// atteint) ? Sinon, pourquoi.
pub async fn claude_indisponible(
    pool: &PgPool,
    r: &reglages::Reglages,
) -> anyhow::Result<Option<String>> {
    Ok(gmflow_core::attente::tri_indisponible(pool, r)
        .await?
        .map(|i| i.message))
}

/// Remet en file les tris bloqués, marque les mails récents à trier et pousse les tâches, les
/// plus récents d'abord. Rend le nombre de tâches poussées.
pub async fn pompe(pool: &PgPool) -> anyhow::Result<usize> {
    sqlx::query!(
        "update mail set tri_etat = 'a_trier', tri_pris_le = null, tri_planifie_le = null
         where tri_etat = 'en_cours' and tri_pris_le < now() - interval '30 minutes'"
    )
    .execute(pool)
    .await?;
    let r = reglages::lire(pool).await?;
    if !r.tri_auto {
        return Ok(0);
    }
    sqlx::query!(
        "update mail set tri_etat = 'a_trier'
         where tri_etat is null and categorie is null
           and coalesce(date_mail, recu_le) >= now() - make_interval(days => $1)",
        r.tri_depuis_jours as i32
    )
    .execute(pool)
    .await?;
    // Sans Claude, seuls les mails jamais examinés passent : les règles peuvent suffire.
    let sans_claude = claude_indisponible(pool, &r).await?.is_some();
    let ids = sqlx::query_scalar!(
        "with c as (
            select id from mail
            where tri_etat = 'a_trier'
              and (tri_planifie_le is null or tri_planifie_le < now() - interval '30 minutes')
              and (not $1 or tri_message is null)
            order by coalesce(date_mail, recu_le) desc
            limit $2
            for update skip locked
         )
         update mail m set tri_planifie_le = now() from c where m.id = c.id
         returning m.id",
        sans_claude,
        PAR_TOUR
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        file::pousser_tri(pool, *id).await?;
    }
    Ok(ids.len())
}

pub async fn pompe_apres_releve(pool: &PgPool) {
    match pompe(pool).await {
        Ok(n) if n > 0 => tracing::info!(mails = n, "tris planifiés"),
        Ok(_) => {}
        Err(e) => tracing::error!(erreur = ?e, "pompe de tri en échec"),
    }
}

struct MailTri {
    id: i64,
    expediteur: String,
    objet: String,
    corps_texte: String,
    chemin_eml: String,
}

/// Tâche `TrierMail`.
pub async fn trier(ctx: &Contexte, mail_id: i64) -> anyhow::Result<()> {
    let Some(m) = sqlx::query_as!(
        MailTri,
        "update mail set tri_etat = 'en_cours', tri_pris_le = now()
         where id = $1 and tri_etat = 'a_trier'
         returning id, expediteur, objet, corps_texte, chemin_eml",
        mail_id
    )
    .fetch_optional(&ctx.pool)
    .await?
    else {
        return Ok(());
    };
    if let Err(e) = trier_pris(ctx, &m).await {
        tracing::error!(mail = m.id, erreur = ?e, "tri en échec");
        sqlx::query!(
            "update mail set tri_etat = 'erreur', tri_message = $2, tri_pris_le = null where id = $1",
            m.id,
            e.to_string()
        )
        .execute(&ctx.pool)
        .await?;
    }
    Ok(())
}

/// Écrit la catégorie, sauf si un utilisateur l'a déjà fixée.
pub async fn appliquer(
    pool: &PgPool,
    id: i64,
    categorie: &str,
    par: &str,
    resume: Option<&str>,
    priorite: Option<&str>,
    message: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "update mail set categorie = $2, categorie_par = $3, resume = $4, priorite = $5,
            tri_etat = 'trie', trie_le = now(), tri_message = $6, tri_pris_le = null,
            statut_reponse = case
                when $2 = 'question' then coalesce(statut_reponse, 'a_repondre')
                when statut_reponse = 'a_repondre' then null
                else statut_reponse end
         where id = $1 and categorie_par is distinct from 'humain'",
        id,
        categorie,
        par,
        resume,
        priorite,
        message
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn remettre(pool: &PgPool, id: i64, message: &str) -> anyhow::Result<()> {
    sqlx::query!(
        "update mail set tri_etat = 'a_trier', tri_message = $2, tri_pris_le = null where id = $1",
        id,
        message
    )
    .execute(pool)
    .await?;
    Ok(())
}

struct PieceTri {
    nom: String,
    type_mime: String,
    etat: Option<String>,
    type_document: Option<String>,
}

async fn trier_pris(ctx: &Contexte, m: &MailTri) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let brut = tokio::fs::read(stockage::chemin_absolu(&ctx.config.fichiers, &m.chemin_eml))
        .await
        .unwrap_or_default();
    let entetes = tri::entetes(&brut);
    let marques: Vec<Marque> = sqlx::query!("select adresse, domaine from expediteur_marque")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|r| match (r.adresse, r.domaine) {
            (Some(a), _) => Some(Marque::Adresse(a)),
            (None, Some(d)) => Some(Marque::Domaine(d)),
            _ => None,
        })
        .collect();
    let internes: Vec<String> = sqlx::query_scalar!(
        r#"select distinct lower(identifiant) as "a!" from boite_mail where identifiant like '%@%'"#
    )
    .fetch_all(pool)
    .await?;
    let pieces = sqlx::query_as!(
        PieceTri,
        r#"select p.nom, p.type_mime, d.etat as "etat?", d.type as "type_document?"
           from piece_jointe p left join document d on d.piece_jointe_id = p.id
           where p.mail_id = $1 order by p.rang"#,
        m.id
    )
    .fetch_all(pool)
    .await?;
    // Une pièce compte si elle n'est pas ignorée ; pas encore lue, une petite image (logo,
    // signature) ne compte pas.
    let pieces_utiles = pieces
        .iter()
        .filter(|p| match p.etat.as_deref() {
            Some("ignore") => false,
            Some(_) => true,
            None => !p.type_mime.starts_with("image/"),
        })
        .count();
    let contexte = ContexteRegle {
        expediteur: &m.expediteur,
        entetes: &entetes,
        marques: &marques,
        adresses_internes: &internes,
        corps: &m.corps_texte,
        pieces_utiles,
    };
    // Ce que l'équipe a décidé pour cet expéditeur passe avant les règles générales.
    let dernieres: Vec<String> = sqlx::query_scalar!(
        r#"select categorie as "c!" from mail
           where categorie_par = 'humain' and categorie is not null and id <> $2
             and lower(expediteur) = lower($1) and expediteur <> ''
           order by trie_le desc nulls last limit 3"#,
        m.expediteur,
        m.id
    )
    .fetch_all(pool)
    .await?;
    if let Some((categorie, raison)) = tri::par_historique(&dernieres) {
        return appliquer(pool, m.id, &categorie, "regle", None, None, Some(&raison)).await;
    }
    if let Some((categorie, raison)) = tri::par_regle(&contexte) {
        return appliquer(pool, m.id, categorie, "regle", None, None, Some(&raison)).await;
    }

    let r = reglages::lire(pool).await?;
    if let Some(raison) = claude_indisponible(pool, &r).await? {
        return remettre(pool, m.id, &raison).await;
    }
    let Some(acces) = claude::acces(pool, &ctx.chiffreur).await? else {
        return remettre(pool, m.id, "Claude n’est pas connecté").await;
    };
    let vues: Vec<PieceVue<'_>> = pieces
        .iter()
        .filter(|p| p.etat.as_deref() != Some("ignore"))
        .map(|p| PieceVue {
            nom: &p.nom,
            type_document: p.type_document.as_deref(),
        })
        .collect();
    let mut prompt = tri::prompt(&m.expediteur, &m.objet, &m.corps_texte, &vues);
    prompt.push_str(&tri::bloc_exemples(&exemples(pool, m).await?));
    let schema = tri::schema().to_string();
    let args = [
        "--json-schema",
        &schema,
        "--system-prompt",
        tri::PROMPT_SYSTEME,
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--tools",
        "",
    ];
    let mut derniere = String::new();
    // Le nouvel essai garde le libellé de tâche « tri_mail » : il compte dans le quota de tri.
    for reglage in [&r.tri_mail, &r.nouvel_essai] {
        let mut a = Appel::avec_acces(reglages::TRI_MAIL, &reglage.modele, &prompt, &acces);
        a.effort = Some(&reglage.effort);
        a.args_en_plus = &args;
        match claude::lancer(pool, &ctx.config.claude_bin, &a).await {
            Ok(sortie) => match tri::depuis_reponse(&sortie.json) {
                Ok(Tri {
                    categorie,
                    resume,
                    priorite,
                }) => {
                    crate::lecture::noter_utilisation(pool).await;
                    return appliquer(
                        pool,
                        m.id,
                        &categorie,
                        "claude",
                        Some(&resume),
                        Some(&priorite),
                        None,
                    )
                    .await;
                }
                Err(e @ (ErreurTri::Absente | ErreurTri::Schema(_))) => {
                    tracing::warn!(mail = m.id, erreur = %e, "réponse de tri non conforme");
                    derniere = e.to_string();
                }
            },
            Err(ErreurClaude::JsonInvalide) => derniere = ErreurClaude::JsonInvalide.to_string(),
            Err(e) => {
                // Panne, délai, jeton refusé : la pompe réessaie dans 30 minutes.
                let message = crate::lecture::message_echec(pool, &e).await;
                return remettre(pool, m.id, &message).await;
            }
        }
    }
    anyhow::bail!("deux réponses de tri non conformes : {derniere}")
}

/// Mails classés à la main, du même expéditeur puis du même domaine d'abord, les corrections
/// avant les confirmations, puis les plus récents.
async fn exemples(pool: &PgPool, m: &MailTri) -> anyhow::Result<Vec<tri::Exemple>> {
    let domaine = tri::domaine(&m.expediteur).unwrap_or_default();
    let lignes = sqlx::query!(
        r#"select expediteur, objet, left(corps_texte, 4000) as "corps!", categorie as "categorie!",
                  case when categorie_auto is distinct from categorie then categorie_auto end as corrige_de
           from mail
           where categorie_par = 'humain' and categorie is not null and id <> $1
           order by (lower(expediteur) = lower($2)) desc,
                    ($3 <> '' and lower(split_part(expediteur, '@', 2)) = $3) desc,
                    (categorie_auto is not null and categorie_auto <> categorie) desc,
                    trie_le desc nulls last
           limit $4"#,
        m.id,
        m.expediteur,
        domaine,
        tri::EXEMPLES_MAX as i64
    )
    .fetch_all(pool)
    .await?;
    Ok(lignes
        .into_iter()
        .map(|l| tri::Exemple {
            expediteur: l.expediteur,
            objet: l.objet,
            corps: l.corps,
            categorie: l.categorie,
            corrige_de: l.corrige_de,
        })
        .collect())
}
