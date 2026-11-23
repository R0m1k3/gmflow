// Petits outils des réglages : serveurs de mail d'après le domaine, noms de dossiers IMAP,
// libellés de l'historique et des durées.

export type Securite = 'tls' | 'starttls';
export type Hebergeur = 'gmail' | 'infomaniak' | null;

export interface Serveurs {
  /** Hébergeur reconnu d'après le domaine ; `null` : inconnu (champs à montrer). */
  hebergeur: Hebergeur;
  imap: { serveur: string; port: number };
  smtp: { serveur: string; port: number; securite: Securite };
}

const GMAIL = ['gmail.com', 'googlemail.com'];
const INFOMANIAK = ['ik.me', 'ikmail.com', 'etik.com'];

/** Domaine d'une adresse, en minuscules ('' sans @). */
export function domaine(adresse: string): string {
  const i = adresse.lastIndexOf('@');
  return i < 0 ? '' : adresse.slice(i + 1).trim().toLowerCase();
}

/**
 * Serveurs à préremplir d'après l'adresse : Gmail, Infomaniak ; un domaine inconnu reçoit
 * les serveurs d'Infomaniak (les boîtes de GMFlow y sont presque toutes), à vérifier.
 */
export function serveursPour(adresse: string): Serveurs {
  const d = domaine(adresse);
  if (GMAIL.includes(d)) {
    return {
      hebergeur: 'gmail',
      imap: { serveur: 'imap.gmail.com', port: 993 },
      smtp: { serveur: 'smtp.gmail.com', port: 465, securite: 'tls' }
    };
  }
  return {
    hebergeur: INFOMANIAK.includes(d) ? 'infomaniak' : null,
    imap: { serveur: 'mail.infomaniak.com', port: 993 },
    smtp: { serveur: 'mail.infomaniak.com', port: 465, securite: 'tls' }
  };
}

/** Serveur d'envoi qui va avec un serveur de réception. */
export function smtpPourImap(imap: string): string {
  const h = imap.trim().toLowerCase();
  if (h === 'imap.gmail.com') return 'smtp.gmail.com';
  if (h === 'mail.infomaniak.com') return h;
  return h.replace(/^imap\./, 'smtp.');
}

export function portPourSecurite(s: Securite): number {
  return s === 'starttls' ? 587 : 465;
}

/** Où créer un mot de passe d'application, selon l'hébergeur. */
export function aideMotDePasse(h: Hebergeur): string {
  if (h === 'gmail') {
    return 'Pas le mot de passe du compte Google : activez la validation en deux étapes, puis créez un mot de passe d’application dans le compte Google, rubrique Sécurité.';
  }
  return 'Pas le mot de passe habituel de la boîte. Chez Infomaniak : Manager, rubrique Service Mail, puis Mots de passe d’application.';
}

/** Nom IMAP en UTF-7 modifié (« Envoy&AOk-s ») vers un nom lisible ; INBOX devient Réception. */
export function nomDossier(nom: string): string {
  if (nom.toUpperCase() === 'INBOX') return 'Réception';
  return nom.replace(/&([^-]*)-/g, (tout, b64: string) => {
    if (b64 === '') return '&';
    try {
      const octets = atob(b64.replace(/,/g, '/'));
      let s = '';
      for (let i = 0; i + 1 < octets.length; i += 2) {
        s += String.fromCharCode((octets.charCodeAt(i) << 8) | octets.charCodeAt(i + 1));
      }
      return s;
    } catch {
      return tout;
    }
  });
}

/** Le dossier ressemble-t-il à celui des messages envoyés ? */
export function ressembleEnvoyes(nom: string): boolean {
  return /sent|envoy/i.test(nomDossier(nom));
}

export function duree(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} s`;
}

/** Pluriel simple : « 1 boîte », « 3 boîtes ». */
export function n(nombre: number, un: string, plusieurs = `${un}s`): string {
  return `${nombre.toLocaleString('fr-FR')} ${nombre > 1 ? plusieurs : un}`;
}

// ---------- Historique (ex-journal) ----------

export const OBJETS_HISTORIQUE: { valeur: string; libelle: string }[] = [
  { valeur: '', libelle: 'Tout' },
  { valeur: 'boite_mail', libelle: 'Boîtes mail' },
  { valeur: 'societe', libelle: 'Sociétés' },
  { valeur: 'utilisateur', libelle: 'Utilisateurs' },
  { valeur: 'mail', libelle: 'Mails' },
  { valeur: 'piece_jointe', libelle: 'Pièces jointes' },
  { valeur: 'claude', libelle: 'Connexion à Claude' }
];

const OBJETS: Record<string, string> = {
  boite_mail: 'boîte mail',
  societe: 'société',
  utilisateur: 'utilisateur',
  mail: 'mail',
  piece_jointe: 'pièce jointe',
  document: 'document',
  fournisseur: 'fournisseur',
  claude: 'Claude',
  expedition: 'adresse d’envoi',
  parametre: 'réglage',
  donnees: 'données'
};
export function libelleObjet(t: string): string {
  return OBJETS[t] ?? t.replace(/_/g, ' ');
}

const ACTIONS: Record<string, string> = {
  creation: 'Création',
  modification: 'Modification',
  suppression: 'Suppression',
  test: 'Essai',
  activation: 'Surveillance démarrée',
  pause: 'Mise en pause',
  pause_auto: 'Mise en pause automatique',
  releve_manuelle: 'Nouveaux mails cherchés',
  uidvalidity_change: 'Dossier renuméroté par le serveur',
  connexion: 'Connexion',
  connexion_refusee: 'Connexion refusée',
  premier_compte: 'Premier compte créé',
  validation: 'Validation',
  verification: 'Validation',
  lecture: 'Lecture',
  relecture: 'Nouvelle lecture',
  relecture_bl: 'Nouvelle lecture des BL',
  depot: 'Envoi à Dext / Yooz',
  depot_echec: 'Envoi échoué',
  rapprochement: 'BL lié',
  rapprochement_defait: 'Lien de BL défait',
  rattachement_fournisseur: 'Fournisseur rattaché',
  fusion: 'Fournisseurs fusionnés',
  categorie: 'Tri corrigé',
  statut_reponse: 'Suite de la réponse',
  brouillon_demande: 'Réponse demandée',
  brouillon_abandonne: 'Réponse effacée',
  reponse_demandee: 'Réponse demandée',
  reponse_envoyee: 'Réponse envoyée',
  reponse_echec: 'Réponse pas partie',
  expediteur_marque: 'Expéditeur publicitaire ajouté',
  expediteur_publicite: 'Expéditeur publicitaire ajouté',
  expediteur_publicite_retire: 'Expéditeur publicitaire retiré',
  purge_publicites: 'Publicités anciennes effacées',
  reinitialisation: 'Réinitialisation : tout effacé, relève relancée',
  piece_alteree: 'Pièce jointe modifiée',
  message_id_reutilise: 'Mail en double',
  seance_setup_token: 'Connexion au compte Claude',
  annulation_setup_token: 'Connexion au compte Claude annulée',
  jeton_colle: 'Jeton de connexion collé',
  jeton_supprime: 'Déconnexion de Claude',
  test_jeton: 'Essai de la connexion à Claude',
  cle_api_enregistree: 'Clé API enregistrée',
  cle_api_supprimee: 'Clé API supprimée',
  acces_claude: 'Moyen de connexion à Claude changé',
  reglages_claude: 'Réglages avancés modifiés',
  essai: 'Essai'
};
export function libelleAction(a: string): string {
  return ACTIONS[a] ?? a.charAt(0).toUpperCase() + a.slice(1).replace(/_/g, ' ');
}
