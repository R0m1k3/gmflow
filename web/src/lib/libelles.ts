// Vocabulaire affiché (docs/architecture.md, §4). Toute page passe par ces fonctions :
// aucun code technique (a_verifier, facture_citee…) ne doit arriver à l'écran.
import type {
  Categorie,
  Controle,
  EtatDocument,
  Priorite,
  StatutBrouillon,
  StatutDepot,
  StatutFournisseur,
  StatutReponse,
  TypeDocument,
  Voie
} from './types.ts';
import { sansFormeJuridique, societe } from './societe.svelte.ts';

/** Ton visuel d'une étiquette ou d'un bandeau (voir Etiquette.svelte). */
export type Ton = 'neutre' | 'ok' | 'alerte' | 'erreur' | 'info' | 'pale';

// ---------- Outil de collecte (Dext / Yooz) ----------

/** Nom de l'outil de collecte de la société, « Dext / Yooz » si inconnu. */
export function nomOutil(outil: string | null | undefined): string {
  if (outil === 'dext') return 'Dext';
  if (outil === 'yooz') return 'Yooz';
  return 'Dext / Yooz';
}

/** « Envoi à Dext », titre du bloc d'envoi d'une fiche. */
export function titreEnvoi(outil: string | null | undefined): string {
  return `Envoi à ${nomOutil(outil)}`;
}

/** Libellé du bouton principal d'une fiche document. */
export function libelleValider(type: TypeDocument | null | undefined, outil: string | null | undefined): string {
  return type === 'facture' || type === 'avoir' ? `Valider et envoyer à ${nomOutil(outil)}` : 'Valider';
}

// ---------- Documents ----------

const TYPES: Record<TypeDocument, string> = {
  facture: 'Facture',
  avoir: 'Avoir',
  bl: 'BL',
  relance: 'Relance',
  devis: 'Devis',
  autre: 'Autre document'
};
const TYPES_LONGS: Record<TypeDocument, string> = { ...TYPES, bl: 'Bon de livraison' };

/** « Facture », « Avoir », « BL »… ; `long` donne « Bon de livraison ». */
export function libelleType(t: TypeDocument | null | undefined, long = false): string {
  if (!t) return 'Document';
  return (long ? TYPES_LONGS : TYPES)[t] ?? t;
}

const ETATS: Record<EtatDocument, string> = {
  a_lire: 'En attente',
  en_cours: 'Lecture en cours',
  lu: 'Lu',
  ignore: 'Écarté (pas une facture)',
  erreur: 'Illisible'
};
const TONS_ETAT: Record<EtatDocument, Ton> = {
  a_lire: 'pale',
  en_cours: 'pale',
  lu: 'neutre',
  ignore: 'pale',
  erreur: 'erreur'
};

/** État de lecture d'un document. */
export function libelleEtat(e: EtatDocument | null | undefined): string {
  return e ? (ETATS[e] ?? e) : '';
}
export function tonEtat(e: EtatDocument | null | undefined): Ton {
  return e ? (TONS_ETAT[e] ?? 'neutre') : 'neutre';
}

/** Statut d'envoi à Dext / Yooz (`statut_depot`). */
export function libelleStatutEnvoi(s: StatutDepot | null | undefined, outil?: string | null): string {
  switch (s) {
    case 'a_verifier':
      return 'À valider';
    case 'pret':
      return 'Prête';
    case 'en_envoi':
      return 'Envoi en cours';
    case 'depose':
      return `Envoyée à ${nomOutil(outil)}`;
    case 'erreur':
      return 'Envoi échoué';
    case 'doublon':
      return 'Doublon';
    default:
      return '';
  }
}
export function tonStatutEnvoi(s: StatutDepot | null | undefined): Ton {
  switch (s) {
    case 'a_verifier':
      return 'alerte';
    case 'depose':
      return 'ok';
    case 'erreur':
    case 'doublon':
      return 'erreur';
    case 'pret':
    case 'en_envoi':
      return 'neutre';
    default:
      return 'pale';
  }
}

/** Voie de lecture, montrée aux admins seulement : « lu dans le scan ». */
export function libelleVoie(v: Voie | null | undefined): string {
  switch (v) {
    case 'facturx':
      return 'lu dans le Factur-X';
    case 'xml':
      return 'lu dans le XML';
    case 'pdf_texte':
      return 'lu dans le PDF';
    case 'tableur':
      return 'lu dans le tableur';
    case 'scan':
      return 'lu dans le scan';
    default:
      return '';
  }
}

/** Titre court d'une alerte (contrôle) ; le message du serveur sert de détail. */
export function titreAlerte(c: Pick<Controle, 'code' | 'message'>): string {
  switch (c.code) {
    case 'montants':
      return c.message.includes('manquant') ? 'Montant manquant' : 'Montant incohérent';
    case 'incomplet':
      return 'Champ manquant';
    case 'identifiants':
      return 'Identifiant invalide';
    case 'societe':
      return 'Société non reconnue';
    case 'fournisseur':
      return c.message.includes('confirmer') ? 'Nouveau fournisseur' : 'Fournisseur inconnu';
    case 'date':
      return 'Date douteuse';
    case 'doublon':
      return 'Doublon possible';
    default:
      return 'À vérifier';
  }
}
/** Détail d'une alerte sous son titre : le message du serveur, sauf s'il ne fait que répéter le titre. */
export function detailAlerte(c: Pick<Controle, 'code' | 'message'>): string {
  const m = c.message.trim();
  if (!m.toLowerCase().startsWith(titreAlerte(c).toLowerCase())) return m;
  return c.code === 'fournisseur' ? 'Vérifiez son nom et son SIRET, puis confirmez la fiche' : '';
}

/** Statut d'un fournisseur. */
export function libelleStatutFournisseur(s: StatutFournisseur | null | undefined): string {
  return s === 'a_confirmer' ? 'Nouveau fournisseur (à confirmer)' : '';
}

// ---------- Rapprochement ----------

const METHODES: Record<string, string> = {
  numero_bl: 'par n° de BL',
  facture_citee: 'facture citée sur le BL', // architecture §4 : « citée dans la facture », ambigu ici
  commande: 'par n° de commande',
  manuel: 'liée à la main'
};
/** Comment un BL et une facture ont été liés. */
export function libelleMethode(m: string | null | undefined): string {
  return m ? (METHODES[m] ?? m) : '';
}
/** Titre du bloc de rapprochement d'une fiche : « BL liés » sur une facture, « Facture liée » sur un BL. */
export function titreLiens(type: TypeDocument | null | undefined): string {
  return type === 'bl' ? 'Facture liée' : 'BL liés';
}
export const BL_EN_ATTENTE = 'BL en attente de facture';
export const FACTURE_SANS_BL = 'Facture sans BL';

// ---------- Mails ----------

const CATEGORIES: Record<Categorie, string> = {
  question: 'Question',
  information: 'Information',
  document_seul: 'Pièce jointe seule',
  publicite: 'Publicité',
  notification: 'Notification automatique'
};
export function libelleCategorie(c: Categorie | null | undefined): string {
  return c ? (CATEGORIES[c] ?? c) : 'Pas encore trié';
}

const STATUTS_REPONSE: Record<StatutReponse, string> = {
  a_repondre: 'À répondre',
  repondu: 'Répondu',
  sans_suite: 'Pas besoin de réponse'
};
export function libelleStatutReponse(s: StatutReponse | null | undefined): string {
  return s ? (STATUTS_REPONSE[s] ?? s) : '';
}
export function tonStatutReponse(s: StatutReponse | null | undefined): Ton {
  return s === 'a_repondre' ? 'alerte' : s === 'repondu' ? 'ok' : 'pale';
}

/** Priorité d'une question : seul « Urgent » s'affiche. */
export function libellePriorite(p: Priorite | null | undefined): string {
  return p === 'haute' ? 'Urgent' : '';
}

const BROUILLONS: Record<StatutBrouillon, string> = {
  generation: 'Réponse en préparation',
  propose: 'Réponse proposée',
  modifie: 'Réponse proposée, modifiée',
  envoi: 'Envoi en cours',
  envoye: 'Envoyée',
  abandonne: 'Effacée'
};
export function libelleBrouillon(s: StatutBrouillon | null | undefined): string {
  return s ? (BROUILLONS[s] ?? s) : '';
}

export const ACTIONS_REPONSE = {
  proposer: 'Proposer une réponse',
  autre: 'Proposer une autre',
  effacer: 'Effacer',
  sansSuite: 'Pas besoin de réponse'
} as const;

/** « Envoyer depuis compta@… » */
export function libelleEnvoyerDepuis(adresse: string | null | undefined): string {
  return adresse ? `Envoyer depuis ${adresse}` : 'Envoyer';
}

const PAR: Record<string, string> = { regle: 'par une règle', claude: 'par Claude', humain: 'à la main' };
/** Qui a trié le mail. */
export function libelleTriePar(p: string | null | undefined): string {
  return p ? (PAR[p] ?? p) : '';
}

// ---------- Réglages ----------

export const RUBRIQUES = {
  societes: 'Sociétés',
  boites: 'Boîtes mail',
  claude: 'Connexion à Claude',
  envoi: 'Envoi vers Dext / Yooz',
  utilisateurs: 'Utilisateurs',
  traitements: 'Traitements',
  avance: 'Avancé',
  expediteursPub: 'Expéditeurs publicitaires',
  historique: 'Historique',
  limites: 'Limite par jour',
  adresseEnvoi: 'Adresse d’envoi vers Dext / Yooz',
  envoiAuto: 'Envoi automatique des factures sûres',
  releve: 'Chercher les nouveaux mails'
} as const;

const TACHES: Record<string, string> = {
  extraction_texte: 'Lecture des PDF et tableurs',
  ocr_scan: 'Lecture des scans',
  nouvel_essai: 'Nouvel essai après échec',
  identification_fournisseur: 'Reconnaissance des fournisseurs',
  tri_mail: 'Tri des mails',
  brouillon_reponse: 'Réponses proposées',
  test_jeton: 'Test de la connexion'
};
/** Tâche confiée à Claude (réglages avancés). */
export function libelleTache(t: string): string {
  return TACHES[t] ?? t;
}

/** Rôle d'un utilisateur. */
export function libelleRole(r: string | null | undefined): string {
  return r === 'admin' ? 'Administrateur' : r === 'responsable' ? 'Responsable de magasin' : (r ?? '');
}

// ---------- Sociétés ----------

/** « SAS Frouard Distribution » devient « Frouard Distribution » (forme juridique retirée). */
export function nomCourtSociete(raison: string | null | undefined): string {
  if (!raison) return '';
  const court = sansFormeJuridique(raison);
  // « La Foir'Fouille Frouard » et « La Foir'Fouille Houdemont » : « Frouard », « Houdemont ».
  const p = societe.prefixe;
  if (p && court.startsWith(p) && court.length > p.length) return court.slice(p.length).trim();
  return court || raison;
}
