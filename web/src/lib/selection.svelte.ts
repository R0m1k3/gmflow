// Pièces cochées pour « Valider N pièces » (audit U2), dans les listes Factures et Avoirs,
// filtre « À contrôler ». Seules les pièces sans alerte sont cochables (même règle que le
// serveur, `POST /api/documents/lot/eligibilite`). La sélection survit au rafraîchissement de
// la liste ; les pièces disparues ou devenues non cochables en sortent.
import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { eligibiliteLot, validerLot } from './api.ts';
import type { EligibiliteLot, ResultatLot } from './types.ts';

/** Pièces au plus par appel au serveur (borne de l'API). */
export const MAX_LOT = 100;

export class Selection {
  coches = new SvelteSet<number>();
  /** Éligibilité connue de chaque pièce affichée. */
  etats = new SvelteMap<number, EligibiliteLot>();
  #tour = 0;

  /** Liste (re)chargée : éligibilité relue, sélection réduite aux pièces encore là et cochables. */
  async actualiser(ids: number[]) {
    const moi = ++this.#tour;
    const presents = new Set(ids);
    for (const id of [...this.coches])
      if (!presents.has(id)) this.coches.delete(id);
    for (const id of [...this.etats.keys()])
      if (!presents.has(id)) this.etats.delete(id);
    const lus: EligibiliteLot[] = [];
    try {
      for (let i = 0; i < ids.length; i += MAX_LOT)
        lus.push(...(await eligibiliteLot(ids.slice(i, i + MAX_LOT))));
    } catch {
      return; // on garde ce qu'on savait ; la prochaine relecture réessaie
    }
    if (moi !== this.#tour) return;
    for (const e of lus) this.etats.set(e.id, e);
    for (const id of [...this.coches])
      if (!this.etats.get(id)?.possible) this.coches.delete(id);
  }

  possible(id: number): boolean {
    return this.etats.get(id)?.possible === true;
  }

  raison(id: number): string {
    return this.etats.get(id)?.raison ?? '';
  }

  basculer(id: number) {
    if (this.coches.has(id)) this.coches.delete(id);
    else if (this.possible(id)) this.coches.add(id);
  }

  /** Cochables parmi `ids` (les lignes chargées). */
  cochables(ids: number[]): number[] {
    return ids.filter((id) => this.possible(id));
  }

  toutCocher(ids: number[]) {
    for (const id of this.cochables(ids).slice(0, MAX_LOT)) this.coches.add(id);
  }

  rien() {
    this.coches.clear();
  }

  /** Valide les pièces cochées, dans l'ordre de la liste. */
  async valider(ordre: number[]): Promise<ResultatLot> {
    const ids = ordre.filter((id) => this.coches.has(id)).slice(0, MAX_LOT);
    const r = await validerLot(ids);
    for (const p of r.pieces) if (p.validee) this.coches.delete(p.id);
    return r;
  }
}

/** « 14 validées, 3 laissées à contrôler (alertes) ». */
export function resumeLot(r: ResultatLot): string {
  const v = `${r.validees} validée${r.validees > 1 ? 's' : ''}`;
  if (!r.laissees) return `${v}.`;
  const raisons = [
    ...new Set(r.pieces.filter((p) => !p.validee).map((p) => motCle(p.raison))),
  ].join(', ');
  return `${v}, ${r.laissees} laissée${r.laissees > 1 ? 's' : ''} à contrôler (${raisons}).`;
}

function motCle(raison: string | null): string {
  const r = (raison ?? '').toLowerCase();
  if (r.includes('alerte')) return 'alertes';
  if (r.includes('doublon')) return 'doublon';
  if (r.includes('déjà')) return 'déjà validée';
  if (r.includes('société')) return 'autre société';
  return 'à ouvrir';
}
