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
  a_lire: 'à lire',
  en_cours: 'lecture en cours',
  lu: 'lu',
  ignore: 'ignoré',
  erreur: 'en erreur'
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

export const libelleTache: Record<string, string> = {
  extraction_texte: 'Extraction (PDF texte, tableur)',
  ocr_scan: 'Lecture des scans',
  nouvel_essai: 'Nouvel essai après échec',
  identification_fournisseur: 'Rapprochement des fournisseurs',
  test_jeton: 'Test du jeton'
};

export const STATUTS_DEPOT: StatutDepot[] = ['a_verifier', 'pret', 'en_envoi', 'depose', 'erreur', 'doublon'];

export const libelleStatutDepot: Record<StatutDepot, string> = {
  a_verifier: 'à vérifier',
  pret: 'prêt à déposer',
  en_envoi: 'envoi en cours',
  depose: 'déposé',
  erreur: 'dépôt en erreur',
  doublon: 'doublon'
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
