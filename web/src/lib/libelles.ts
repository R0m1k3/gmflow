// Vocabulaire affiché (docs/architecture.md, §4). Toute page passe par ces fonctions :
// aucun code technique (a_verifier, facture_citee…) ne doit arriver à l'écran.
import type {
  Categorie,
  Controle,
  EtatDocument,
  MotifEcartement,
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
  ignore: 'Écarté',
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
      return 'À contrôler';
    case 'pret':
      return 'Validée';
    case 'en_envoi':
      return 'Envoi en cours';
    case 'depose':
      return `Envoyée à ${nomOutil(outil)}`;
    case 'erreur':
      return 'Échec d’envoi';
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

/** Voie de lecture de l'écran « Qualité de la lecture » : voie, suivie de `+ocr` (texte des pages
 * scannées lu par l'OCR local) ou de `+cellules` (lignes lues dans les cellules du tableur). */
export function libelleVoieQualite(cle: string | null | undefined): string {
  const [v, precision] = (cle ?? '').split('+');
  const l = libelleVoie((v || null) as Voie | null);
  if (!l) return 'Voie inconnue';
  const suite = precision === 'ocr' ? ', texte reconnu sur le serveur (OCR)' : precision === 'cellules' ? ', lignes lues dans les cellules' : '';
  return l.charAt(0).toUpperCase() + l.slice(1) + suite;
}

/** Nom d'un champ corrigé à la main (clé de `document.corrections`). */
export function libelleChampCorrige(champ: string): string {
  const noms: Record<string, string> = {
    type: 'Type de pièce',
    societe_id: 'Société',
    fournisseur_id: 'Fournisseur',
    numero: 'Numéro',
    date_document: 'Date',
    numero_commande: 'N° de commande',
    montant_ht: 'Montant HT',
    montant_tva: 'TVA',
    montant_ttc: 'Montant TTC',
    date_echeance: 'Échéance'
  };
  return noms[champ] ?? champ;
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
    case 'lignes':
      return 'Lignes ≠ total HT';
    case 'iban':
      return 'IBAN changé';
    default:
      return 'À vérifier';
  }
}
/** Détail d'une alerte sous son titre : le message du serveur, sauf s'il ne fait que répéter le titre. */
export function detailAlerte(c: Pick<Controle, 'code' | 'message'>): string {
  const m = c.message.trim();
  if (c.code === 'lignes') {
    // « la somme des lignes (1633.34 €) ne fait pas le total HT (1671.03 €) » : les chiffres seuls
    const [s, t] = [...m.matchAll(/\((-?[\d.]+) €\)/g)].map((x) => Number(x[1]));
    if (s !== undefined && t !== undefined) {
      const e = (n: number) => new Intl.NumberFormat('fr-FR', { style: 'currency', currency: 'EUR' }).format(n);
      return `lignes ${e(s)}, total HT ${e(t)}, écart ${e(Math.abs(t - s))}`;
    }
  }
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
  serveurApi: 'Serveur du magasin',
  logiflow: 'Logiflow',
  utilisateurs: 'Utilisateurs',
  traitements: 'Traitements',
  avance: 'Avancé',
  expediteursPub: 'Expéditeurs publicitaires',
  historique: 'Historique',
  journal: 'Journal',
  zoneDangereuse: 'Zone dangereuse',
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

// ---------- Aide (petit « ? », composant Aide) ----------

/** Notions métier expliquées en une ou deux phrases. Chaque texte décrit ce que fait l'application. */
export const AIDES = {
  ecart_bl: {
    titre: 'Écart BL',
    texte: 'Un article de la facture manque sur les bons de livraison liés, est en trop, ou n’a pas le même prix.'
  },
  remise: {
    titre: 'Remise',
    texte:
      'Remise lue au pied de la facture. Le total HT est déjà net de remise : si les BL sont avant remise, la remise explique l’écart de montant.'
  },
  rapprochement: {
    titre: 'Rapprochement',
    texte:
      'GMFlow relie chaque facture à ses bons de livraison : par numéro de BL, par facture citée sur le BL ou par numéro de commande. On peut aussi les lier à la main.'
  },
  doublon_document: {
    titre: 'Doublon',
    texte:
      'Le même fichier, ou une pièce du même fournisseur avec le même numéro et le même montant TTC, déjà reçue. Un doublon n’est pas envoyé.'
  },
  doublon_fiche: {
    titre: 'Doublon de fiche',
    texte:
      'Deux fiches qui désignent sans doute le même fournisseur (noms proches, même domaine mail). Vous gardez l’une, l’autre est fusionnée avec ses documents.'
  },
  code_bo: {
    titre: 'Code BO Phénix',
    texte: 'Le code du fournisseur dans BO Phénix. Il sert à l’export vers Phénix et à retrouver le fournisseur dans Logiflow.'
  }
} as const;
export type SujetAide = keyof typeof AIDES;

/** Motifs rapides d'« Écarter » (fiche). */
export const MOTIFS_ECARTEMENT: { valeur: MotifEcartement; libelle: string }[] = [
  { valeur: 'doublon', libelle: 'Doublon' },
  { valeur: 'pas_pour_nous', libelle: 'Pas pour nous' },
  { valeur: 'publicite', libelle: 'Publicité' },
  { valeur: 'autre', libelle: 'Autre' }
];
export function libelleMotif(m: MotifEcartement | null | undefined): string {
  return MOTIFS_ECARTEMENT.find((x) => x.valeur === m)?.libelle ?? '';
}
