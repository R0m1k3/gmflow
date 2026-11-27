//! Mesure de la lecture sur un mois : pièces validées par un humain, part corrigée à la main
//! (par champ, par voie de lecture, par modèle du dernier essai, fournisseurs les plus
//! corrigés) et coût estimé des appels de lecture (`appel_claude`, coût donné par le CLI).
//!
//! Une pièce compte dans le mois où elle a été validée (`verifie_le`, heure de Paris) ; un appel
//! dans le mois où il a été fait. Avec une société, seules ses pièces comptent, et seuls les
//! appels rattachés à une de ses pièces.

use serde::Serialize;
use sqlx::PgPool;
use time::Date;

use crate::reglages;

/// Fournisseurs les plus corrigés montrés.
pub const FOURNISSEURS_MAX: i64 = 10;

#[derive(Debug, Serialize)]
pub struct Qualite {
    /// Factures, avoirs et BL validés par un humain dans le mois.
    pub validees: i64,
    /// Dont au moins un champ corrigé à la main.
    pub corrigees: i64,
    pub par_champ: Vec<ParChamp>,
    pub par_voie: Vec<Groupe>,
    pub par_modele: Vec<ParModele>,
    pub fournisseurs: Vec<ParFournisseur>,
    pub cout: Cout,
}

/// Champ corrigé (nom de l'API : `numero`, `date_document`, `montant_ttc`…) sur `pieces`.
#[derive(Debug, Serialize)]
pub struct ParChamp {
    pub champ: String,
    pub pieces: i64,
}

#[derive(Debug, Serialize)]
pub struct Groupe {
    /// Voie de lecture (`facturx`, `xml`, `pdf_texte`, `tableur`, `scan`), vide si inconnue ;
    /// suivie de `+ocr` (texte des pages scannées lu par l'OCR local) ou de `+cellules` (lignes
    /// lues dans les cellules du tableur).
    pub cle: Option<String>,
    pub validees: i64,
    pub corrigees: i64,
}

/// Pièces selon le dernier appel de lecture réussi : modèle, et premier essai ou nouvel essai
/// (reprise après une lecture douteuse, ou relecture en détail). Sans modèle : pièce lue sans
/// Claude (Factur-X, XML) ou avant la mesure des appels.
#[derive(Debug, Serialize)]
pub struct ParModele {
    pub modele: Option<String>,
    pub nouvel_essai: bool,
    pub validees: i64,
    pub corrigees: i64,
}

#[derive(Debug, Serialize)]
pub struct ParFournisseur {
    pub fournisseur_id: i64,
    pub nom: String,
    pub validees: i64,
    pub corrigees: i64,
}

#[derive(Debug, Serialize)]
pub struct Cout {
    /// Somme des coûts estimés des appels de lecture du mois (dollars).
    pub total_usd: f64,
    /// Appels de lecture du mois, et ceux sans coût connu (CLI sans `total_cost_usd`, ou
    /// faits avant la mesure).
    pub appels: i64,
    pub appels_sans_cout: i64,
    /// Pièces auxquelles des appels avec coût sont rattachés, et leur coût moyen.
    pub pieces: i64,
    pub moyen_par_piece_usd: Option<f64>,
}

/// Tâches dont le coût est celui de la lecture.
fn taches_cout() -> Vec<String> {
    reglages::TACHES_LECTURE
        .iter()
        .map(|t| t.to_string())
        .collect()
}

/// Mesure du mois commençant le `debut` (1er du mois), pour une société ou toutes.
pub async fn mesurer(pool: &PgPool, debut: Date, societe: Option<i64>) -> sqlx::Result<Qualite> {
    let lecture: Vec<String> = [
        reglages::EXTRACTION_TEXTE,
        reglages::OCR_SCAN,
        reglages::NOUVEL_ESSAI,
    ]
    .iter()
    .map(|t| t.to_string())
    .collect();
    let totaux = sqlx::query!(
        r#"select count(*) as "validees!",
                  count(*) filter (where d.corrections <> '{}'::jsonb) as "corrigees!"
           from document d
           where d.verifie_le >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and d.verifie_le < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ($2::bigint is null or d.societe_id = $2)"#,
        debut,
        societe
    )
    .fetch_one(pool)
    .await?;
    let par_champ = sqlx::query_as!(
        ParChamp,
        r#"select k as "champ!", count(*) as "pieces!"
           from document d, jsonb_object_keys(d.corrections) k
           where d.verifie_le >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and d.verifie_le < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ($2::bigint is null or d.societe_id = $2)
           group by k order by 2 desc, 1"#,
        debut,
        societe
    )
    .fetch_all(pool)
    .await?;
    let par_voie = sqlx::query_as!(
        Groupe,
        r#"select d.voie || case when d.confiance->>'texte_source' = 'ocr_local' then '+ocr'
                                 when d.confiance->>'lignes_source' = 'cellules' then '+cellules'
                                 else '' end as "cle?",
                  count(*) as "validees!",
                  count(*) filter (where d.corrections <> '{}'::jsonb) as "corrigees!"
           from document d
           where d.verifie_le >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and d.verifie_le < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ($2::bigint is null or d.societe_id = $2)
           group by 1 order by 2 desc, 1"#,
        debut,
        societe
    )
    .fetch_all(pool)
    .await?;
    let par_modele = sqlx::query_as!(
        ParModele,
        r#"select a.modele as "modele?", coalesce(a.tache = $3, false) as "nouvel_essai!",
                  count(*) as "validees!",
                  count(*) filter (where d.corrections <> '{}'::jsonb) as "corrigees!"
           from document d
           left join lateral (
               select x.modele, x.tache from appel_claude x
               where x.document_id = d.id and x.succes and x.tache = any($4)
               order by x.id desc limit 1
           ) a on true
           where d.verifie_le >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and d.verifie_le < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ($2::bigint is null or d.societe_id = $2)
           group by 1, 2 order by 3 desc, 1, 2"#,
        debut,
        societe,
        reglages::NOUVEL_ESSAI,
        &lecture
    )
    .fetch_all(pool)
    .await?;
    let fournisseurs = sqlx::query_as!(
        ParFournisseur,
        r#"select f.id as fournisseur_id, f.nom, count(*) as "validees!",
                  count(*) filter (where d.corrections <> '{}'::jsonb) as "corrigees!"
           from document d join fournisseur f on f.id = d.fournisseur_id
           where d.verifie_le >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and d.verifie_le < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and d.etat = 'lu' and d.type in ('facture', 'avoir', 'bl')
             and ($2::bigint is null or d.societe_id = $2)
           group by f.id, f.nom
           having count(*) filter (where d.corrections <> '{}'::jsonb) > 0
           order by count(*) filter (where d.corrections <> '{}'::jsonb)::float8 / count(*) desc,
                    3 desc, f.nom
           limit $3"#,
        debut,
        societe,
        FOURNISSEURS_MAX
    )
    .fetch_all(pool)
    .await?;
    let c = sqlx::query!(
        r#"select coalesce(sum(a.cout_usd), 0)::float8 as "total!",
                  count(*) as "appels!",
                  count(*) filter (where a.cout_usd is null) as "sans_cout!",
                  count(distinct a.document_id) filter (where a.cout_usd is not null) as "pieces!",
                  coalesce(sum(a.cout_usd) filter (where a.document_id is not null), 0)::float8
                      as "cout_pieces!"
           from appel_claude a
           left join document d on d.id = a.document_id
           where a.quand >= ($1::date)::timestamp at time zone 'Europe/Paris'
             and a.quand < ($1::date + interval '1 month') at time zone 'Europe/Paris'
             and a.tache = any($3)
             and ($2::bigint is null or d.societe_id = $2)"#,
        debut,
        societe,
        &taches_cout()
    )
    .fetch_one(pool)
    .await?;
    Ok(Qualite {
        validees: totaux.validees,
        corrigees: totaux.corrigees,
        par_champ,
        par_voie,
        par_modele,
        fournisseurs,
        cout: Cout {
            total_usd: c.total,
            appels: c.appels,
            appels_sans_cout: c.sans_cout,
            pieces: c.pieces,
            moyen_par_piece_usd: (c.pieces > 0).then(|| c.cout_pieces / c.pieces as f64),
        },
    })
}
