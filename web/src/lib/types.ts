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
}

export interface Tableau {
  mails_du_jour: { societe_id: number; raison_sociale: string; n: number }[];
  boites_alerte: { id: number; libelle: string; motif: 'erreur' | 'pause_auto'; depuis: string | null; message: string | null }[];
  claude?: { etat: string; teste_le?: string | null };
  stockage?: { octets: number; libre_pourcent: number | null };
  documents: { a_lire: number; erreur: number; a_verifier: number; lus_aujourdhui: number };
  depots: { a_verifier: number; prets: number; bloques: number; deposes_mois: number };
  lecture?: { auto: boolean; quota: number; appels_du_jour: number };
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
}

export interface ReglagesVue extends Reglages {
  appels_lecture_du_jour: number;
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
