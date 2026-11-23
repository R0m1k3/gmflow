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
de la fiche qui désigne la même entreprise (même société, autre écriture du nom : sigle, \
forme juridique, ville ajoutée, nom collé ou nom de domaine comme « lafoirfouille.fr » pour \
« La Foir'Fouille »), ou null si aucune ne correspond (sinon une nouvelle fiche est créée). \
Deux sociétés différentes d'un même groupe ont des SIREN différents : si les SIRET ou TVA \
connus diffèrent, ce n'est pas la même fiche. Dans le doute, null.";

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
    let ids: Vec<i64> = candidats.iter().map(|c| c.id).collect();
    demander(ctx, document_id, PROMPT_SYSTEME, &prompt, &ids).await
}

/// Pose la question à Claude (sortie structurée `fournisseur_id`) : l'id choisi s'il est
/// parmi `ids`, sinon rien (Claude indisponible, en échec ou qui hésite).
async fn demander(
    ctx: &Contexte,
    document_id: i64,
    systeme: &str,
    prompt: &str,
    ids: &[i64],
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
    let reglage = r.pour(reglages::IDENTIFICATION_FOURNISSEUR).clone();
    let schema = schema().to_string();
    let args = [
        "--json-schema",
        &schema,
        "--system-prompt",
        systeme,
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--tools",
        "",
    ];
    let mut a = Appel::avec_acces(
        reglages::IDENTIFICATION_FOURNISSEUR,
        &reglage.modele,
        prompt,
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
    Ok(choisi.filter(|id| ids.contains(id)))
}

const PROMPT_MARQUE: &str = "Une même société facture pour plusieurs marques, chacune avec sa \
fiche fournisseur. Tu lis le texte d'une facture ou d'un bon de livraison de cette société et tu \
dis de quelle marque il s'agit (logo, en-tête, mention « marque », gamme, adresse ou site de la \
marque). Tu réponds uniquement avec la sortie structurée demandée : fournisseur_id est l'id de \
la marque, ou null si le document en concerne plusieurs ou si rien ne permet de trancher.";

/// Fiche de la marque d'un document dont le fournisseur est (ou fait partie d') un groupe
/// qui a déjà des marques : sans Claude, l'entité lue, la mention « Dép. : X » ou la seule
/// marque nommée dans le texte ; sinon une entité lue ou mentionnée inconnue devient une
/// nouvelle marque « à confirmer » ; sinon celle que Claude désigne. Rien si la fiche n'est
/// pas un groupe ou si rien ne tranche (le document reste sur sa fiche). Le booléen dit si le
/// groupe a gagné une marque.
async fn vers_marque(
    ctx: &Contexte,
    document_id: i64,
    groupe: i64,
    texte: &str,
    entite_lue: Option<&str>,
) -> anyhow::Result<Option<(i64, &'static str, bool)>> {
    let pool = &ctx.pool;
    let marques = fournisseurs::marques(pool, groupe).await?;
    if marques.is_empty() {
        return Ok(None);
    }
    if let Some(m) = fournisseurs::marque_du_document(&marques, texte, entite_lue) {
        return Ok(Some((m.id, "marque nommée", false)));
    }
    let nom_groupe = sqlx::query_scalar!("select nom from fournisseur where id = $1", groupe)
        .fetch_one(pool)
        .await?;
    let entite = entite_lue
        .map(fournisseurs::nom_propre)
        .or_else(|| fournisseurs::mention_entite(texte))
        .filter(|e| fournisseurs::entite_distincte(&nom_groupe, e));
    if let Some(e) = entite {
        let verrou = fournisseurs::verrouiller(pool).await?;
        let (id, nouvelle) = fournisseurs::marque_lue(pool, groupe, &e).await?;
        verrou.commit().await?;
        return Ok(Some((id, "entité nommée", nouvelle)));
    }
    let mut prompt = String::from("Marques de la société :\n");
    for m in &marques {
        prompt.push_str(&format!("- {} : {}", m.id, m.nom));
        if !m.noms_documents.is_empty() {
            prompt.push_str(&format!(" (aussi écrit {})", m.noms_documents.join(", ")));
        }
        prompt.push('\n');
    }
    prompt.push_str("\nTexte du document :\n");
    prompt.push_str(&gmflow_core::lecture::extraction::debut_et_fin(texte, 6000));
    let ids: Vec<i64> = marques.iter().map(|m| m.id).collect();
    Ok(demander(ctx, document_id, PROMPT_MARQUE, &prompt, &ids)
        .await?
        .map(|id| (id, "marque par claude", false)))
}

/// Le groupe vient de gagner une marque : ses documents déjà reçus vont à leur marque (sans
/// Claude), puis contrôles et rapprochements des fiches touchées sont recalculés.
async fn rattraper(pool: &PgPool, groupe: i64) -> anyhow::Result<()> {
    let (deplaces, _) = fournisseurs::repartir(pool, groupe).await?;
    if deplaces.is_empty() {
        return Ok(());
    }
    journal::ecrire(
        pool,
        None,
        "repartition",
        "fournisseur",
        Some(groupe.to_string()),
        json!({ "documents": deplaces.len(), "automatique": true }),
    )
    .await?;
    let mut touchees: Vec<i64> = deplaces.iter().map(|(_, m)| *m).collect();
    touchees.push(groupe);
    touchees.sort_unstable();
    touchees.dedup();
    for f in touchees {
        gmflow_core::depot::reevaluer(pool, None, Some(f)).await?;
        gmflow_core::rapprochement::recalculer_fournisseur(pool, f).await?;
    }
    Ok(())
}

/// Rattache un document lu à une fiche fournisseur (trouvée, choisie par Claude ou créée « à
/// confirmer »). Groupe et entité lus et distincts (« U10 », « Dép. : L3C ») : le document va
/// à la fiche de l'entité dans la fiche du groupe, créées au besoin « à confirmer ».
pub async fn rattacher(ctx: &Contexte, document_id: i64) -> anyhow::Result<()> {
    let pool = &ctx.pool;
    let Some(d) = sqlx::query!(
        r#"select d.fournisseur_nom, d.fournisseur_siret, d.fournisseur_tva, d.societe_id,
                  d.fournisseur_groupe, d.fournisseur_entite,
                  d.corrections ? 'fournisseur_id' as "fixe!", m.expediteur, m.objet,
                  coalesce(d.texte, '') as "texte!"
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
    let mut ind = Indices {
        nom: d.fournisseur_nom.clone(),
        siret: d.fournisseur_siret.clone(),
        tva: d.fournisseur_tva.clone(),
        domaine: fournisseurs::domaine_utile(&d.expediteur, &exclus),
    };
    ind.sans_identifiants_de(&fournisseurs::sirens_societes(pool).await?);
    let texte = format!(
        "{}\n{}\n{}",
        d.fournisseur_nom.as_deref().unwrap_or(""),
        d.objet,
        d.texte
    );
    // (fiche du document, comment, groupe qui a gagné une marque)
    let (id, comment, nouvelle_marque) = if let Some((g, e)) = fournisseurs::groupe_et_entite(
        d.fournisseur_groupe.as_deref(),
        d.fournisseur_entite.as_deref(),
    ) {
        // Les identifiants lus sont ceux du groupe (U10 facture pour L3C) : appris au groupe.
        let ind_groupe = Indices {
            nom: Some(g),
            ..ind.clone()
        };
        let verrou = fournisseurs::verrouiller(pool).await?;
        let (groupe, par_identifiant) = fournisseurs::groupe_lu(pool, &ind_groupe, &e).await?;
        fournisseurs::apprendre(pool, groupe, &ind_groupe, par_identifiant).await?;
        let (marque, nouvelle) = fournisseurs::marque_lue(pool, groupe, &e).await?;
        verrou.commit().await?;
        (marque, "groupe et entité lus", nouvelle.then_some(groupe))
    } else {
        // (fiche, comment, avec le domaine) ; les identifiants appris vont au groupe.
        let (id, comment, apprendre) = match fournisseurs::identifier(pool, &ind).await? {
            Identification::Trouve { id, par } => (Some(id), par, Some(par == "identifiant")),
            Identification::Candidats(c) => {
                match choisir(ctx, document_id, &ind, &d.expediteur, &c).await? {
                    Some(id) => (Some(id), "claude", Some(false)),
                    None => (
                        fournisseurs::creer(pool, &ind).await?,
                        "nouvelle fiche",
                        None,
                    ),
                }
            }
            Identification::Aucun => (
                fournisseurs::creer(pool, &ind).await?,
                "nouvelle fiche",
                None,
            ),
        };
        let Some(trouvee) = id else {
            return Ok(());
        };
        let groupe =
            sqlx::query_scalar!("select groupe_id from fournisseur where id = $1", trouvee)
                .fetch_one(pool)
                .await?
                .unwrap_or(trouvee);
        if let Some(avec_domaine) = apprendre {
            fournisseurs::apprendre(pool, groupe, &ind, avec_domaine).await?;
        }
        match vers_marque(
            ctx,
            document_id,
            groupe,
            &texte,
            d.fournisseur_entite.as_deref(),
        )
        .await?
        {
            Some((m, par, nouvelle)) => (m, par, nouvelle.then_some(groupe)),
            None => (trouvee, comment, None),
        }
    };
    let defaut = sqlx::query_scalar!(
        "select societe_defaut_id from fournisseur where id = $1",
        id
    )
    .fetch_one(pool)
    .await?;
    let societe_regle = d.societe_id.is_none() && defaut.is_some();
    let maj = sqlx::query!(
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
    if maj.rows_affected() == 0 {
        // Document effacé pendant le rattachement (la réinitialisation vide aussi les
        // fournisseurs) : la fiche, la marque ou le groupe créés pour lui ne restent pas seuls.
        let groupe = sqlx::query_scalar!("select groupe_id from fournisseur where id = $1", id)
            .fetch_optional(pool)
            .await?
            .flatten();
        for f in std::iter::once(id).chain(groupe) {
            sqlx::query!(
                "delete from fournisseur f where f.id = $1
                 and not exists (select 1 from document d where d.fournisseur_id = f.id)
                 and not exists (select 1 from fournisseur g where g.groupe_id = f.id)",
                f
            )
            .execute(pool)
            .await?;
        }
        return Ok(());
    }
    journal::ecrire(
        pool,
        None,
        "rattachement_fournisseur",
        "document",
        Some(document_id.to_string()),
        json!({ "fournisseur_id": id, "par": comment, "societe_par_regle": societe_regle }),
    )
    .await?;
    if let Some(g) = nouvelle_marque {
        rattraper(pool, g).await?;
    }
    Ok(())
}
