// Notes internes d'une pièce (`crates/gmflow-app/src/routes/notes.rs`) et écartement par lot.
import { api } from '#lib/api.ts';
import type { MotifEcartement } from '#lib/types.ts';
import type { ResultatEcartLot } from '#lib/ui/NotesOutils.ts';

/** Note interne : jamais envoyée à l'extérieur (ni Dext/Yooz, ni réponse au fournisseur). */
export interface NoteInterne {
  id: number;
  auteur_id: number | null;
  auteur_nom: string;
  texte: string;
  cree_le: string;
  /** Son auteur, ou un administrateur. */
  supprimable: boolean;
}

/** Longueur maximale d'une note (même valeur que le serveur). */
export const NOTE_MAX = 2000;

export const notesDocument = (id: number) => api.get<NoteInterne[]>(`/api/documents/${id}/notes`);
export const ajouterNote = (id: number, texte: string) => api.post<NoteInterne>(`/api/documents/${id}/notes`, { texte });
export const supprimerNote = (noteId: number) => api.del<void>(`/api/notes/${noteId}`);

/** « Écarter N pièces » : même motif pour toutes, résultat par pièce. */
export const ecarterLot = (ids: number[], motif: MotifEcartement, texte: string | null) =>
  api.post<ResultatEcartLot>('/api/documents/lot/ecarter', { ids, motif, texte });
