//! OCR local des pages scannées d'un PDF (L20, L21) : `pdftoppm` (poppler-utils) rend la page
//! en image, `tesseract` (langue `fra`) la lit. Le texte obtenu sert de texte du document
//! (contrôle des montants, recherche) ; le PDF reste joint à Claude, qui fait foi.
//!
//! Le texte est mesuré page par page : un PDF mi-texte mi-scan ne passe à l'OCR que ses pages
//! sans couche texte. Outils absents, en échec ou texte trop pauvre : rien, la lecture se fait
//! comme avant.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Caractères utiles (lettres, chiffres) en dessous desquels une page n'a pas de couche texte.
pub const PAGE_TEXTE_MIN: usize = 40;
/// Pages lues par l'OCR au plus : les premières et les dernières (totaux), le milieu est sauté.
pub const PAGES_MAX: usize = 12;
const PAGES_DEBUT: usize = 8;
/// Délai d'une commande (rendu ou lecture d'une page) et de tout l'OCR d'un document.
const DELAI_COMMANDE: Duration = Duration::from_secs(60);
const DELAI_TOTAL: Duration = Duration::from_secs(180);
/// Résolution du rendu des pages (points par pouce).
const RESOLUTION: &str = "300";

/// Clé de `document.confiance` : d'où vient le texte du document (« ocr_local »).
pub const TEXTE_SOURCE: &str = "texte_source";

/// Commandes utilisées (`GMFLOW_PDFTOPPM`, `GMFLOW_TESSERACT`, sinon dans le PATH).
#[derive(Debug, Clone)]
pub struct Outils {
    pub pdftoppm: String,
    pub tesseract: String,
}

impl Default for Outils {
    fn default() -> Self {
        Self {
            pdftoppm: std::env::var("GMFLOW_PDFTOPPM").unwrap_or_else(|_| "pdftoppm".into()),
            tesseract: std::env::var("GMFLOW_TESSERACT").unwrap_or_else(|_| "tesseract".into()),
        }
    }
}

fn utile(t: &str) -> usize {
    t.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Pages (numérotées à partir de 1) sans couche texte, d'après le texte de chaque page.
pub fn pages_sans_texte(textes: &[String]) -> Vec<usize> {
    textes
        .iter()
        .enumerate()
        .filter(|(_, t)| utile(t) < PAGE_TEXTE_MIN)
        .map(|(i, _)| i + 1)
        .collect()
}

/// Pages à passer à l'OCR : toutes jusqu'à `PAGES_MAX`, sinon les premières et les dernières.
pub fn pages_a_lire(sans_texte: &[usize]) -> Vec<usize> {
    if sans_texte.len() <= PAGES_MAX {
        return sans_texte.to_vec();
    }
    let fin = PAGES_MAX - PAGES_DEBUT;
    let mut v = sans_texte[..PAGES_DEBUT].to_vec();
    v.extend_from_slice(&sans_texte[sans_texte.len() - fin..]);
    v
}

/// Texte d'un PDF complété par l'OCR local.
#[derive(Debug, Clone, PartialEq)]
pub struct TexteOcr {
    /// Texte de toutes les pages, dans l'ordre (couche texte ou OCR).
    pub texte: String,
    /// Pages lues par l'OCR (à partir de 1).
    pub pages_ocr: Vec<usize>,
    /// Pages du PDF.
    pub pages: usize,
    /// Des pages sans texte n'ont pas été lues (au-delà de `PAGES_MAX`, délai, échec).
    pub partiel: bool,
}

impl TexteOcr {
    /// Marques de `document.confiance`.
    pub fn confiance(&self) -> Value {
        json!({
            TEXTE_SOURCE: "ocr_local",
            "pages_ocr": self.pages_ocr,
            "ocr_partiel": self.partiel,
        })
    }

    /// Consigne ajoutée au prompt : le texte vient de l'OCR, le fichier joint fait foi.
    pub fn consigne(&self, fichier: &str) -> String {
        let mut s = format!(
            "\nLe texte ci-dessus a été obtenu en partie par reconnaissance de caractères (OCR) \
             des pages scannées : il peut contenir des erreurs (chiffres ou lettres confondus, \
             colonnes mélangées). Le document d'origine est le fichier {fichier} du dossier \
             courant : lis-le avec l'outil Read et, en cas de doute, fie-toi au fichier plutôt \
             qu'au texte, surtout pour les montants, les numéros et les dates. Ne donne pas de \
             transcription.\n"
        );
        if self.pages > 1 {
            s.push_str(&super::extraction::lire_les_pages(self.pages));
        }
        s
    }
}

/// Les outils sont-ils là (tesseract avec la langue `fra`, pdftoppm) ?
pub async fn disponible(o: &Outils) -> bool {
    let langues = commande(&o.tesseract, &["--list-langs"], None).await;
    let fra = langues.is_some_and(|s| s.lines().any(|l| l.trim() == "fra"));
    fra && commande(&o.pdftoppm, &["-v"], None).await.is_some()
}

/// Lance une commande avec un délai ; sa sortie standard (et d'erreur, `pdftoppm -v` écrit
/// là), ou rien si elle est absente, en échec ou trop lente.
async fn commande(bin: &str, args: &[&str], dossier: Option<&Path>) -> Option<String> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Some(d) = dossier {
        cmd.current_dir(d);
    }
    let sortie = tokio::time::timeout(DELAI_COMMANDE, cmd.output())
        .await
        .ok()?
        .ok()?;
    if !sortie.status.success() {
        return None;
    }
    let mut s = String::from_utf8_lossy(&sortie.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&sortie.stderr));
    Some(s)
}

/// Lit une page par l'OCR (rendu en PNG puis tesseract) ; rien en cas d'échec.
async fn lire_page(o: &Outils, dossier: &Path, page: usize) -> Option<String> {
    let p = page.to_string();
    let base = format!("page{page}");
    commande(
        &o.pdftoppm,
        &[
            "-f",
            &p,
            "-l",
            &p,
            "-r",
            RESOLUTION,
            "-gray",
            "-png",
            "-singlefile",
            "document.pdf",
            &base,
        ],
        Some(dossier),
    )
    .await?;
    let png = format!("{base}.png");
    let mut cmd = tokio::process::Command::new(&o.tesseract);
    cmd.args([png.as_str(), "stdout", "-l", "fra", "--psm", "4"])
        .current_dir(dossier)
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let sortie = tokio::time::timeout(DELAI_COMMANDE, cmd.output())
        .await
        .ok()?
        .ok()?;
    let _ = tokio::fs::remove_file(dossier.join(&png)).await;
    sortie
        .status
        .success()
        .then(|| String::from_utf8_lossy(&sortie.stdout).into_owned())
}

/// Texte du PDF complété par l'OCR de ses pages sans couche texte. Rien si toutes les pages ont
/// du texte, si les outils manquent, ou si l'OCR ne rend presque rien (moins de
/// `TEXTE_UTILE_MIN` caractères utiles sur ses pages) : la lecture se fait alors comme avant.
pub async fn completer(o: &Outils, pdf: &[u8]) -> Option<TexteOcr> {
    let octets = pdf.to_vec();
    let textes = tokio::task::spawn_blocking(move || super::pdf::texte_par_page(&octets))
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let pages = super::pdf::pages(pdf);
    if pages == 0 {
        return None;
    }
    // Couche texte illisible : toutes les pages sont à lire.
    let textes = if textes.len() == pages {
        textes
    } else {
        vec![String::new(); pages]
    };
    let sans_texte = pages_sans_texte(&textes);
    if sans_texte.is_empty() || !disponible(o).await {
        return None;
    }
    let dossier = crate::claude::DossierTemp::creer("gmflow-ocr").ok()?;
    tokio::fs::write(dossier.0.join("document.pdf"), pdf)
        .await
        .ok()?;
    let a_lire = pages_a_lire(&sans_texte);
    let mut partiel = a_lire.len() < sans_texte.len();
    let debut = Instant::now();
    let mut lus: Vec<(usize, String)> = Vec::new();
    for p in a_lire {
        if debut.elapsed() > DELAI_TOTAL {
            partiel = true;
            break;
        }
        match lire_page(o, &dossier.0, p).await {
            Some(t) => lus.push((p, t)),
            None => partiel = true,
        }
    }
    if lus.iter().map(|(_, t)| utile(t)).sum::<usize>() < super::TEXTE_UTILE_MIN {
        tracing::info!(pages = ?sans_texte, "OCR local sans texte exploitable");
        return None;
    }
    let mut texte = String::new();
    for (i, t) in textes.iter().enumerate() {
        let n = i + 1;
        let page = lus
            .iter()
            .find(|(p, _)| *p == n)
            .map_or(t.as_str(), |(_, t)| t.as_str());
        if !texte.is_empty() {
            texte.push_str("\n\n");
        }
        texte.push_str(page.trim_end());
    }
    Some(TexteOcr {
        texte,
        pages_ocr: lus.into_iter().map(|(p, _)| p).collect(),
        pages,
        partiel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_mesurees_une_a_une() {
        let textes = vec![
            "FACTURE 12 Total 60,00 Magasin Frouard, zone commerciale".to_string(),
            String::new(),
            " 3 ".to_string(),
        ];
        assert_eq!(pages_sans_texte(&textes), vec![2, 3]);
        let v: Vec<usize> = (1..=30).collect();
        let a = pages_a_lire(&v);
        assert_eq!(a.len(), PAGES_MAX);
        assert_eq!(a[..3], [1, 2, 3]);
        assert_eq!(*a.last().unwrap(), 30);
    }

    #[tokio::test]
    async fn outils_absents_rien() {
        let o = Outils {
            pdftoppm: "/inexistant/pdftoppm".into(),
            tesseract: "/inexistant/tesseract".into(),
        };
        assert!(!disponible(&o).await);
        // PDF d'une page blanche : à passer à l'OCR, mais les outils manquent.
        use lopdf::{Document, Object, dictionary};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
            ),
        );
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", cat);
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();
        assert_eq!(crate::lecture::pdf::pages(&pdf), 1);
        assert_eq!(completer(&o, &pdf).await, None);
    }
}
