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
  pieces: { id: number; nom: string; type_mime: string; taille: number }[];
  boites: { boite_id: number; libelle: string; dossier: string }[];
}

export interface Tableau {
  mails_du_jour: { societe_id: number; raison_sociale: string; n: number }[];
  boites_alerte: { id: number; libelle: string; motif: 'erreur' | 'pause_auto'; depuis: string | null; message: string | null }[];
  claude?: { etat: string; teste_le?: string | null };
  stockage?: { octets: number; libre_pourcent: number | null };
}

export interface Societe {
  id: number;
  raison_sociale: string;
  siret: string | null;
  tva: string | null;
  adresses: string[];
  outil_depot: 'dext' | 'yooz' | null;
  adresse_depot: string | null;
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
