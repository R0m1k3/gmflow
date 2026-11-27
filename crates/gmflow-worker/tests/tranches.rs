//! Lecture par tranches d'un très gros BL PDF, de bout en bout, avec le faux Claude
//! (`faux-claude.sh`, mode `FAUX_TRANCHES`) : toutes les lignes une seule fois, totaux
//! contrôlés, tranche en échec réessayée seule, plafond du nombre de tranches, quota du jour et
//! reprise sans tout relire.

mod commun;

use commun::{Banc, faux_claude, mail};
use gmflow_core::fixtures::{self, PageTest};
use gmflow_core::lecture::tranches::TRANCHES_MAX;
use gmflow_core::montant::{self, Decimal};
use gmflow_core::{parametre, reglages};
use serde_json::{Value, json};

/// Faux Claude des tranches : script d'appel qui pose `FAUX_TRANCHES` puis lance
/// `faux-claude.sh` (l'environnement du CLI est vidé par `claude::lancer`).
struct FauxTranches {
    dossier: gmflow_core::claude::DossierTemp,
}

impl FauxTranches {
    fn new(entete: &Value) -> Self {
        let dossier = gmflow_core::claude::DossierTemp::creer("faux-tranches").unwrap();
        std::fs::write(dossier.0.join("entete.json"), entete.to_string()).unwrap();
        let script = format!(
            "#!/bin/sh\nFAUX_TRANCHES='{}' exec sh '{}' \"$@\"\n",
            dossier.0.display(),
            faux_claude()
        );
        let chemin = dossier.0.join("claude");
        std::fs::write(&chemin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dossier }
    }

    fn bin(&self) -> String {
        self.dossier.0.join("claude").display().to_string()
    }

    /// Tranches qui échouent : « 4 haiku » (au premier essai), « 4 * » (toujours).
    fn echec(&self, regles: &str) {
        std::fs::write(self.dossier.0.join("echec"), regles).unwrap();
    }

    fn journal(&self) -> Vec<String> {
        std::fs::read_to_string(self.dossier.0.join("journal"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn vider_journal(&self) {
        let _ = std::fs::remove_file(self.dossier.0.join("journal"));
    }
}

async fn banc(faux: &FauxTranches) -> Banc {
    let mut b = Banc::new().await;
    let mut config = (*b.ctx.config).clone();
    config.claude_bin = faux.bin();
    b.ctx.config = std::sync::Arc::new(config);
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_tranches")
        .await
        .unwrap();
    b
}

async fn quota(b: &Banc, n: i64) {
    let mut r = reglages::lire(b.pool()).await.unwrap();
    r.quota_lecture_jour = n;
    reglages::ecrire(b.pool(), &r).await.unwrap();
}

fn euros(m: Decimal) -> String {
    montant::euros(m).replace('.', ",")
}

/// BL PDF de `n` lignes, `par_page` par page, en-tête de tableau répété sur chaque page ; rend
/// le PDF, le nombre de pages et le total HT (imprimé en pied de la dernière page).
fn bl(n: usize, par_page: usize) -> (Vec<u8>, usize, Decimal) {
    let pages = n.div_ceil(par_page);
    let mut total = Decimal::ZERO;
    let mut v = Vec::new();
    for p in 0..pages {
        let mut l = vec![
            format!("BON DE LIVRAISON BL-400 Page {}/{pages}", p + 1),
            "Réf Désignation Qté PU Montant".to_string(),
        ];
        for i in p * par_page..((p + 1) * par_page).min(n) {
            let (q, pu) = (
                Decimal::from(i % 5 + 1),
                Decimal::from(i % 7) + Decimal::new(5, 1),
            );
            total += q * pu;
            l.push(format!(
                "REF{i:04} Article {i} {q} {} {}",
                euros(pu),
                euros(q * pu)
            ));
        }
        if p + 1 == pages {
            l.push(format!("Total HT {}", euros(total)));
            l.push(format!("Nombre de lignes : {n}"));
        }
        v.push(PageTest::Texte(l));
    }
    (fixtures::pdf_pages(v), pages, montant::centimes(total))
}

fn entete(ht: Decimal, lignes: usize) -> Value {
    json!({
        "type": "bl", "societe_id": null,
        "fournisseur": { "nom": "Durand", "siret": null, "tva": null, "groupe": null, "entite": null },
        "numero": "BL-400", "date": "2026-11-20", "numero_commande": null, "echeance": null,
        "montant_ht": ht, "montant_tva": null, "montant_ttc": null,
        "remise_ht": null, "remise_taux": null, "annexes_ht": null, "iban": null, "bic": null,
        "lignes": [], "numeros_bl": [],
        "confiance": { "type": 1, "societe": 0.5, "fournisseur": 0.9, "numero": 1, "date": 1, "montants": 0.9 },
        "nombre_lignes_annonce": lignes, "quantite_totale_annoncee": null
    })
}

/// Dépose le BL et le fait lire ; rend l'id du document.
async fn deposer_et_lire(b: &Banc, pdf: &[u8]) -> i64 {
    let a = b.societe("Magasin Frouard").await;
    let m = mail(
        b,
        a,
        &fixtures::eml(
            "bl400@test",
            "BL BL-400",
            &[("bl.pdf", "application/pdf", pdf)],
            false,
        ),
    )
    .await;
    // Document créé par la pompe, lu directement (sans délai d'attente : l'OCR est lent).
    gmflow_worker::lecture::pompe(b.pool()).await.unwrap();
    let id = sqlx::query_scalar("select id from document where mail_id = $1")
        .bind(m)
        .fetch_one(b.pool())
        .await
        .unwrap();
    gmflow_worker::lecture::lire(&b.ctx, id, false, false, false)
        .await
        .unwrap();
    id
}

#[derive(Debug, sqlx::FromRow)]
struct Doc {
    etat: String,
    message: Option<String>,
    montant_ht: Option<Decimal>,
    confiance: Value,
}

async fn doc(b: &Banc, id: i64) -> Doc {
    sqlx::query_as("select etat, message, montant_ht, confiance from document where id = $1")
        .bind(id)
        .fetch_one(b.pool())
        .await
        .unwrap()
}

async fn references(b: &Banc, id: i64) -> Vec<String> {
    sqlx::query_scalar("select reference from ligne_document where document_id = $1 order by rang")
        .bind(id)
        .fetch_all(b.pool())
        .await
        .unwrap()
}

async fn appels(b: &Banc, tache: &str) -> i64 {
    b.compter(&format!(
        "select count(*) from appel_claude where tache = '{tache}' and succes"
    ))
    .await
}

fn attendues(n: usize, sauf: std::ops::Range<usize>) -> Vec<String> {
    (0..n)
        .filter(|i| !sauf.contains(i))
        .map(|i| format!("REF{i:04}"))
        .collect()
}

#[tokio::test]
async fn bl_de_400_lignes_lu_en_tranches_toutes_les_lignes_une_fois() {
    let (pdf, pages, ht) = bl(400, 30);
    assert_eq!(pages, 14);
    let faux = FauxTranches::new(&entete(ht, 400));
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.etat, "lu", "{d:?}");
    assert_eq!(d.message, None, "tout est lu et contrôlé : {d:?}");
    assert_eq!(d.montant_ht, Some(ht));
    assert!(
        d.confiance.get("lignes_tronquees").is_none(),
        "{}",
        d.confiance
    );
    let t = &d.confiance["tranches"];
    assert_eq!(t["complet"], json!(true));
    assert_eq!(t["verifie"], json!(["somme_ht", "nombre_lignes"]));
    // 33 lignes de texte par page : 3 pages par tranche, 5 tranches + l'en-tête.
    assert_eq!(t["parties"], json!(6), "{t}");
    // Le faux recopie aussi la première ligne de la page suivante : 4 doublons retirés.
    assert_eq!(t["doublons_retires"], json!(4));
    assert_eq!(references(&b, id).await, attendues(400, 0..0));
    // Modèle le moins cher partout, une tranche après l'autre, chacune une seule fois.
    assert_eq!(
        faux.journal(),
        [
            "entete haiku",
            "tranche 1 haiku",
            "tranche 4 haiku",
            "tranche 7 haiku",
            "tranche 10 haiku",
            "tranche 13 haiku"
        ]
    );
    assert_eq!(appels(&b, "extraction_texte").await, 1);
    assert_eq!(appels(&b, "lignes_tranche").await, 5);
    // Coût mesuré pour chaque appel, compté dans le quota du jour.
    assert_eq!(
        b.compter(
            "select count(*) from appel_claude where document_id is not null and cout_usd > 0"
        )
        .await,
        6
    );
    assert_eq!(reglages::appels_lecture_du_jour(b.pool()).await.unwrap(), 6);
    // Avancement effacé à la fin ; contrôles du BL sans « lecture incomplète ».
    assert_eq!(b.compter("select count(*) from lecture_tranche").await, 0);
    let controles: Value = sqlx::query_scalar("select controles from document where id = $1")
        .bind(id)
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert!(
        !controles.to_string().contains("lignes_tronquees"),
        "{controles}"
    );
    b.fermer().await;
}

#[tokio::test]
async fn tranche_en_echec_reessayee_seule_sans_perdre_les_autres() {
    let (pdf, _, ht) = bl(400, 30);
    let faux = FauxTranches::new(&entete(ht, 400));
    faux.echec("4 haiku\n");
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(
        (d.etat.as_str(), d.message.as_deref()),
        ("lu", None),
        "{d:?}"
    );
    assert_eq!(references(&b, id).await, attendues(400, 0..0));
    let j = faux.journal();
    assert_eq!(
        j.iter().filter(|l| l.starts_with("tranche 4 ")).count(),
        2,
        "{j:?}"
    );
    assert!(j.contains(&"tranche 4 sonnet pdf".to_string()));
    assert_eq!(j.iter().filter(|l| l.starts_with("tranche 7 ")).count(), 1);
    b.fermer().await;
}

#[tokio::test]
async fn tranche_toujours_en_echec_lecture_incomplete_avec_les_pages() {
    let (pdf, _, ht) = bl(400, 30);
    let faux = FauxTranches::new(&entete(ht, 400));
    faux.echec("4 *\n");
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.etat, "lu");
    assert_eq!(d.confiance["lignes_tronquees"], json!(true));
    let m = d.message.unwrap();
    assert!(
        m.starts_with("Lecture à vérifier : lecture incomplète (document lu en 6 parties) : pages 4 à 6 non lues"),
        "{m}"
    );
    // Les autres tranches sont gardées : 400 lignes moins les 90 des pages 4 à 6 (la première
    // ligne de la page 4, recopiée en trop par la tranche 1, reste : rien pour la dédoublonner).
    assert_eq!(references(&b, id).await, attendues(400, 91..180));
    assert_eq!(d.confiance["tranches"]["pages_non_lues"], json!([4, 5, 6]));
    b.fermer().await;
}

#[tokio::test]
async fn somme_des_lignes_fausse_lecture_incomplete() {
    let (pdf, _, ht) = bl(400, 30);
    // Total HT lu différent de la somme des lignes, et un nombre de lignes annoncé faux.
    let faux = FauxTranches::new(&entete(ht + Decimal::TEN, 401));
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.confiance["lignes_tronquees"], json!(true));
    let m = d.message.unwrap();
    assert!(m.contains("la somme des lignes"), "{m}");
    assert!(
        m.contains("le document annonce 401 lignes, 400 lues"),
        "{m}"
    );
    // L'en-tête a été relu une fois avec le modèle du nouvel essai, PDF joint.
    assert!(faux.journal().contains(&"entete sonnet pdf".to_string()));
    b.fermer().await;
}

#[tokio::test]
async fn plafond_du_nombre_de_tranches() {
    // 64 pages de 10 lignes : 4 pages par tranche, 16 tranches pour un plafond de 15.
    let (pdf, pages, ht) = bl(640, 10);
    assert_eq!((pages, TRANCHES_MAX), (64, 15));
    let faux = FauxTranches::new(&entete(ht, 640));
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.confiance["lignes_tronquees"], json!(true));
    assert!(d.message.unwrap().contains("pages 61 à 64 non lues"));
    assert_eq!(appels(&b, "lignes_tranche").await, 15);
    // Plus la première ligne de la page 61, recopiée en trop par la dernière tranche lue.
    assert_eq!(references(&b, id).await, attendues(601, 0..0));
    b.fermer().await;
}

#[tokio::test]
async fn quota_atteint_en_cours_de_route_reprise_sans_tout_relire() {
    let (pdf, _, ht) = bl(400, 30);
    let faux = FauxTranches::new(&entete(ht, 400));
    let b = banc(&faux).await;
    // En-tête + 2 tranches, puis la limite du jour.
    quota(&b, 3).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.etat, "a_lire", "{d:?}");
    let m = d.message.unwrap();
    assert!(
        m.starts_with(
            "Lecture en 6 parties : 3 déjà lues, gardées pour la suite. Limite du jour atteinte"
        ),
        "{m}"
    );
    assert_eq!(
        b.compter("select count(*) from lecture_tranche where etat = 'lue'")
            .await,
        3
    );
    // Avancement montré pendant la lecture.
    sqlx::query("update document set etat = 'en_cours', pris_le = now() where id = $1")
        .bind(id)
        .execute(b.pool())
        .await
        .unwrap();
    let a = gmflow_core::attente::expliquer(b.pool(), id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        a.raison.starts_with("Lecture en cours, partie 4 sur 6"),
        "{}",
        a.raison
    );
    sqlx::query("update document set etat = 'a_lire', pris_le = null where id = $1")
        .bind(id)
        .execute(b.pool())
        .await
        .unwrap();
    // La limite relevée, la tâche relancée ne relit que les trois dernières tranches.
    quota(&b, 1000).await;
    faux.vider_journal();
    gmflow_worker::lecture::lire(&b.ctx, id, false, false, false)
        .await
        .unwrap();
    assert_eq!(
        faux.journal(),
        ["tranche 7 haiku", "tranche 10 haiku", "tranche 13 haiku"]
    );
    let d = doc(&b, id).await;
    assert_eq!(
        (d.etat.as_str(), d.message.as_deref()),
        ("lu", None),
        "{d:?}"
    );
    assert_eq!(references(&b, id).await, attendues(400, 0..0));
    assert_eq!(b.compter("select count(*) from lecture_tranche").await, 0);
    b.fermer().await;
}

#[tokio::test]
async fn relecture_en_detail_relit_tout_avec_le_modele_du_nouvel_essai() {
    let (pdf, _, ht) = bl(400, 30);
    let faux = FauxTranches::new(&entete(ht, 400));
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    faux.vider_journal();
    sqlx::query("update document set etat = 'a_lire' where id = $1")
        .bind(id)
        .execute(b.pool())
        .await
        .unwrap();
    gmflow_worker::lecture::lire(&b.ctx, id, true, true, true)
        .await
        .unwrap();
    let j = faux.journal();
    assert_eq!(j.len(), 6, "{j:?}");
    assert!(j.iter().all(|l| l.ends_with(" sonnet pdf")), "{j:?}");
    assert_eq!(references(&b, id).await, attendues(400, 0..0));
    b.fermer().await;
}

#[tokio::test]
async fn petit_bl_lu_en_un_seul_appel() {
    let (pdf, _, ht) = bl(40, 30);
    let faux = FauxTranches::new(&entete(ht, 40));
    let b = banc(&faux).await;
    deposer_et_lire(&b, &pdf).await;
    assert!(
        faux.journal().is_empty(),
        "pas de tranches : {:?}",
        faux.journal()
    );
    assert_eq!(appels(&b, "lignes_tranche").await, 0);
    b.fermer().await;
}

#[tokio::test]
async fn scan_lu_par_l_ocr_en_tranches_pdf_joint_a_chaque_appel() {
    if !gmflow_core::lecture::ocr::disponible(&gmflow_core::lecture::ocr::Outils::default()).await {
        eprintln!("OCR absent : test sauté");
        return;
    }
    // 5 pages scannées de 45 lignes : plus de 200 lignes chiffrées après l'OCR.
    let mut pages = Vec::new();
    for p in 0..5 {
        let mut l = vec![format!("BON DE LIVRAISON BL-400 Page {}/5", p + 1)];
        for i in p * 45..(p + 1) * 45 {
            l.push(format!("REF{i:04} Article {i} 2 1,50 3,00"));
        }
        let l: Vec<&str> = l.iter().map(String::as_str).collect();
        let Some(page) = fixtures::page_scannee(&l) else {
            eprintln!("pdftoppm absent : test sauté");
            return;
        };
        pages.push(page);
    }
    let pdf = fixtures::pdf_pages(pages);
    let faux = FauxTranches::new(&entete(Decimal::from(675), 225));
    let b = banc(&faux).await;
    let id = deposer_et_lire(&b, &pdf).await;
    let d = doc(&b, id).await;
    assert_eq!(d.etat, "lu", "{d:?}");
    assert_eq!(d.confiance["texte_source"], json!("ocr_local"));
    assert!(
        d.confiance["tranches"]["parties"].as_i64().unwrap() >= 3,
        "{}",
        d.confiance
    );
    let j = faux.journal();
    // Le scan fait foi : le PDF est joint à l'en-tête et à chaque tranche.
    assert!(
        j.len() >= 3 && j.iter().all(|l| l.ends_with(" pdf")),
        "{j:?}"
    );
    assert!(j[0].starts_with("entete haiku"), "{j:?}");
    // Les lignes viennent du texte de l'OCR, page par page : la plupart sont reconnues.
    assert!(references(&b, id).await.len() >= 200);
    b.fermer().await;
}
