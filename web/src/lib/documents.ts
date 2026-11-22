import type { EtatDocument, StatutDepot, TypeDocument, Voie } from './types.ts';

export const TYPES: TypeDocument[] = ['facture', 'avoir', 'bl', 'relance', 'devis', 'autre'];

export const libelleType: Record<TypeDocument, string> = {
  facture: 'Facture',
  avoir: 'Avoir',
  bl: 'Bon de livraison',
  relance: 'Relance',
  devis: 'Devis',
  autre: 'Autre'
};

export const libelleEtat: Record<EtatDocument, string> = {
  a_lire: 'À lire par Claude',
  en_cours: 'Lecture en cours',
  lu: 'Lu',
  ignore: 'Sans intérêt',
  erreur: 'Non lu'
};

export const classeEtat: Record<EtatDocument, string> = {
  a_lire: 'att',
  en_cours: 'att',
  lu: 'ok',
  ignore: '',
  erreur: 'ko'
};

export const libelleVoie: Record<Voie, string> = {
  facturx: 'Factur-X',
  xml: 'XML',
  pdf_texte: 'PDF texte',
  tableur: 'Tableur',
  scan: 'Scan (Claude)'
};

/** Phrase de contexte pour un état de lecture : ce qui s'est passé et ce qu'il faut faire. */
export const aideEtat: Record<EtatDocument, string> = {
  a_lire: 'Claude va lire ce document.',
  en_cours: 'Claude lit ce document, la page se met à jour toute seule.',
  lu: 'Claude a lu ce document : vérifiez les données.',
  ignore: 'Ce document a été écarté : ce n’est pas une facture, un avoir ni un bon de livraison.',
  erreur: 'Claude n’a pas pu lire ce document.'
};

export const libelleTache: Record<string, string> = {
  extraction_texte: 'Extraction (PDF texte, tableur)',
  ocr_scan: 'Lecture des scans',
  nouvel_essai: 'Nouvel essai après échec',
  identification_fournisseur: 'Rapprochement des fournisseurs',
  tri_mail: 'Tri des mails',
  brouillon_reponse: 'Brouillons de réponse',
  test_jeton: 'Test du jeton'
};

export const STATUTS_DEPOT: StatutDepot[] = ['a_verifier', 'pret', 'en_envoi', 'depose', 'erreur', 'doublon'];

export const libelleStatutDepot: Record<StatutDepot, string> = {
  a_verifier: 'À vérifier',
  pret: 'Prête à déposer',
  en_envoi: 'Dépôt en cours',
  depose: 'Déposée',
  erreur: 'Refusée par Dext ou Yooz',
  doublon: 'Déjà déposée'
};

export const classeStatutDepot: Record<StatutDepot, string> = {
  a_verifier: 'att',
  pret: 'att',
  en_envoi: 'att',
  depose: 'ok',
  erreur: 'ko',
  doublon: 'ko'
};

export const libelleOutil: Record<string, string> = { dext: 'Dext', yooz: 'Yooz' };

export const libelleMethode: Record<string, string> = {
  numero_bl: 'la facture cite le BL',
  facture_citee: 'le BL cite la facture',
  commande: 'même n° de commande',
  manuel: 'lien manuel'
};

/** Écart de montant HT au-delà duquel une facture est signalée (comme côté serveur). */
export const SEUIL_ECART = 0.05;

/** Codes d'unité UN/ECE des factures électroniques : libellé court, sinon le texte tel quel. */
const UNITES: Record<string, string> = {
  C62: 'u',
  H87: 'u',
  EA: 'u',
  XPP: 'u',
  KGM: 'kg',
  GRM: 'g',
  LTR: 'L',
  MTR: 'm',
  MTK: 'm²',
  MTQ: 'm³',
  HUR: 'h',
  DAY: 'j',
  SET: 'lot',
  XBX: 'boîte',
  XCT: 'carton',
  XPK: 'colis',
  XPA: 'paquet'
};

export function libelleUnite(u: string | null): string {
  if (!u) return '';
  return UNITES[u.trim().toUpperCase()] ?? u;
}
