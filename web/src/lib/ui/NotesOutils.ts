// Outils sans dépendance (testés par `npm test`) pour les notes internes et l'écartement par
// lot (audit U23).
import type { MotifEcartement, ResultatEcart } from '#lib/types.ts';

/** Début d'une note, sur une ligne : pour le survol de la pastille « Note » des listes. */
export function apercuNote(texte: string | null | undefined, max = 80): string {
  const t = (texte ?? '').replace(/\s+/g, ' ').trim();
  if (t.length <= max) return t;
  return `${t.slice(0, max - 1).trimEnd()}…`;
}

/** Bulle de la pastille « Note » d'une ligne de liste. */
export function bulleNotes(n: number | undefined, derniere: string | null | undefined): string {
  if (!n) return '';
  const quoi = n > 1 ? `${n} notes internes` : '1 note interne';
  const a = apercuNote(derniere);
  return a ? `${quoi}. Dernière : « ${a} »` : `${quoi}.`;
}

/** Réponse de `POST /api/documents/lot/ecarter`. */
export type ResultatEcartLot = ResultatEcart;

/** « 3 écartées, 1 laissée (déjà envoyée). » */
export function resumeEcartLot(r: ResultatEcartLot): string {
  const e = `${r.ecartees} écartée${r.ecartees > 1 ? 's' : ''}`;
  if (!r.laissees) return `${e}.`;
  const raisons = [...new Set(r.pieces.filter((p) => !p.ecartee).map((p) => (p.raison ?? '').trim().toLowerCase()).filter(Boolean))];
  return `${e}, ${r.laissees} laissée${r.laissees > 1 ? 's' : ''}${raisons.length ? ` (${raisons.join(', ')})` : ''}.`;
}

/** Corps de l'écartement : un motif est demandé ; la précision est facultative. */
export function corpsEcart(motif: MotifEcartement | null, texte: string): { motif: MotifEcartement; texte: string | null } | null {
  if (!motif) return null;
  const t = texte.trim();
  return { motif, texte: t || null };
}
