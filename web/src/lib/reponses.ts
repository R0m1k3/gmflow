import type { Categorie, Priorite, StatutBrouillon, StatutReponse } from './types.ts';

export const CATEGORIES: Categorie[] = ['question', 'information', 'document_seul', 'notification', 'publicite'];

export const libelleCategorie: Record<Categorie, string> = {
  question: 'Question',
  information: 'Information',
  document_seul: 'Document seul',
  publicite: 'Publicité',
  notification: 'Notification'
};

export const classeCategorie: Record<Categorie, string> = {
  question: 'att',
  information: '',
  document_seul: 'ok',
  publicite: '',
  notification: ''
};

export const libelleCategoriePar: Record<string, string> = {
  regle: 'par règle',
  claude: 'par Claude',
  humain: 'à la main'
};

export const libellePriorite: Record<Priorite, string> = {
  haute: 'haute',
  normale: 'normale',
  basse: 'basse'
};

export const classePriorite: Record<Priorite, string> = {
  haute: 'ko',
  normale: 'att',
  basse: ''
};

export const libelleStatutReponse: Record<StatutReponse, string> = {
  a_repondre: 'à répondre',
  repondu: 'répondu',
  sans_suite: 'sans suite'
};

export const libelleBrouillon: Record<StatutBrouillon, string> = {
  generation: 'brouillon en préparation',
  propose: 'brouillon proposé',
  modifie: 'brouillon modifié',
  envoi: 'envoi en cours',
  envoye: 'envoyé',
  abandonne: 'abandonné'
};
