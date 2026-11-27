//! Valeurs possibles des champs texte de l'API, pour les types TypeScript du front (fonction `ts`).
//!
//! L'API sérialise ces champs en `String` (lus tels quels dans la base) ; ces énumérations ne
//! servent qu'à décrire leurs valeurs côté front : un champ `statut_depot: Option<String>` porte
//! `#[cfg_attr(feature = "ts", ts(as = "Option<v::StatutDepot>"))]`. Celles qui suivent une
//! contrainte `check (x in (...))` de la base sont comparées à elle par un test
//! (`vocabulaire_comme_la_base`) : ajouter une valeur dans une migration oblige à l'ajouter ici.

use ts_rs::TS;

macro_rules! vocabulaire {
    ($($(#[$doc:meta])* $nom:ident { $($variante:ident),+ $(,)? })+) => {
        $(
            $(#[$doc])*
            #[derive(TS)]
            #[ts(export, rename_all = "snake_case")]
            pub enum $nom { $($variante),+ }
        )+
    };
}

vocabulaire! {
    /// `utilisateur.role`.
    Role { Admin, Responsable }
    /// `mail.categorie` (tri des mails).
    Categorie { Question, Information, DocumentSeul, Publicite, Notification }
    /// `mail.categorie_par` : qui a classé le mail.
    CategoriePar { Regle, Claude, Humain }
    /// `mail.priorite`.
    Priorite { Haute, Normale, Basse }
    /// `mail.statut_reponse`.
    StatutReponse { ARepondre, Repondu, SansSuite }
    /// `mail.tri_etat`.
    TriEtat { ATrier, EnCours, Trie, Erreur }
    /// `brouillon_reponse.statut`.
    StatutBrouillon { Generation, Propose, Modifie, Envoi, Envoye, Abandonne }
    /// `document.type`.
    TypeDocument { Facture, Avoir, Bl, Relance, Devis, Autre }
    /// Types de document qu'on valide (file « À valider », fil d'un fournisseur).
    TypePiece { Facture, Avoir, Bl }
    /// `document.etat` (lecture).
    EtatDocument { ALire, EnCours, Lu, Ignore, Erreur }
    /// `document.voie` (`lecture::Voie`).
    Voie { Facturx, Xml, PdfTexte, Tableur, Scan }
    /// `document.statut_depot`.
    StatutDepot { AVerifier, Pret, EnEnvoi, Depose, Erreur, Doublon }
    /// `envoi_depot.etat`.
    EtatEnvoi { EnCours, Envoye, Erreur }
    /// `societe.outil_depot`.
    OutilDepot { Dext, Yooz }
    /// `fournisseur.statut`.
    StatutFournisseur { AConfirmer, Valide }
    /// `fournisseur.colonnes_export` (null : les deux).
    ColonnesExport { Ref, Gencod }
    /// `rapprochement.methode` (`rapprochement::Methode`).
    MethodeRapprochement { Commande, NumeroBl, FactureCitee, Manuel }
    /// `boite_mail.smtp_securite` (`clair` : serveur de test seulement).
    SecuriteSmtp { Tls, Starttls, Clair }
    /// `fournisseur_iban.origine`.
    OrigineIban { Validation, Confirmation }
    /// `seance_claude.etat`.
    EtatSeance { Demarrage, AttenteCode, CodeRecu, Terminee, Erreur, Expiree }
    /// Accès à Claude (`parametre::CLAUDE_ACCES`) : setup token ou clé API.
    ModeAcces { Jeton, CleApi }
    /// Champs visés par un contrôle (`controles::Echec::champs`).
    ChampControle { Numero, DateDocument, MontantHt, MontantTva, MontantTtc, SocieteId, FournisseurId }
    /// Pourquoi un document attend sa lecture (`attente::Attente::genre`).
    GenreAttente {
        EnCours, EnFile, Bloque, FileArretee, JetonAbsent, ClaudeRefuse, Quota, LectureCoupee,
        ARelancer,
    }
    /// Section « Bloqué » de l'accueil (`a_traiter::Bloque::genre`).
    GenreBloque { EnvoiErreur, Doublon, Illisible, Boite, Claude }
    /// Montants d'un BL calculés depuis ses lignes (`confiance.montants_calcules`).
    MontantCalcule { Ht, Tva, Ttc }
    /// Écart d'article entre une facture et ses BL (`rapprochement::comparer`).
    GenreEcart { Manquant, EnTrop, Prix }
    /// Motif d'un écartement par un humain.
    MotifEcartement { Doublon, PasPourNous, Publicite, Autre }
    /// Report d'une livraison dans Logiflow (`logiflow::LigneResultat::etat`).
    EtatLivraisonLogiflow {
        Complete, DejaFait, DejaRapproche, AutreFacture, Introuvable, Ambigu, Erreur,
    }
    /// Livraison d'un BL retrouvée dans Logiflow (`logiflow_livraison.etat`).
    EtatSuiviLogiflow { Trouve, Ambigu }
    /// Pièce trop volumineuse (`lecture::limites::Depassement::code`).
    Depassement { Taille, Decompresse, EntreesZip, Cellules, Pages, Objets, Duree, Structure }
    /// Étapes de la mise en route.
    CleEtape { Societes, Boites, Claude, Envoi, Utilisateurs }
    /// Étapes de Réglages > Traitements.
    CleEtapeTraitement { Releve, Lecture, Tri, Rapprochement, Depot }
    /// État d'une étape de Réglages > Traitements.
    EtatTraitement { Ok, EnRetard, Bloque, Arrete }
}
