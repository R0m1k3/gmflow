// Notes internes d'une pièce (`crates/gmflow-app/src/routes/notes.rs`) et écartement par lot.
import { api } from '#lib/api.ts';
import type { DemandeEcartLot, MotifEcartement, Note, NouvelleNote, ResultatEcart } from '#lib/types.ts';

/** Note interne : jamais envoyée à l'extérieur (ni Dext/Yooz, ni réponse au fournisseur). */
export type NoteInterne = Note;

/** Longueur maximale d'une note (même valeur que le serveur). */
export const NOTE_MAX = 2000;

export const notesDocument = (id: number) => api.get<NoteInterne[]>(`/api/documents/${id}/notes`);
export const ajouterNote = (id: number, texte: string) => api.post<NoteInterne>(`/api/documents/${id}/notes`, { texte } satisfies NouvelleNote);
export const supprimerNote = (noteId: number) => api.del<void>(`/api/notes/${noteId}`);

/** « Écarter N pièces » : même motif pour toutes, résultat par pièce. */
export const ecarterLot = (ids: number[], motif: MotifEcartement, texte: string | null) =>
  api.post<ResultatEcart>('/api/documents/lot/ecarter', { ids, motif, texte } satisfies DemandeEcartLot);
