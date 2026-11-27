//! Formes des colonnes `jsonb` (et des objets construits en SQL) que l'API rend telles quelles,
//! pour les types TypeScript du front (fonction `ts` seulement). Le serveur ne construit pas ces
//! structures : chacune dit qui écrit la valeur décrite, et doit changer avec elle.

use gmflow_core::vocabulaire as v;
use ts_rs::TS;

/// `document.confiance`, écrit par la lecture (`extraction::Confiance`, de 0 à 1), puis
/// complété par le worker (`lecture::enregistrer`, OCR, tranches). Chaque clé peut manquer :
/// `{}` pour un document pas encore lu.
#[derive(TS)]
#[ts(export, rename = "Confiance", optional_fields)]
pub struct Confiance {
    pub r#type: Option<f64>,
    pub societe: Option<f64>,
    pub fournisseur: Option<f64>,
    pub numero: Option<f64>,
    pub date: Option<f64>,
    pub montants: Option<f64>,
    /// BL sans totaux imprimés : montants calculés depuis les lignes.
    #[ts(as = "Option<Vec<v::MontantCalcule>>")]
    pub montants_calcules: Option<Vec<String>>,
    /// Lignes articles coupées à `extraction::LIGNES_MAX`, ou lecture par tranches incomplète.
    pub lignes_tronquees: Option<bool>,
    /// Société reconnue par une règle du worker (`fournisseur::rattacher`).
    pub societe_par_regle: Option<bool>,
    /// Pièce trop volumineuse pour être lue (`limites::Depassement::code`) : pas lue, à
    /// contrôler à la main (`document.message` dit pourquoi).
    #[ts(as = "Option<v::Depassement>")]
    pub trop_volumineux: Option<String>,
    /// Fichier découpé en plusieurs pièces (`decoupage::Plage`, pages comprises).
    pub decoupage: Option<Vec<PlageDecoupage>>,
    /// Bilan de la lecture par tranches (`tranches::CLE_CONFIANCE`).
    pub tranches: Option<BilanTranches>,
    /// Texte lu par l'OCR local (`ocr::TEXTE_SOURCE`).
    #[ts(type = "'ocr_local'")]
    pub texte_source: Option<String>,
    pub pages_ocr: Option<Vec<i64>>,
    pub ocr_partiel: Option<bool>,
    /// Lignes lues dans les cellules du tableur (`tableau_articles::LIGNES_SOURCE`).
    #[ts(type = "'cellules'")]
    pub lignes_source: Option<String>,
    #[ts(type = "'entete' | 'apprise' | 'claude'")]
    pub colonnes_source: Option<String>,
    pub cellules_en_erreur: Option<i64>,
}

/// Une pièce d'un fichier découpé (`decoupage::Plage`).
#[derive(TS)]
#[ts(export, rename = "PlageDecoupage")]
pub struct PlageDecoupage {
    pub debut: i64,
    pub fin: i64,
    #[ts(as = "Option<v::TypeDocument>")]
    pub r#type: Option<String>,
}

/// Bilan de la lecture par tranches d'un très gros PDF (`lecture::tranches`, worker).
#[derive(TS)]
#[ts(export, rename = "BilanTranches")]
pub struct BilanTranches {
    pub parties: i64,
    pub pages: i64,
    pub lues: i64,
    pub pages_non_lues: Vec<i64>,
    pub doublons_retires: i64,
    pub complet: bool,
    /// Contrôles réussis : `somme_ht`, `nombre_lignes`, `quantite_totale`.
    #[ts(type = "Array<'somme_ht' | 'nombre_lignes' | 'quantite_totale'>")]
    pub verifie: Vec<String>,
    pub raisons: Vec<String>,
}

/// `document.ecartement` sans sa clé `avant` (`retour::noter_ecartement`).
#[derive(TS)]
#[ts(export, rename = "Ecartement")]
pub struct Ecartement {
    pub le: String,
    pub par: Option<i64>,
    pub par_nom: Option<String>,
    #[ts(as = "Option<v::MotifEcartement>")]
    pub motif: Option<String>,
    pub texte: Option<String>,
}

/// Dernier report d'une facture dans Logiflow (`logiflow_rapprochement`, objet construit par
/// `documents::fiche` ; `livraisons` écrit par le worker).
#[derive(TS)]
#[ts(export, rename = "ResultatLogiflow")]
pub struct ResultatLogiflow {
    pub quand: String,
    pub ok: bool,
    pub message: String,
    pub livraisons: Vec<gmflow_core::logiflow::LigneResultat>,
}

/// Livraison d'un BL retrouvée dans Logiflow (`logiflow_livraison`, objet construit par
/// `documents::fiche`).
#[derive(TS)]
#[ts(export, rename = "LivraisonLogiflow")]
pub struct LivraisonLogiflow {
    pub vu_le: String,
    #[ts(as = "v::EtatSuiviLogiflow")]
    pub etat: String,
    pub possibles: i32,
    pub livraison_id: Option<i64>,
    pub magasin: Option<String>,
    pub fournisseur: Option<String>,
    pub bl: Option<String>,
    /// AAAA-MM-JJ.
    pub livree_le: Option<String>,
    pub montant_bl: Option<f64>,
    pub facture: Option<String>,
    pub facture_ht: Option<f64>,
    pub facture_ttc: Option<f64>,
    /// AAAA-MM-JJ.
    pub echeance: Option<String>,
    pub rapprochee: bool,
}

/// Dernier test d'une boîte (`boite_mail.dernier_test`) : écrit par le worker
/// (`test_boite::tester`) ou repris d'un essai de l'assistant (`admin_boites`, avec `essai_id`).
#[derive(TS)]
#[ts(export, rename = "TestBoite", optional_fields)]
pub struct TestBoite {
    pub quand: Option<String>,
    #[ts(optional = false)]
    pub ok: bool,
    /// Phrase en clair (« Mot de passe refusé : … », « Connexion réussie : 7 dossiers trouvés. »).
    pub message: Option<String>,
    /// Erreur brute du serveur.
    #[ts(optional = nullable)]
    pub detail: Option<String>,
    /// Noms IMAP, à enregistrer tels quels.
    pub dossiers: Option<Vec<String>>,
    /// Noms décodés et attributs (tests faits depuis la refonte ; absent avant).
    pub dossiers_detail: Option<Vec<gmflow_core::imap::Dossier>>,
    /// Dossier des envoyés repéré (nom IMAP), null si rien de sûr.
    #[ts(optional = nullable)]
    pub envoyes: Option<String>,
    /// Création depuis un essai de l'assistant.
    pub essai_id: Option<String>,
}

/// Dernier test d'un serveur (paramètres `expedition_test`, `serveur_api_test`, `logiflow_test`,
/// écrits par le worker).
#[derive(TS)]
#[ts(export, rename = "DernierTest")]
pub struct DernierTest {
    pub quand: String,
    pub ok: bool,
    pub message: Option<String>,
}

/// Dernière copie des fournisseurs du magasin (paramètre `serveur_api_synchro`, worker).
#[derive(TS)]
#[ts(export, rename = "SynchroMagasin")]
pub struct SynchroMagasin {
    pub quand: String,
    pub ok: bool,
    pub message: Option<String>,
    pub nombre: Option<i64>,
}

/// Dernière vérification Logiflow (paramètre `logiflow_suivi`) ; `ok` null : en cours.
#[derive(TS)]
#[ts(export, rename = "SuiviLogiflow")]
pub struct SuiviLogiflow {
    pub quand: String,
    pub ok: Option<bool>,
    pub message: Option<String>,
}
