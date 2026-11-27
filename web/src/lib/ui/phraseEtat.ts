// Phrase d'état de la fiche (U21) : une seule phrase en tête, celle du problème le plus grave,
// puis une liste « Aussi : … » pour le reste. La fiche (`DocFiche.svelte`) dit quelles phrases
// s'appliquent, avec les conditions de ses anciens bandeaux ; ce module ne fait que les ranger.
// Aucune phrase n'est perdue, sauf le « tout va bien » quand une alerte existe.
// Module pur : testé par `node --test` (phraseEtat.test.ts).

export type TonPhrase = 'erreur' | 'attention' | 'info' | 'ok' | 'neutre';

/** Dans l'ordre de priorité : la première qui s'applique devient la phrase principale. */
export const ORDRE = [
  'iban',
  'envoi_echec',
  'doublon',
  'sans_adresse',
  'illisible',
  'trop_volumineux',
  'societe',
  'fournisseur',
  'lecture_incomplete',
  'montants_non_lus',
  'controle',
  'articles',
  'sans_bl',
  'echeance',
  'attente',
  'validee',
  'envoi_en_cours',
  'prete',
  'bl_attente',
  'sans_bl_info',
  'envoyee',
  'ecartee',
  'pas_une_piece',
  'decoupe'
] as const;
export type CodePhrase = (typeof ORDRE)[number];

/**
 * Pièce qui n'est plus (ou pas encore) à contrôler : son état passe devant les alertes, comme
 * avant (une facture envoyée s'annonce « Envoyée », même s'il lui reste un écart d'article).
 */
const HORS_CONTROLE: readonly CodePhrase[] = ['attente', 'validee', 'envoi_en_cours', 'envoyee', 'ecartee', 'pas_une_piece', 'decoupe'];

export interface Phrase {
  code: CodePhrase;
  ton: TonPhrase;
  /** En gras, ponctuation comprise (« Doublon. », « Montant incohérent : »). */
  titre: string;
  texte: string;
  /** Distingue deux phrases de même code (deux contrôles). */
  id?: string;
}

export interface PhraseEtat {
  principale: Phrase | null;
  aussi: Phrase[];
}

const rang = (p: Phrase) => (HORS_CONTROLE.includes(p.code) ? -1 : ORDRE.indexOf(p.code));

/** Range les phrases qui s'appliquent ; à rang égal, l'ordre reçu est gardé. */
export function rangerPhrases(phrases: readonly Phrase[]): PhraseEtat {
  const alerte = phrases.some((p) => p.ton === 'erreur' || p.ton === 'attention');
  const gardees = phrases
    .filter((p) => !(alerte && p.code === 'prete'))
    .map((p, i) => ({ p, i }))
    .sort((a, b) => rang(a.p) - rang(b.p) || a.i - b.i)
    .map((x) => x.p);
  return { principale: gardees[0] ?? null, aussi: gardees.slice(1) };
}

/** Phrase de la liste « Aussi » : titre et texte à la suite, sans majuscule (sauf sigle : « IBAN »). */
export function phraseCourte(p: Phrase): string {
  const t = `${p.titre} ${p.texte}`.replace(/\s+/g, ' ').trim();
  return /^[A-ZÀ-Ý][a-zà-ÿ’' ]/.test(t) ? t[0].toLowerCase() + t.slice(1) : t;
}

const JOUR = 86_400_000;

/**
 * « À payer dans 3 jours (30/11). », « Échéance dépassée depuis 2 jours (25/11). » ; `null` si
 * l'échéance est à plus de 7 jours. Dates AAAA-MM-JJ.
 */
export function phraseEcheance(echeance: string | null, aujourdhui: string): string | null {
  if (!echeance || !/^\d{4}-\d\d-\d\d$/.test(echeance)) return null;
  const n = Math.round((Date.parse(`${echeance}T00:00:00Z`) - Date.parse(`${aujourdhui}T00:00:00Z`)) / JOUR);
  if (Number.isNaN(n) || n > 7) return null;
  const le = `(${echeance.slice(8, 10)}/${echeance.slice(5, 7)})`;
  if (n < 0) return `Échéance dépassée depuis ${-n} jour${n < -1 ? 's' : ''} ${le}.`;
  if (n === 0) return `À payer aujourd’hui ${le}.`;
  if (n === 1) return `À payer demain ${le}.`;
  return `À payer dans ${n} jours ${le}.`;
}

/** Aujourd'hui à Paris, AAAA-MM-JJ. */
export const jourParis = (d = new Date()) => new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Paris' }).format(d);
