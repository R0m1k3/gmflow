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
  /** Adresse de la boîte d'envoi : « Envoyer depuis compta@… ». */
  boite_adresse: string;
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
  /** Corps HTML d'origine disponible (`/api/mails/{id}/html`), affiché à la place du texte. */
  html: boolean;
  /** Fiche fournisseur reconnue par le domaine de l'expéditeur. */
  fournisseur_id: number | null;
  fournisseur: string | null;
  pieces: {
    id: number;
    nom: string;
    type_mime: string;
    taille: number;
    document_id: number | null;
    document_etat: EtatDocument | null;
    document_type: TypeDocument | null;
    document_voie: Voie | null;
    /** Résumé du document lu (null sans document). */
    document_statut_depot: StatutDepot | null;
    document_verifie: boolean | null;
    document_numero: string | null;
    document_montant_ttc: number | null;
    document_fournisseur_id: number | null;
    /** Fiche fournisseur, à défaut nom lu. */
    document_fournisseur: string | null;
    document_societe_id: number | null;
    document_societe: string | null;
    document_outil: 'dext' | 'yooz' | null;
    /** Dernier envoi réussi à Dext/Yooz. */
    document_envoye_le: string | null;
  }[];
  /** Boîtes qui ont reçu le mail (visibles), une ligne par dossier. */
  boites: {
    boite_id: number;
    libelle: string;
    dossier: string;
    /** Adresse de la boîte (identifiant IMAP). */
    adresse: string;
    societe_id: number;
    societe: string;
    /** Serveur SMTP configuré : la boîte peut envoyer les réponses. */
    smtp: boolean;
  }[];
  categorie: Categorie | null;
  categorie_par: 'regle' | 'claude' | 'humain' | null;
  resume: string | null;
  priorite: Priorite | null;
  statut_reponse: StatutReponse | null;
  tri_etat: 'a_trier' | 'en_cours' | 'trie' | 'erreur' | null;
  tri_message: string | null;
  brouillons: Brouillon[];
}

export interface Societe {
  id: number;
  raison_sociale: string;
  /** Autres noms employés par les fournisseurs (nom complet, enseigne et ville...). */
  autres_noms: string[];
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
  /** Phrase en clair (« Mot de passe refusé : … », « Connexion réussie : 7 dossiers trouvés. »). */
  message?: string;
  /** Erreur brute du serveur. */
  detail?: string | null;
  /** Noms IMAP, à enregistrer tels quels. */
  dossiers?: string[];
  /** Noms décodés et attributs (tests faits depuis la refonte ; absent avant). */
  dossiers_detail?: DossierImap[];
  /** Dossier des envoyés repéré (nom IMAP), null si rien de sûr. */
  envoyes?: string | null;
  /** Création depuis un essai de l'assistant. */
  essai_id?: string;
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
  /** Jour de la réinitialisation (`AAAA-MM-JJ`) : la relève ne remonte pas avant. */
  reprise_depuis: string | null;
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

export type StatutDepot = 'a_verifier' | 'pret' | 'en_envoi' | 'depose' | 'erreur' | 'doublon';

export interface Controle {
  code: string;
  /** Phrase dans le vocabulaire de l'écran (société, Dext/Yooz, « 1 250,40 € »), sans point final. */
  message: string;
  /** Champs visés (noms de l'API), absents sur les contrôles enregistrés avant la refonte. */
  champs?: ChampControle[];
  /** Correction proposée en un clic (montants). */
  proposition?: { champ: ChampControle; valeur: number };
}

export type ChampControle =
  | 'numero'
  | 'date_document'
  | 'montant_ht'
  | 'montant_tva'
  | 'montant_ttc'
  | 'societe_id'
  | 'fournisseur_id';

export interface Confiance {
  type?: number;
  societe?: number;
  fournisseur?: number;
  numero?: number;
  date?: number;
  montants?: number;
  /** BL sans totaux imprimés : montants calculés depuis les lignes. */
  montants_calcules?: ('ht' | 'tva' | 'ttc')[];
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
  /** Échéance du paiement (facture), AAAA-MM-JJ. */
  date_echeance: string | null;
  montant_ht: number | null;
  montant_tva: number | null;
  montant_ttc: number | null;
  /** Remise HT déduite sur la pièce, en positif, et son taux (%) s'il est imprimé. */
  remise_ht: number | null;
  remise_taux: number | null;
  /** Éco-participation, port… compris dans le HT (jamais sur un BL). */
  annexes_ht: number | null;
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
  /** Réception du mail d'origine (date du mail, à défaut de la relève). */
  recu_le: string;
  /** Envoi en échec relancé il y a moins de 30 min, pas encore repris : ne pas renvoyer. */
  envoi_relance: boolean;
  envois: Envoi[];
  /** Boîtes de réception du mail d'origine (visibles). */
  boites: { boite_id: number; libelle: string; adresse: string; societe_id: number; societe: string }[];
  /** Document pas encore lu : pourquoi il attend. */
  attente: Attente | null;
  /** Dernier report de la facture dans Logiflow (après l'envoi à Dext/Yooz). */
  logiflow: ResultatLogiflow | null;
  logiflow_configure: boolean;
  /** Facture envoyée à l'instant : le résultat Logiflow arrive. */
  logiflow_attendu: boolean;
  /** BL : sa livraison retrouvée dans Logiflow par la vérification de chaque heure. */
  logiflow_livraison: LivraisonLogiflow | null;
}

export interface LivraisonLogiflow {
  vu_le: string;
  /** trouve : une livraison ; ambigu : plusieurs possibles. */
  etat: 'trouve' | 'ambigu';
  possibles: number;
  livraison_id: number | null;
  magasin: string | null;
  fournisseur: string | null;
  bl: string | null;
  livree_le: string | null;
  montant_bl: number | null;
  facture: string | null;
  facture_ht: number | null;
  facture_ttc: number | null;
  echeance: string | null;
  rapprochee: boolean;
}

export type EtatLivraisonLogiflow = 'complete' | 'deja_fait' | 'deja_rapproche' | 'autre_facture' | 'introuvable' | 'ambigu' | 'erreur';

export interface ResultatLogiflow {
  quand: string;
  ok: boolean;
  message: string;
  livraisons: { bl: string; livraison_id: number | null; magasin: string | null; etat: EtatLivraisonLogiflow; message: string }[];
}

export type GenreAttente =
  | 'en_cours'
  | 'en_file'
  | 'bloque'
  | 'file_arretee'
  | 'jeton_absent'
  | 'claude_refuse'
  | 'quota'
  | 'lecture_coupee'
  | 'a_relancer';

/** Pourquoi un document n'est pas encore lu. */
export interface Attente {
  genre: GenreAttente;
  /** Phrase principale, sans point final. */
  raison: string;
  /** En attente depuis. */
  depuis: string;
  /** Documents qui passeront avant lui dans la file de lecture. */
  devant: number | null;
  lecture_auto: boolean;
  /** Raison notée au dernier essai, si autre. */
  dernier_essai: string | null;
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
  code_gestion: string | null;
  siret: string | null;
  tva: string | null;
  domaines: string[];
  statut: StatutFournisseur;
  societe_defaut_id: number | null;
  societe_defaut: string | null;
  /** Groupe dont ce fournisseur fait partie. */
  groupe: string | null;
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
  /** Code BO Phénix : code interne du fournisseur dans le logiciel de gestion du magasin. */
  code_gestion: string | null;
  siret: string | null;
  tva: string | null;
  domaines: string[];
  statut: StatutFournisseur;
  societe_defaut_id: number | null;
  /** Nom de la société par défaut. */
  societe_defaut: string | null;
  format_bl: string | null;
  notes: string | null;
  adresse: string | null;
  telephone: string | null;
  email: string | null;
  interlocuteur: string | null;
  conditions_paiement: string | null;
  /** Fiche du groupe (société qui facture pour plusieurs marques) et son nom. */
  groupe_id: number | null;
  groupe: string | null;
  /** Noms cherchés dans le texte des documents du groupe pour reconnaître cette marque. */
  noms_documents: string[];
  /** Débuts de codes qui désignent cette marque sur les documents du groupe (« LC »). */
  prefixes_codes: string[];
  /** Export Phénix : colonnes reprises du BL (`ref` ou `gencod` seul ; null : les deux ou le réglage du groupe). */
  colonnes_export: 'ref' | 'gencod' | null;
  fusionne_dans: number | null;
  cree_le: string;
}

/** Marque d'un groupe (`GET /api/fournisseurs/{id}/marques`). */
export interface LigneMarque {
  id: number;
  nom: string;
  code_gestion: string | null;
  noms_documents: string[];
  prefixes_codes: string[];
  statut: StatutFournisseur;
  nb_documents: number;
}

/** Fiche résumée d'une paire de `GET /api/admin/fournisseurs/doublons`. */
export interface FicheCourte {
  id: number;
  nom: string;
  code_gestion: string | null;
  siret: string | null;
  tva: string | null;
  domaines: string[];
  statut: StatutFournisseur;
  nb_documents: number;
  /** Groupe dont la fiche fait partie, et son nom. */
  groupe_id: number | null;
  groupe: string | null;
  /** Marques de la fiche (> 0 : c'est un groupe). */
  nb_marques: number;
}

/** Deux fiches qui désignent sans doute le même fournisseur, ou deux marques d'un même groupe. */
export interface PaireDoublon {
  raison: string;
  /** Identifiant, domaine ou adresse en commun, noms sans rapport : à regrouper, pas à fusionner. */
  meme_groupe: boolean;
  a: FicheCourte;
  b: FicheCourte;
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

export interface ServeurApiVue {
  serveur: { adresse: string; fin: string | null } | null;
  jeton_enregistre: boolean;
  dernier_test: { quand: string; ok: boolean; message: string | null } | null;
  synchro: { quand: string; ok: boolean; message: string | null; nombre: number | null } | null;
  /** Fournisseurs du magasin copiés. */
  nombre: number;
}

export interface LogiflowVue {
  serveur: { adresse: string; fin: string | null } | null;
  jeton_enregistre: boolean;
  dernier_test: { quand: string; ok: boolean; message: string | null } | null;
  /** Dernière vérification régulière (toutes les heures) ; `ok` null : en cours. */
  dernier_suivi: { quand: string; ok: boolean | null; message: string | null } | null;
}

export interface SuggestionCode {
  code: string;
  nom: string | null;
  email: string | null;
  adresse: string | null;
  nb_articles: number | null;
  raison: string | null;
  score: number;
  deja_pris_par: string | null;
}

export interface CodesMagasin {
  nombre: number;
  suggestions: SuggestionCode[];
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
  /** Remise HT déduite sur la facture (explique l'écart avec les BL). */
  remise_ht: number | null;
  /** Éco-participation, port… compris dans le HT de la facture. */
  annexes_ht: number | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  societe: string | null;
  nom_fichier: string;
}

export type MethodeRapprochement = 'commande' | 'numero_bl' | 'facture_citee' | 'manuel';

export interface EcartLigne {
  /** Absent sur les écarts calculés avant la comparaison des prix. */
  genre?: 'manquant' | 'en_trop' | 'prix';
  reference: string | null;
  designation: string | null;
  qte_bl: number;
  qte_facture: number;
  prix_facture?: number | null;
  prix_bl?: number | null;
  /** Part de l'écart HT due à l'article : facturé − livré. */
  montant_ht?: number | null;
}

export interface Rapproche {
  facture: DocRap;
  bls: (DocRap & { lien_id: number; methode: MethodeRapprochement })[];
  ecart_montant: number | null;
  /** Éco-participations déjà comprises dans le montant des BL (HT calculé depuis les lignes). */
  eco_bl: number | null;
  ecarts: EcartLigne[];
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
  liens: { id: number; methode: MethodeRapprochement; ecart_montant: number | null; eco_bl: number | null; ecarts: EcartLigne[]; auteur: string | null; autre: DocRap }[];
  candidats: DocRap[];
}

// ---------------------------------------------------------------------------------------------
// Refonte de l'interface : contrat dans docs/api-refonte.md.

export type CodeRefus =
  | 'etat'
  | 'deja_envoye'
  | 'expedition'
  | 'erreur_envoi'
  | 'doublon'
  | 'sans_adresse'
  | 'smtp_absent'
  | 'essai';

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
  remise_ht: number | null;
  /** Articles en écart avec les BL liés (manquant, en trop, prix changé). */
  articles_ecart: number;
  /** Première alerte en une phrase, ou null. */
  alerte: string | null;
  /** Code stable de cette alerte (`doublon`, `montants`, `societe`, `depot`, `fournisseur`...). */
  alerte_code: string | null;
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
  /** Fiche fournisseur reconnue par le domaine de l'expéditeur. */
  fournisseur: string | null;
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
  date_echeance: string | null;
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
      remise_ht: number | null;
      statut_depot: StatutDepot | null;
      verifie: boolean;
      societe_id: number | null;
      societe: string | null;
      /** Outil de collecte de la société (« Envoyée à Dext »). */
      outil: 'dext' | 'yooz' | null;
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
  /** Outil de la société (null si pas encore rangé ou pas réglé). */
  outil: 'dext' | 'yooz' | null;
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
  /** BL lus avant le rapprochement, sans leurs lignes (tout l'historique, mêmes filtres). */
  bl_sans_lignes: number;
  qualite: { lecture: Bilan; tri: SemaineTri[] };
}

/** Corps de `POST /api/admin/boites` (création) ; `PUT /api/admin/boites/{id}` ignore les deux derniers champs. */
export interface EntreeBoite {
  societe_id: number;
  libelle: string;
  serveur: string;
  port?: number;
  identifiant: string;
  mot_de_passe?: string;
  dossiers?: string[];
  reprise_mois?: number;
  smtp_serveur?: string | null;
  smtp_port?: number | null;
  smtp_securite?: 'tls' | 'starttls' | null;
  dossier_envoyes?: string | null;
  /** Essai réussi de ces paramètres (moins d'une heure) : repris comme test de la boîte. */
  essai_id?: string;
  /** Démarrer la surveillance tout de suite (exige `essai_id`). */
  activer?: boolean;
}

/** Corps de `PATCH /api/admin/fournisseurs/{id}` : seuls les champs présents changent. */
export type ChangementsFournisseur = Partial<
  Pick<Fournisseur, 'nom' | 'code_gestion' | 'siret' | 'tva' | 'domaines' | 'statut' | 'societe_defaut_id' | 'format_bl' | 'notes'
  | 'adresse' | 'telephone' | 'email' | 'interlocuteur' | 'conditions_paiement'
  | 'groupe_id' | 'noms_documents' | 'prefixes_codes' | 'colonnes_export'
>
>;

// ---------- Onglets Factures, BL, À répondre (listes existantes) ----------

/** Ligne de `GET /api/documents?type=&verifie=&statut_depot=&q=&page=`. */
export interface LigneDocument {
  id: number;
  mail_id: number;
  nom_fichier: string;
  etat: EtatDocument;
  voie: string | null;
  type: TypeDocument | null;
  societe: string | null;
  fournisseur_nom: string | null;
  numero: string | null;
  date_document: string | null;
  montant_ttc: number | null;
  remise_ht: number | null;
  /** Articles en écart avec les BL liés (manquant, en trop, prix changé). */
  articles_ecart: number;
  message: string | null;
  verifie: boolean;
  date_mail: string | null;
  fournisseur_id: number | null;
  fournisseur: string | null;
  statut_depot: StatutDepot | null;
  controles: unknown;
}

/** Ligne de `GET /api/a-repondre?page=` : priorité haute d'abord, puis les plus anciens. */
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

/** Réponse de `POST /api/admin/reinitialiser` : ce qui a été effacé. */
export interface BilanReinitialisation {
  mails: number;
  documents: number;
  fournisseurs: number;
  taches: number;
  boites_relancees: number;
}

// ---------- Réglages > Traitements ----------

export type CleEtapeTraitement = 'releve' | 'lecture' | 'tri' | 'rapprochement' | 'depot';

export interface EtapeTraitement {
  cle: CleEtapeTraitement;
  nom: string;
  /** `bloque` : elle ne peut pas avancer et du travail attend ; `arrete` : sa file ne bat plus. */
  etat: 'ok' | 'en_retard' | 'bloque' | 'arrete';
  resume: string;
  /** Pourquoi elle ne peut pas avancer (étape bloquée). */
  blocage: string | null;
  en_attente: number;
  plus_vieille: { depuis: string; raison: string } | null;
  dernier_passage: string | null;
  file: { nom: string; battement: string | null; vivante: boolean };
  /** Relance demandée, pas encore prise par le worker. */
  relance_demandee: boolean;
}

export interface Traitements {
  service: { en_marche: boolean; vu_le: string | null; autres_arretees: string[] };
  /** Pourquoi Claude ne lit pas en ce moment, sinon null. */
  claude: string | null;
  etapes: EtapeTraitement[];
}

/** Alerte de l'export Phénix : `rang` = ligne du BL (0 = première), absent si générale. */
export interface AlerteExport {
  rang: number | null;
  article: string | null;
  message: string;
}

/** GET /api/documents/{id}/export-phenix */
export interface EtatExportPhenix {
  serveur: boolean;
  en_cours: boolean;
  verifie_le: string | null;
  erreur: string | null;
  pret: boolean;
  code_fournisseur: string | null;
  lignes: number;
  cherchees: number;
  trouves: number;
  lots: number;
  composants: number;
  a_relire: boolean;
  matrice: { rangees: number; alertes: AlerteExport[] };
  trame: { rangees: number; alertes: AlerteExport[] };
}
