export type Role = 'admin' | 'responsable';

export interface Moi {
  id: number;
  identifiant: string;
  role: Role;
  societe_id: number | null;
}

export interface Page<T> {
  total: number;
  page: number;
  elements: T[];
}

export interface SocieteVisible {
  id: number;
  raison_sociale: string;
}

export interface BoiteVisible {
  id: number;
  libelle: string;
  societe_id: number;
}

export interface LigneMail {
  id: number;
  expediteur: string;
  objet: string;
  date_mail: string | null;
  nb_pieces: number;
  boites: string[];
  categorie: Categorie | null;
  statut_reponse: StatutReponse | null;
}

export type Categorie = 'question' | 'information' | 'document_seul' | 'publicite' | 'notification';
export type Priorite = 'haute' | 'normale' | 'basse';
export type StatutReponse = 'a_repondre' | 'repondu' | 'sans_suite';
export type StatutBrouillon = 'generation' | 'propose' | 'modifie' | 'envoi' | 'envoye' | 'abandonne';

export interface Brouillon {
  id: number;
  boite_id: number;
  boite: string;
  texte: string;
  statut: StatutBrouillon;
  modele: string | null;
  effort: string | null;
  erreur: string | null;
  cree_le: string;
  modifie_le: string;
  envoye_le: string | null;
  envoye_par: string | null;
  smtp: boolean;
}

export interface LigneARepondre {
  id: number;
  expediteur: string;
  objet: string;
  date_mail: string | null;
  resume: string | null;
  priorite: Priorite | null;
  categorie_par: string | null;
  structures: string[];
  boites: string[];
  brouillon: StatutBrouillon | null;
}

export interface SemaineTri {
  debut: string;
  fin: string;
  par_categorie: { cle: Categorie | null; n: number }[];
  a_repondre: number;
  repondu: number;
  sans_suite: number;
  revus: number;
  corriges: number;
}

export interface ExpediteurMarque {
  id: number;
  adresse: string | null;
  domaine: string | null;
  cree_le: string;
  cree_par: string | null;
}

export interface FicheMail {
  id: number;
  message_id: string;
  expediteur: string;
  destinataires: string[];
  objet: string;
  date_mail: string | null;
  corps_texte: string;
  pieces: {
    id: number;
    nom: string;
    type_mime: string;
    taille: number;
    document_id: number | null;
    document_etat: EtatDocument | null;
    document_type: TypeDocument | null;
    document_voie: Voie | null;
  }[];
  boites: { boite_id: number; libelle: string; dossier: string }[];
  categorie: Categorie | null;
  categorie_par: 'regle' | 'claude' | 'humain' | null;
  resume: string | null;
  priorite: Priorite | null;
  statut_reponse: StatutReponse | null;
  tri_etat: 'a_trier' | 'en_cours' | 'trie' | 'erreur' | null;
  tri_message: string | null;
  brouillons: Brouillon[];
}

export interface Tableau {
  mails_du_jour: { societe_id: number; raison_sociale: string; n: number }[];
  boites_alerte: { id: number; libelle: string; motif: 'erreur' | 'pause_auto'; depuis: string | null; message: string | null }[];
  claude?: { etat: string; teste_le?: string | null };
  stockage?: { octets: number; libre_pourcent: number | null };
  documents: { a_lire: number; erreur: number; a_verifier: number; lus_aujourdhui: number };
  depots: { a_verifier: number; prets: number; bloques: number; deposes_mois: number };
  lecture?: { auto: boolean; quota: number; appels_du_jour: number };
  a_repondre: number;
  tri?: { auto: boolean; quota: number; appels_du_jour: number; en_attente: number; en_erreur: number };
}

export interface Societe {
  id: number;
  raison_sociale: string;
  siret: string | null;
  tva: string | null;
  adresses: string[];
  outil_depot: 'dext' | 'yooz' | null;
  adresse_depot: string | null;
  depot_auto: boolean;
  archivee: boolean;
}

export interface TestBoite {
  quand?: string;
  ok: boolean;
  message?: string;
  dossiers?: string[];
}

export interface Boite {
  id: number;
  societe_id: number;
  libelle: string;
  serveur: string;
  port: number;
  identifiant: string;
  dossiers: string[];
  reprise_mois: number;
  active: boolean;
  pause_auto: boolean;
  echecs_auth: number;
  derniere_releve: string | null;
  derniere_erreur: string | null;
  erreur_depuis: string | null;
  dernier_test: TestBoite | null;
  smtp_serveur: string | null;
  smtp_port: number | null;
  smtp_securite: 'tls' | 'starttls' | 'clair' | null;
  dossier_envoyes: string | null;
}

export interface Compteur {
  dossier: string;
  messages_vus: number;
  mails_uniques: number;
  messages_vus_total: number;
  dernier_uid: number;
}

export interface Utilisateur {
  id: number;
  identifiant: string;
  role: Role;
  societe_id: number | null;
  actif: boolean;
  cree_le: string;
}

export interface EtatClaude {
  etat: 'absent' | 'en_test' | 'valide' | 'erreur';
  message?: string;
  teste_le?: string;
  utilise_le?: string | null;
  fin?: string;
  acces?: { mode: 'jeton' | 'cle_api'; jeton: boolean; cle_api: boolean; cle_api_fin: string | null };
}

export interface Seance {
  etat: 'demarrage' | 'attente_code' | 'code_recu' | 'terminee' | 'erreur' | 'expiree';
  url: string | null;
  message: string | null;
}

export interface LigneJournal {
  quand: string;
  auteur: string | null;
  action: string;
  objet_type: string;
  objet_id: string | null;
  details: Record<string, unknown>;
}

export type TypeDocument = 'facture' | 'avoir' | 'bl' | 'relance' | 'devis' | 'autre';
export type EtatDocument = 'a_lire' | 'en_cours' | 'lu' | 'ignore' | 'erreur';
export type Voie = 'facturx' | 'xml' | 'pdf_texte' | 'tableur' | 'scan';

export interface LigneDocument {
  id: number;
  mail_id: number;
  nom_fichier: string;
  etat: EtatDocument;
  voie: Voie | null;
  type: TypeDocument | null;
  societe: string | null;
  fournisseur_nom: string | null;
  numero: string | null;
  date_document: string | null;
  montant_ttc: number | null;
  message: string | null;
  verifie: boolean;
  date_mail: string | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  statut_depot: StatutDepot | null;
  controles: Controle[];
}

export type StatutDepot = 'a_verifier' | 'pret' | 'en_envoi' | 'depose' | 'erreur' | 'doublon';

export interface Controle {
  code: string;
  message: string;
}

export interface Confiance {
  type?: number;
  societe?: number;
  fournisseur?: number;
  numero?: number;
  date?: number;
  montants?: number;
}

export interface FicheDocument {
  id: number;
  mail_id: number;
  mail_objet: string;
  mail_expediteur: string;
  piece_id: number;
  nom_fichier: string;
  type_mime: string;
  etat: EtatDocument;
  voie: Voie | null;
  message: string | null;
  type: TypeDocument | null;
  societe_id: number | null;
  societe: string | null;
  fournisseur_nom: string | null;
  fournisseur_siret: string | null;
  fournisseur_tva: string | null;
  numero: string | null;
  date_document: string | null;
  numero_commande: string | null;
  montant_ht: number | null;
  montant_tva: number | null;
  montant_ttc: number | null;
  confiance: Confiance;
  texte: string | null;
  modele: string | null;
  effort: string | null;
  copie_de: number | null;
  lu_le: string | null;
  verifie_le: string | null;
  verifie_par: string | null;
  type_corrige: boolean;
  societe_corrigee: boolean;
  fournisseur_id: number | null;
  fournisseur: string | null;
  fournisseur_statut: StatutFournisseur | null;
  statut_depot: StatutDepot | null;
  controles: Controle[];
  valide_le: string | null;
  valide_par: string | null;
  corrections: Record<string, { avant: unknown; apres: unknown }>;
  societe_adresse_depot: string | null;
  societe_outil_depot: string | null;
  envois: Envoi[];
}

export interface Envoi {
  id: number;
  quand: string;
  adresse: string;
  outil: string | null;
  etat: 'en_cours' | 'envoye' | 'erreur';
  message: string | null;
  auteur: string | null;
}

export type StatutFournisseur = 'a_confirmer' | 'valide';

export interface LigneFournisseur {
  id: number;
  nom: string;
  siret: string | null;
  tva: string | null;
  domaines: string[];
  statut: StatutFournisseur;
  societe_defaut_id: number | null;
  societe_defaut: string | null;
  nb_documents: number;
  a_verifier: number;
  /** Ligne d'état (refonte) : documents lus visibles, de la société choisie (`?societe=`). */
  factures_mois: number;
  a_valider: number;
  bl_sans_facture: number;
  dernier_document: string | null;
}

export interface Fournisseur {
  id: number;
  nom: string;
  siret: string | null;
  tva: string | null;
  domaines: string[];
  statut: StatutFournisseur;
  societe_defaut_id: number | null;
  format_bl: string | null;
  notes: string | null;
  fusionne_dans: number | null;
  cree_le: string;
}

export interface Expedition {
  serveur: string;
  port: number;
  securite: 'tls' | 'starttls';
  identifiant: string;
  adresse: string;
  nom: string | null;
}

export interface ExpeditionVue {
  expedition: Expedition | null;
  mot_de_passe_enregistre: boolean;
  alerte_destinataires: string[];
  dernier_test: { quand: string; ok: boolean; message: string | null } | null;
}

export interface LigneSuivi {
  societe_id: number | null;
  societe: string | null;
  recues: number;
  deposees: number;
  pretes: number;
  a_verifier: number;
  en_erreur: number;
  doublons: number;
  montant_depose: number;
}

export interface Compte {
  cle: string | null;
  n: number;
}

export interface Bilan {
  par_etat: Compte[];
  par_voie: Compte[];
  par_type: Compte[];
  verifies: number;
  type_justes: number;
  societe_justes: number;
  semaines: { semaine: string; valides: number; sans_correction: number }[];
}

export interface Reglage {
  modele: string;
  effort: string;
}

export interface Reglages {
  extraction_texte: Reglage;
  ocr_scan: Reglage;
  nouvel_essai: Reglage;
  identification_fournisseur: Reglage;
  lecture_auto: boolean;
  quota_lecture_jour: number;
  tri_mail: Reglage;
  brouillon_reponse: Reglage;
  tri_auto: boolean;
  quota_tri_jour: number;
  tri_depuis_jours: number;
}

export interface ReglagesVue extends Reglages {
  appels_lecture_du_jour: number;
  appels_tri_du_jour: number;
  modeles: string[];
  efforts: string[];
}

export interface AppelClaude {
  quand: string;
  tache: string;
  modele: string;
  effort: string | null;
  duree_ms: number;
  succes: boolean;
  message: string | null;
}

export interface Appels extends Page<AppelClaude> {
  stats: { tache: string; modele: string; n: number; reussis: number; duree_moyenne_ms: number }[];
}

export interface DocRap {
  id: number;
  type: TypeDocument;
  numero: string | null;
  date_document: string | null;
  numero_commande: string | null;
  montant_ht: number | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  societe: string | null;
  nom_fichier: string;
}

export type MethodeRapprochement = 'commande' | 'numero_bl' | 'facture_citee' | 'manuel';

export interface EcartLigne {
  reference: string | null;
  designation: string | null;
  qte_bl: number;
  qte_facture: number;
}

export interface Rapproche {
  facture: DocRap;
  bls: (DocRap & { lien_id: number; methode: MethodeRapprochement })[];
  ecart_montant: number | null;
  ecarts: EcartLigne[];
}

export interface ResumeFournisseur {
  fournisseur_id: number | null;
  fournisseur: string | null;
  bls: number;
  factures: number;
  bl_sans_facture: number;
  factures_sans_bl: number;
  factures_avec_ecart: number;
}

export interface VueRapprochement {
  resume: ResumeFournisseur[];
  bl_sans_facture: DocRap[];
  factures_sans_bl: DocRap[];
  avec_ecart: Rapproche[];
  sans_ecart: Rapproche[];
  tronque: boolean;
  bl_sans_lignes: number;
}

export interface LigneArticle {
  rang: number;
  reference: string | null;
  designation: string | null;
  quantite: number | null;
  unite: string | null;
  prix_unitaire_ht: number | null;
  montant_ht: number | null;
}

export interface DetailRapprochement {
  lignes: LigneArticle[];
  lignes_lues: boolean;
  numeros_bl: string[];
  ignore: boolean;
  liens: { id: number; methode: MethodeRapprochement; ecart_montant: number | null; ecarts: EcartLigne[]; auteur: string | null; autre: DocRap }[];
  candidats: DocRap[];
}

// ---------------------------------------------------------------------------------------------
// Refonte de l'interface : contrat dans docs/api-refonte.md.

/** Réponse d'erreur de l'API ; `code` seulement pour certains 409 (voir api-refonte.md). */
export interface ErreurCorps {
  erreur: string;
  code?: CodeRefus;
}

export type CodeRefus =
  | 'etat'
  | 'deja_envoye'
  | 'expedition'
  | 'erreur_envoi'
  | 'doublon'
  | 'sans_adresse'
  | 'smtp_absent';

export interface CompteursATraiter {
  bloque: number;
  a_valider: number;
  a_repondre: number;
  total: number;
}

export type GenreBloque = 'envoi_erreur' | 'doublon' | 'illisible' | 'boite' | 'claude';

export interface Bloque {
  genre: GenreBloque;
  document_id: number | null;
  boite_id: number | null;
  /** Libellé de la boîte, ou nom du fichier du document. */
  libelle: string | null;
  /** Boîte : `pause_auto` ou `erreur` ; Claude : `absent` ou `erreur`. */
  motif: string | null;
  message: string | null;
  depuis: string | null;
  type: TypeDocument | null;
  fournisseur: string | null;
  numero: string | null;
  societe_id: number | null;
  societe: string | null;
}

export interface DocumentAValider {
  id: number;
  mail_id: number;
  type: 'facture' | 'avoir' | 'bl';
  fournisseur_id: number | null;
  fournisseur: string | null;
  fournisseur_a_confirmer: boolean;
  numero: string | null;
  societe_id: number | null;
  societe: string | null;
  date_document: string | null;
  recu_le: string;
  montant_ttc: number | null;
  /** Première alerte en une phrase, ou null. */
  alerte: string | null;
  nb_alertes: number;
  statut_depot: StatutDepot | null;
  /** Déjà vérifié (facture vérifiée mais pas encore envoyée). */
  verifie: boolean;
}

export interface MailARepondre {
  id: number;
  expediteur: string;
  objet: string;
  date_mail: string | null;
  resume: string | null;
  priorite: Priorite | null;
  societes: string[];
  brouillon: StatutBrouillon | null;
}

export interface FileATraiter {
  compteurs: CompteursATraiter;
  bloque: Bloque[];
  a_valider: DocumentAValider[];
  a_repondre: MailARepondre[];
  /** Premier document à valider (« Commencer »). */
  suivant: number | null;
}

/** Corps de `PUT /api/documents/{id}` et des corrections de `valider`. */
export interface CorrectionDocument {
  type: TypeDocument;
  societe_id: number | null;
  fournisseur_id: number | null;
  numero: string | null;
  date_document: string | null;
  numero_commande: string | null;
  montant_ht: number | null;
  montant_tva: number | null;
  montant_ttc: number | null;
}

export interface DemandeValidation {
  corrections?: CorrectionDocument | null;
  /** Administrateur seulement : envoyer un doublon quand même. */
  forcer?: boolean;
}

export interface ResultatValidation {
  id: number;
  statut_depot: StatutDepot | null;
  /** Envoi à Dext/Yooz demandé. */
  envoi: boolean;
  /** Document suivant de la file, ou null : « Tout est validé ». */
  suivant: number | null;
}

export interface EtatFournisseur {
  factures_mois: number;
  a_valider: number;
  bl_sans_facture: number;
}

export type ElementFil =
  | {
      genre: 'document';
      id: number;
      mail_id: number;
      type: 'facture' | 'avoir' | 'bl';
      numero: string | null;
      /** Date du document, à défaut du mail (AAAA-MM-JJ) : sert au classement. */
      date: string;
      date_document: string | null;
      montant_ht: number | null;
      montant_ttc: number | null;
      statut_depot: StatutDepot | null;
      verifie: boolean;
      societe_id: number | null;
      societe: string | null;
      rapproche: boolean;
      nb_alertes: number;
    }
  | {
      genre: 'mail';
      id: number;
      date: string;
      date_mail: string | null;
      expediteur: string;
      objet: string;
      categorie: Categorie | null;
      statut_reponse: StatutReponse | null;
      resume: string | null;
      /** Documents de ce fournisseur portés par le mail (aussi dans le fil). */
      document_ids: number[];
    };

export interface FilFournisseur {
  fournisseur: Fournisseur;
  etat: EtatFournisseur;
  mois: { mois: string; elements: ElementFil[] }[];
  a_rapprocher: {
    bl_sans_facture: DocRap[];
    factures_sans_bl: DocRap[];
    avec_ecart: Rapproche[];
    tronque: boolean;
  };
}

export interface InterpretationRecherche {
  mots: string[];
  montant: number | null;
  nombres: number[];
  date: { jour: number; mois: number; annee: number | null } | null;
}

export interface DocumentTrouve {
  id: number;
  mail_id: number;
  type: TypeDocument | null;
  etat: EtatDocument;
  statut_depot: StatutDepot | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  numero: string | null;
  date_document: string | null;
  montant_ht: number | null;
  montant_ttc: number | null;
  societe_id: number | null;
  societe: string | null;
  nom_fichier: string;
  recu_le: string;
  verifie: boolean;
}

export interface MailTrouve {
  id: number;
  expediteur: string;
  objet: string;
  date_mail: string | null;
  categorie: Categorie | null;
  statut_reponse: StatutReponse | null;
  resume: string | null;
  nb_pieces: number;
}

export interface ResultatsRecherche {
  interpretation: InterpretationRecherche;
  documents: { total: number; elements: DocumentTrouve[] };
  mails: { total: number; elements: MailTrouve[] };
}

export type CleEtape = 'societes' | 'boites' | 'claude' | 'envoi' | 'utilisateurs';

export interface EtapeMiseEnRoute {
  cle: CleEtape;
  fait: boolean;
  resume: string;
  manques: string[];
}

export interface MiseEnRoute {
  etapes: EtapeMiseEnRoute[];
  terminee: boolean;
}

/** Corps de `POST /api/admin/boites/tester`. */
export interface DemandeEssaiBoite {
  boite_id?: number;
  serveur?: string;
  port?: number;
  identifiant?: string;
  mot_de_passe?: string;
  /** false : SMTP seulement. */
  imap?: boolean;
  smtp_serveur?: string;
  smtp_port?: number;
  smtp_securite?: 'tls' | 'starttls';
}

export interface DossierImap {
  /** Nom IMAP (à enregistrer tel quel). */
  nom: string;
  /** Nom lisible. */
  affiche: string;
  envoyes: boolean;
  non_selectionnable: boolean;
}

export interface EssaiBoite {
  etat: 'en_cours' | 'termine';
  cree_le: string;
  boite_id: number | null;
  identifiant: string;
  imap: { serveur: string; port: number } | null;
  smtp: { serveur: string; port: number; securite: string } | null;
  resultat_imap: {
    ok: boolean;
    message: string;
    detail: string | null;
    dossiers: DossierImap[];
    reception: string | null;
    envoyes: string | null;
  } | null;
  resultat_smtp: { ok: boolean; message: string; detail: string | null } | null;
  termine_le: string | null;
}

export interface BlEnAttente {
  societe_id: number | null;
  societe: string | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  bl: number;
}

export interface BilanMois {
  mois: string;
  societes: LigneSuivi[];
  bl_en_attente: BlEnAttente[];
  qualite: { lecture: Bilan; tri: SemaineTri[] };
}
