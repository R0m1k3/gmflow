import type { EtatDocument, TypeDocument, Voie } from './types.ts';

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
  test_jeton: 'Test du jeton'
};
