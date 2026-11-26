//! Rapprochement des bons de livraison et des factures (lot 4).
//!
//! Une facture et un BL du même fournisseur (et de la même structure quand les deux sont
//! connues) sont reliés quand la facture cite le n° du BL, quand le BL cite le n° de la
//! facture, ou quand ils portent le même n° de commande. Une facture peut regrouper plusieurs
//! BL ; un BL n'est relié automatiquement qu'à une facture. Les écarts de montant et de
//! quantités sont signalés, jamais bloquants. Les liens posés par un humain ne sont jamais
//! défaits par le calcul, et un lien automatique défait par un humain n'est pas recréé.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::lecture::extraction::Ligne;

/// Écart de montant HT au-delà duquel une facture est signalée.
pub const SEUIL_MONTANT: f64 = 0.05;
/// Écart toléré entre la somme des lignes d'un BL et son total HT.
pub const SEUIL_LIGNES_BL: f64 = 0.05;
/// Clé de verrou consultatif (avec l'id du fournisseur) : un calcul à la fois par fournisseur.
const VERROU: i32 = 0x0052_4150;
/// Texte d'un BL parcouru pour y chercher le n° d'une facture.
const TEXTE_CITATION: i32 = 30_000;

/// Majuscules, lettres et chiffres seulement : « cmd-077 / A » -> « CMD077A ».
pub fn normaliser(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

/// Le texte cite-t-il ce numéro, comme mot entier (les séparateurs `-/._` à l'intérieur d'un
/// mot sont ignorés) ? Les numéros de moins de 4 caractères ne comptent pas.
pub fn cite(texte: &str, numero: &str) -> bool {
    let n = normaliser(numero);
    if n.chars().count() < 4 {
        return false;
    }
    let mut mot = String::new();
    let mut prec_sep = false;
    for c in texte.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() {
            mot.extend(c.to_uppercase());
            prec_sep = false;
        } else if matches!(c, '-' | '/' | '.' | '_') && !mot.is_empty() && !prec_sep {
            prec_sep = true;
        } else {
            if mot == n {
                return true;
            }
            mot.clear();
            prec_sep = false;
        }
    }
    false
}

/// Méthode de rapprochement automatique d'une facture et d'un BL, la plus sûre d'abord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Methode {
    NumeroBl,
    FactureCitee,
    Commande,
}

impl Methode {
    pub fn code(self) -> &'static str {
        match self {
            Methode::NumeroBl => "numero_bl",
            Methode::FactureCitee => "facture_citee",
            Methode::Commande => "commande",
        }
    }
}

/// Ce qu'il faut d'une facture pour la rapprocher.
#[derive(Debug, Default, Clone)]
pub struct FactureRef {
    pub numero: Option<String>,
    pub numero_commande: Option<String>,
    pub numeros_bl: Vec<String>,
}

/// Ce qu'il faut d'un BL pour le rapprocher.
#[derive(Debug, Default, Clone)]
pub struct BlRef {
    pub numero: Option<String>,
    pub numero_commande: Option<String>,
    pub texte: Option<String>,
}

fn non_vide(s: &Option<String>) -> Option<String> {
    s.as_deref().map(normaliser).filter(|n| !n.is_empty())
}

/// Méthode qui relie cette facture et ce BL, s'il y en a une.
pub fn correspondance(f: &FactureRef, b: &BlRef) -> Option<Methode> {
    if let Some(n) = non_vide(&b.numero)
        && f.numeros_bl.iter().any(|x| normaliser(x) == n)
    {
        return Some(Methode::NumeroBl);
    }
    if let (Some(texte), Some(numero)) = (&b.texte, &f.numero)
        && cite(texte, numero)
    {
        return Some(Methode::FactureCitee);
    }
    if let (Some(a), Some(c)) = (non_vide(&f.numero_commande), non_vide(&b.numero_commande))
        && a == c
    {
        return Some(Methode::Commande);
    }
    None
}

/// Écart sur un article entre la facture et ses BL : quantité (manquant au BL, livré sans être
/// facturé) ou prix unitaire différent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EcartLigne {
    /// `manquant` (facturé, moins livré), `en_trop` (livré, moins facturé) ou `prix`.
    pub genre: &'static str,
    pub reference: Option<String>,
    pub designation: Option<String>,
    pub qte_bl: f64,
    pub qte_facture: f64,
    /// Prix unitaires HT lus de chaque côté (ou montant / quantité).
    pub prix_facture: Option<f64>,
    pub prix_bl: Option<f64>,
    /// Part de l'écart HT due à cet article : facturé − livré (positif si la facture dépasse).
    pub montant_ht: Option<f64>,
}

/// Article d'un côté : lignes de même gencod (sinon référence, sinon désignation) regroupées.
struct Article<'a> {
    gencod: Option<String>,
    reference: Option<String>,
    designation: Option<String>,
    qte: f64,
    /// Montant HT cumulé, quand chaque ligne en donne un (prix × quantité, ou montant lu).
    montant: Option<f64>,
    ligne: &'a Ligne,
}

/// Ce qui retrouve un article de l'autre côté, du plus sûr au moins sûr.
#[derive(Clone, Copy)]
enum Cle {
    Gencod,
    Reference,
    Designation,
}

impl Article<'_> {
    fn cle(&self, c: Cle) -> &Option<String> {
        match c {
            Cle::Gencod => &self.gencod,
            Cle::Reference => &self.reference,
            Cle::Designation => &self.designation,
        }
    }

    fn prix(&self) -> Option<f64> {
        self.montant
            .filter(|_| self.qte.abs() > 0.0005)
            .map(|m| m / self.qte)
    }
}

fn montant_ligne(l: &Ligne) -> Option<f64> {
    l.montant_ht
        .or_else(|| Some(l.prix_unitaire_ht? * l.quantite?))
}

/// Gencod comparable : clé GS1 juste, zéros de tête retirés (GTIN-14 « 0… » = EAN-13).
fn cle_gencod(l: &Ligne) -> Option<String> {
    let g: String = l.gencod.as_deref()?.split_whitespace().collect();
    crate::lecture::xml::gencod_valide(&g).then(|| g.trim_start_matches('0').to_string())
}

/// Quantité livrée ou facturée en unités : colis × PCB quand la ligne donne les deux (comme
/// l'export Phénix), sinon la quantité.
fn quantite(l: &Ligne) -> Option<f64> {
    match (l.colis, l.pcb) {
        (Some(c), Some(p)) if c > 0.0 && p > 0.0 => Some(c * p),
        _ => l.quantite,
    }
}

/// Articles d'un côté, dans l'ordre d'apparition.
fn articles(lignes: &[Ligne]) -> Vec<Article<'_>> {
    let mut v: Vec<Article> = Vec::new();
    for l in lignes {
        let Some(q) = quantite(l) else { continue };
        let (g, r, d) = (
            cle_gencod(l),
            non_vide(&l.reference),
            non_vide(&l.designation),
        );
        if g.is_none() && r.is_none() && d.is_none() {
            continue;
        }
        let m = montant_ligne(l);
        let meme = |a: &&mut Article| match (&g, &a.gencod, &r, &a.reference) {
            (Some(x), Some(y), _, _) => x == y,
            (_, _, Some(x), Some(y)) => x == y,
            (_, _, None, None) => d.is_some() && d == a.designation,
            _ => false,
        };
        match v.iter_mut().find(meme) {
            Some(a) => {
                a.qte += q;
                a.montant = a.montant.zip(m).map(|(x, y)| x + y);
            }
            None => v.push(Article {
                gencod: g,
                reference: r,
                designation: d,
                qte: q,
                montant: m,
                ligne: l,
            }),
        }
    }
    v
}

/// Même prix, ou prix de la facture net de la remise (`taux` %) : pas un changement de prix.
fn meme_prix(pf: f64, pb: f64, taux: Option<f64>) -> bool {
    let proche = |a: f64, b: f64| (a - b).abs() <= 0.01_f64.max(b.abs() * 0.001);
    proche(pf, pb)
        || taux.is_some_and(|t| {
            let k = 1.0 - t / 100.0;
            proche(pf, pb * k) || proche(pf * k, pb)
        })
}

fn arrondi(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Écarts entre les lignes d'une facture et celles de ses BL : articles manquants ou en trop,
/// prix unitaires différents (hors remise de `remise_taux` %). Un article est retrouvé par son
/// gencod, sinon sa référence, sinon sa désignation (référence fournisseur d'un côté, pas de
/// l'autre) ; une ligne en colis compte colis × PCB unités. Vide
/// si l'un des deux côtés n'a pas de lignes avec quantité (rien à comparer).
pub fn comparer(facture: &[Ligne], bls: &[Ligne], remise_taux: Option<f64>) -> Vec<EcartLigne> {
    let (f, b) = (articles(facture), articles(bls));
    if f.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut pris = vec![false; b.len()];
    let mut ecarts = Vec::new();
    let mut ajouter = |fa: Option<&Article>, ba: Option<&Article>| {
        let (qf, qb) = (fa.map_or(0.0, |a| a.qte), ba.map_or(0.0, |a| a.qte));
        let (pf, pb) = (fa.and_then(Article::prix), ba.and_then(Article::prix));
        let genre = if qf - qb > 0.0005 {
            "manquant"
        } else if qb - qf > 0.0005 {
            "en_trop"
        } else if let (Some(x), Some(y)) = (pf, pb)
            && !meme_prix(x, y, remise_taux)
        {
            "prix"
        } else {
            return;
        };
        // Part de l'écart : chaque côté à son prix, à défaut au prix de l'autre.
        let montant = match (pf.or(pb), pb.or(pf)) {
            (Some(x), Some(y)) => Some(arrondi(qf * x - qb * y)),
            _ => None,
        };
        let l = fa.or(ba).map(|a| a.ligne);
        ecarts.push(EcartLigne {
            genre,
            reference: l.and_then(|l| l.reference.clone()),
            designation: l.and_then(|l| l.designation.clone()),
            qte_bl: qb,
            qte_facture: qf,
            prix_facture: pf.map(|p| (p * 10_000.0).round() / 10_000.0),
            prix_bl: pb.map(|p| (p * 10_000.0).round() / 10_000.0),
            montant_ht: montant,
        });
    };
    for fa in &f {
        let trouve = |cle: Cle| {
            (0..b.len())
                .find(|&i| !pris[i] && fa.cle(cle).is_some() && fa.cle(cle) == b[i].cle(cle))
        };
        match trouve(Cle::Gencod)
            .or_else(|| trouve(Cle::Reference))
            .or_else(|| trouve(Cle::Designation))
        {
            Some(i) => {
                pris[i] = true;
                ajouter(Some(fa), Some(&b[i]));
            }
            None => ajouter(Some(fa), None),
        }
    }
    for (i, ba) in b.iter().enumerate() {
        if !pris[i] {
            ajouter(None, Some(ba));
        }
    }
    ecarts
}

/// Somme des montants des lignes, si toutes en ont un.
fn somme_lignes(lignes: &[Ligne]) -> Option<f64> {
    if lignes.is_empty() {
        return None;
    }
    lignes.iter().map(|l| l.montant_ht).sum()
}

/// Montant HT d'un BL : son total, sinon la somme de ses lignes.
pub fn montant_bl(lignes: &[Ligne], total_ht: Option<f64>) -> Option<f64> {
    total_ht.or_else(|| somme_lignes(lignes))
}

/// Écart expliqué par la facture elle-même : sa remise (absente des BL valorisés avant remise)
/// et ses montants annexes (éco-participation, port : jamais sur un BL). Vrai quand, une fois
/// ces montants retirés, l'écart tombe sous le seuil, que les BL soient avant ou après remise.
pub fn ecart_explique(ecart: f64, remise: Option<f64>, annexes: Option<f64>) -> bool {
    let (r, a) = (remise.unwrap_or(0.0), annexes.unwrap_or(0.0));
    if r <= 0.0 && a <= 0.0 {
        return false;
    }
    (ecart + r - a).abs() <= SEUIL_MONTANT || (ecart - a).abs() <= SEUIL_MONTANT
}

/// HT de la facture moins la somme des BL, si tous les montants sont connus.
pub fn ecart_montant(facture_ht: Option<f64>, bls: &[Option<f64>]) -> Option<f64> {
    if bls.is_empty() {
        return None;
    }
    let total: Option<f64> = bls.iter().copied().sum();
    Some(((facture_ht? - total?) * 100.0).round() / 100.0)
}

/// Contrôle d'un BL (non bloquant) : la somme des lignes doit faire le total HT, quand les
/// deux existent.
pub fn controle_bl(lignes: &[Ligne], total_ht: Option<f64>) -> Vec<Value> {
    match (somme_lignes(lignes), total_ht) {
        (Some(s), Some(t)) if (s - t).abs() > SEUIL_LIGNES_BL => vec![json!({
            "code": "lignes",
            "message": format!(
                "la somme des lignes ({s:.2} €) ne fait pas le total HT ({t:.2} €)"
            ),
        })],
        _ => Vec::new(),
    }
}

/// Contrôle d'un BL lu en partie seulement (`extraction::LIGNES_TRONQUEES`), à la place de la
/// somme des lignes.
pub fn controle_lignes_tronquees() -> Value {
    json!({
        "code": "lignes_tronquees",
        "message": "BL trop long pour être lu en entier : des lignes articles manquent \
                    (totaux non calculés, pas de comparaison avec la facture, export Phénix \
                    incomplet)",
    })
}

/// Lignes de plusieurs documents.
pub async fn lignes(c: &mut PgConnection, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<Ligne>>> {
    let r = sqlx::query!(
        "select document_id, reference, designation, quantite::float8 as quantite, unite,
                prix_unitaire_ht::float8 as prix, montant_ht::float8 as montant, gencod,
                colis::float8 as colis, pcb::float8 as pcb
         from ligne_document where document_id = any($1) order by document_id, rang",
        ids
    )
    .fetch_all(&mut *c)
    .await?;
    let mut m: HashMap<i64, Vec<Ligne>> = HashMap::new();
    for l in r {
        m.entry(l.document_id).or_default().push(Ligne {
            reference: l.reference,
            designation: l.designation,
            quantite: l.quantite,
            unite: l.unite,
            prix_unitaire_ht: l.prix,
            montant_ht: l.montant,
            gencod: l.gencod,
            colis: l.colis,
            pcb: l.pcb,
            ..Default::default()
        });
    }
    Ok(m)
}

/// Remplace les lignes d'un document.
pub async fn ecrire_lignes(
    c: &mut PgConnection,
    document_id: i64,
    lignes: &[Ligne],
) -> sqlx::Result<()> {
    sqlx::query!(
        "delete from ligne_document where document_id = $1",
        document_id
    )
    .execute(&mut *c)
    .await?;
    if lignes.is_empty() {
        return Ok(());
    }
    let rangs: Vec<i32> = (0..lignes.len() as i32).collect();
    let refs: Vec<Option<String>> = lignes.iter().map(|l| l.reference.clone()).collect();
    let des: Vec<Option<String>> = lignes.iter().map(|l| l.designation.clone()).collect();
    let qte: Vec<Option<f64>> = lignes.iter().map(|l| l.quantite).collect();
    let unites: Vec<Option<String>> = lignes.iter().map(|l| l.unite.clone()).collect();
    let prix: Vec<Option<f64>> = lignes.iter().map(|l| l.prix_unitaire_ht).collect();
    let montants: Vec<Option<f64>> = lignes.iter().map(|l| l.montant_ht).collect();
    let gencods: Vec<Option<String>> = lignes
        .iter()
        .map(|l| {
            l.gencod
                .as_deref()
                .map(|g| g.split_whitespace().collect::<String>())
                .filter(|g| !g.is_empty())
        })
        .collect();
    let nombres = |f: fn(&Ligne) -> Option<f64>| lignes.iter().map(f).collect::<Vec<_>>();
    let (colis, pcb, remises, tva) = (
        nombres(|l| l.colis),
        nombres(|l| l.pcb),
        nombres(|l| l.remise_taux),
        nombres(|l| l.taux_tva),
    );
    let (deee, ecomob, pvc) = (
        nombres(|l| l.deee_ht),
        nombres(|l| l.ecomob_ht),
        nombres(|l| l.pv_conseille),
    );
    sqlx::query!(
        r#"insert into ligne_document (document_id, rang, reference, designation, quantite, unite,
                                       prix_unitaire_ht, montant_ht, gencod, colis, pcb,
                                       remise_taux, taux_tva, deee_ht, ecomob_ht, pv_conseille)
           select $1, r, re, de, q::numeric(14, 3), u, p::numeric(14, 4), m::numeric(14, 2), g,
                  c::numeric(14, 3), pc::numeric(14, 3), rt::numeric(8, 4), tv::numeric(6, 3),
                  dh::numeric(14, 4), eh::numeric(14, 4), pv::numeric(14, 4)
           from unnest($2::int[], $3::text[], $4::text[], $5::float8[], $6::text[], $7::float8[],
                       $8::float8[], $9::text[], $10::float8[], $11::float8[], $12::float8[],
                       $13::float8[], $14::float8[], $15::float8[], $16::float8[])
                as t(r, re, de, q, u, p, m, g, c, pc, rt, tv, dh, eh, pv)"#,
        document_id,
        &rangs,
        &refs as &[Option<String>],
        &des as &[Option<String>],
        &qte as &[Option<f64>],
        &unites as &[Option<String>],
        &prix as &[Option<f64>],
        &montants as &[Option<f64>],
        &gencods as &[Option<String>],
        &colis as &[Option<f64>],
        &pcb as &[Option<f64>],
        &remises as &[Option<f64>],
        &tva as &[Option<f64>],
        &deee as &[Option<f64>],
        &ecomob as &[Option<f64>],
        &pvc as &[Option<f64>]
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Recalcule les écarts de tous les liens d'une facture.
pub async fn actualiser_ecarts(c: &mut PgConnection, facture_id: i64) -> sqlx::Result<()> {
    let bls = sqlx::query!(
        r#"select b.id, b.montant_ht::float8 as ht,
                  coalesce((b.confiance->>'lignes_tronquees')::boolean, false) as "tronque!"
           from rapprochement r
           join document b on b.id = r.bl_id
           where r.facture_id = $1 and not r.rejete order by b.id"#,
        facture_id
    )
    .fetch_all(&mut *c)
    .await?;
    if bls.is_empty() {
        return Ok(());
    }
    let f = sqlx::query!(
        r#"select montant_ht::float8 as ht, remise_taux::float8 as taux,
                  coalesce((confiance->>'lignes_tronquees')::boolean, false) as "tronque!"
           from document where id = $1"#,
        facture_id
    )
    .fetch_one(&mut *c)
    .await?;
    let ht_facture = f.ht;
    let mut ids: Vec<i64> = bls.iter().map(|b| b.id).collect();
    ids.push(facture_id);
    let mut l = lignes(c, &ids).await?;
    let lf = l.remove(&facture_id).unwrap_or_default();
    let mut lb: Vec<Ligne> = Vec::new();
    let mut montants = Vec::new();
    for b in &bls {
        let lignes_bl = l.remove(&b.id).unwrap_or_default();
        // Lignes incomplètes : leur somme n'est pas le montant du BL.
        montants.push(montant_bl(if b.tronque { &[] } else { &lignes_bl }, b.ht));
        lb.extend(lignes_bl);
    }
    let ecart = ecart_montant(ht_facture, &montants);
    // Des lignes manquent d'un côté : les comparer fabriquerait des articles « en trop » ou
    // « manquants ».
    let tronque = f.tronque || bls.iter().any(|b| b.tronque);
    let ecarts = if tronque {
        json!([])
    } else {
        json!(comparer(&lf, &lb, f.taux))
    };
    sqlx::query!(
        "update rapprochement set ecart_montant = $2::float8::numeric(14, 2), ecarts = $3
         where facture_id = $1 and not rejete",
        facture_id,
        ecart,
        ecarts
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Recalcule les liens automatiques d'un document (facture ou BL), et les écarts des factures
/// concernées. Sans effet visible sur un document qui n'est ni une facture ni un BL lus.
pub async fn recalculer(pool: &PgPool, document_id: i64) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let Some(d) = sqlx::query!(
        r#"select etat, type, fournisseur_id, societe_id, numero, numero_commande, numeros_bl,
                rapprochement_ignore, montant_ht::float8 as ht, left(texte, $2) as texte,
                coalesce(confiance->'montants_calcules' ? 'ht', false) as "ht_calcule!",
                coalesce((confiance->>'lignes_tronquees')::boolean, false) as "tronque!"
         from document where id = $1"#,
        document_id,
        TEXTE_CITATION
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(());
    };
    if let Some(f) = d.fournisseur_id {
        sqlx::query("select pg_advisory_xact_lock($1, ($2::bigint % 2147483647)::int)")
            .bind(VERROU)
            .bind(f)
            .execute(&mut *tx)
            .await?;
    }
    // Factures dont les écarts changent : celles liées au document avant le calcul.
    let mut factures: Vec<i64> = sqlx::query_scalar!(
        "select distinct facture_id from rapprochement where facture_id = $1 or bl_id = $1",
        document_id
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!(
        "delete from rapprochement
         where (facture_id = $1 or bl_id = $1) and methode <> 'manuel' and not rejete",
        document_id
    )
    .execute(&mut *tx)
    .await?;
    let lu = d.etat == "lu" && !d.rapprochement_ignore;
    let type_doc = d.r#type.as_deref().unwrap_or("");
    if type_doc == "bl" && d.etat == "lu" {
        let l = lignes(&mut tx, &[document_id]).await?;
        // HT calculé depuis les lignes (éco-participations comprises) : rien à contrôler.
        let total = d.ht.filter(|_| !d.ht_calcule);
        let c = if d.tronque {
            vec![controle_lignes_tronquees()]
        } else {
            controle_bl(l.get(&document_id).map_or(&[][..], |v| v), total)
        };
        sqlx::query!(
            "update document set controles = $2 where id = $1",
            document_id,
            json!(c)
        )
        .execute(&mut *tx)
        .await?;
    }
    if let (true, Some(fournisseur)) = (lu, d.fournisseur_id) {
        match type_doc {
            "facture" => {
                let f = FactureRef {
                    numero: d.numero.clone(),
                    numero_commande: d.numero_commande.clone(),
                    numeros_bl: d.numeros_bl.clone(),
                };
                let bls = sqlx::query!(
                    "select b.id, b.numero, b.numero_commande, left(b.texte, $4) as texte
                     from document b
                     where b.type = 'bl' and b.etat = 'lu' and not b.rapprochement_ignore
                       and b.fournisseur_id = $2 and b.id <> $1
                       and (b.societe_id is null or $3::bigint is null or b.societe_id = $3)
                       and not exists (select 1 from rapprochement r where r.bl_id = b.id
                                       and (r.facture_id = $1 or not r.rejete))
                     order by b.id",
                    document_id,
                    fournisseur,
                    d.societe_id,
                    TEXTE_CITATION
                )
                .fetch_all(&mut *tx)
                .await?;
                for b in bls {
                    let bl = BlRef {
                        numero: b.numero,
                        numero_commande: b.numero_commande,
                        texte: b.texte,
                    };
                    if let Some(m) = correspondance(&f, &bl) {
                        inserer(&mut tx, document_id, b.id, m).await?;
                    }
                }
                factures.push(document_id);
            }
            "bl" => {
                let deja = sqlx::query_scalar!(
                    r#"select exists (select 1 from rapprochement where bl_id = $1 and not rejete) as "ok!""#,
                    document_id
                )
                .fetch_one(&mut *tx)
                .await?;
                if !deja {
                    let bl = BlRef {
                        numero: d.numero.clone(),
                        numero_commande: d.numero_commande.clone(),
                        texte: d.texte.clone(),
                    };
                    let candidates = sqlx::query!(
                        "select f.id, f.numero, f.numero_commande, f.numeros_bl from document f
                         where f.type = 'facture' and f.etat = 'lu' and not f.rapprochement_ignore
                           and f.fournisseur_id = $2 and f.id <> $1
                           and (f.societe_id is null or $3::bigint is null or f.societe_id = $3)
                           and not exists (select 1 from rapprochement r
                                           where r.facture_id = f.id and r.bl_id = $1)
                         order by f.id desc",
                        document_id,
                        fournisseur,
                        d.societe_id
                    )
                    .fetch_all(&mut *tx)
                    .await?;
                    let meilleure = candidates
                        .into_iter()
                        .filter_map(|f| {
                            let fr = FactureRef {
                                numero: f.numero,
                                numero_commande: f.numero_commande,
                                numeros_bl: f.numeros_bl,
                            };
                            correspondance(&fr, &bl).map(|m| (m, f.id))
                        })
                        // La méthode la plus sûre, puis la facture la plus récente.
                        .min_by_key(|(m, id)| (*m, std::cmp::Reverse(*id)));
                    if let Some((m, f)) = meilleure {
                        inserer(&mut tx, f, document_id, m).await?;
                        factures.push(f);
                    }
                }
            }
            _ => {}
        }
    }
    factures.sort_unstable();
    factures.dedup();
    for f in factures {
        actualiser_ecarts(&mut tx, f).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn inserer(c: &mut PgConnection, facture: i64, bl: i64, m: Methode) -> sqlx::Result<()> {
    sqlx::query!(
        "insert into rapprochement (facture_id, bl_id, methode) values ($1, $2, $3)
         on conflict (facture_id, bl_id) do nothing",
        facture,
        bl,
        m.code()
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Recalcule tous les documents d'un fournisseur (après une fusion de fiches, par exemple) :
/// les factures d'abord, puis les BL restés seuls.
pub async fn recalculer_fournisseur(pool: &PgPool, fournisseur_id: i64) -> anyhow::Result<usize> {
    let ids = sqlx::query_scalar!(
        "select id from document where fournisseur_id = $1 and etat = 'lu' and type in ('facture', 'bl')
         order by (type = 'bl'), id",
        fournisseur_id
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        recalculer(pool, *id).await?;
    }
    Ok(ids.len())
}

/// Erreurs de rapprochement manuel.
#[derive(Debug, thiserror::Error)]
pub enum ErreurLien {
    #[error("le premier document doit être une facture et le second un BL, lus tous les deux")]
    Types,
    #[error("lien introuvable")]
    Absent,
    #[error(transparent)]
    Base(#[from] sqlx::Error),
}

/// Lien posé par un humain entre une facture et un BL. Un lien automatique du BL vers une
/// autre facture est défait (et ne sera pas recréé).
pub async fn lier(
    pool: &PgPool,
    facture_id: i64,
    bl_id: i64,
    auteur_id: i64,
) -> Result<Vec<i64>, ErreurLien> {
    let mut tx = pool.begin().await?;
    let ok = sqlx::query_scalar!(
        r#"select (select type = 'facture' and etat = 'lu' from document where id = $1)
              and (select type = 'bl' and etat = 'lu' from document where id = $2) as "ok""#,
        facture_id,
        bl_id
    )
    .fetch_one(&mut *tx)
    .await?;
    if ok != Some(true) {
        return Err(ErreurLien::Types);
    }
    let autres: Vec<i64> = sqlx::query_scalar!(
        "update rapprochement set rejete = true
         where bl_id = $1 and facture_id <> $2 and methode <> 'manuel' and not rejete
         returning facture_id",
        bl_id,
        facture_id
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!(
        "insert into rapprochement (facture_id, bl_id, methode, auteur_id) values ($1, $2, 'manuel', $3)
         on conflict (facture_id, bl_id) do update
           set methode = 'manuel', rejete = false, auteur_id = $3, cree_le = now()",
        facture_id,
        bl_id,
        auteur_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "update document set rapprochement_ignore = false where id in ($1, $2)",
        facture_id,
        bl_id
    )
    .execute(&mut *tx)
    .await?;
    for f in autres.iter().chain(std::iter::once(&facture_id)) {
        actualiser_ecarts(&mut tx, *f).await?;
    }
    tx.commit().await?;
    Ok(autres)
}

/// Défait un lien : supprimé s'il était manuel, marqué rejeté s'il était automatique. Rend
/// (facture, BL).
pub async fn delier(pool: &PgPool, id: i64) -> Result<(i64, i64), ErreurLien> {
    let mut tx = pool.begin().await?;
    let Some(r) = sqlx::query!(
        "select facture_id, bl_id, methode from rapprochement where id = $1 and not rejete for update",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Err(ErreurLien::Absent);
    };
    if r.methode == "manuel" {
        sqlx::query!("delete from rapprochement where id = $1", id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query!(
            "update rapprochement set rejete = true, ecart_montant = null, ecarts = '[]' where id = $1",
            id
        )
        .execute(&mut *tx)
        .await?;
    }
    actualiser_ecarts(&mut tx, r.facture_id).await?;
    tx.commit().await?;
    Ok((r.facture_id, r.bl_id))
}

/// « Pas de correspondance attendue » : le document sort du rapprochement automatique (ses
/// liens automatiques sont retirés), ou y revient.
pub async fn ignorer(pool: &PgPool, document_id: i64, ignore: bool) -> anyhow::Result<()> {
    sqlx::query!(
        "update document set rapprochement_ignore = $2 where id = $1",
        document_id,
        ignore
    )
    .execute(pool)
    .await?;
    recalculer(pool, document_id).await
}
