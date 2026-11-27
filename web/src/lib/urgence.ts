// Raison de la place d'une pièce dans la liste de travail de l'accueil (U5) : la file « À valider »
// arrive triée par urgence (`rang` du serveur : 1 échéance dans 7 jours ou dépassée, 2 validable
// d'un clic, 3 à corriger). Module pur : testé par `node --test` (urgence.test.ts).

export interface PieceUrgente {
  rang: 1 | 2 | 3;
  date_echeance: string | null;
}

export interface Raison {
  texte: string;
  /** `e` : dépassée ou aujourd'hui ; `w` : bientôt ; `ok` : validable d'un clic. */
  ton: 'e' | 'w' | 'ok';
}

const JOUR = 86_400_000;
const jours = (a: string, b: string) => Math.round((Date.parse(`${a}T00:00:00Z`) - Date.parse(`${b}T00:00:00Z`)) / JOUR);

/**
 * « Échéance dépassée depuis 3 jours », « À payer demain », « Validable d’un clic » ; `null`
 * pour une pièce à corriger (son alerte dit déjà pourquoi). `aujourdhui` : AAAA-MM-JJ (Paris).
 */
export function raisonUrgence(p: PieceUrgente, aujourdhui: string): Raison | null {
  if (p.rang === 2) return { texte: 'Validable d’un clic', ton: 'ok' };
  if (p.rang !== 1 || !p.date_echeance) return null;
  const n = jours(p.date_echeance, aujourdhui);
  if (n < 0) return { texte: `Échéance dépassée depuis ${-n} jour${n < -1 ? 's' : ''}`, ton: 'e' };
  if (n === 0) return { texte: 'À payer aujourd’hui', ton: 'e' };
  if (n === 1) return { texte: 'À payer demain', ton: 'w' };
  return { texte: `À payer dans ${n} jours`, ton: 'w' };
}
