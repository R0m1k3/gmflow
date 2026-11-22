// Société choisie dans la barre du haut, partagée par toutes les pages.
// Admin : `null` = toutes les sociétés, sinon l'id choisi, gardé dans localStorage.
// Responsable : pas de sélecteur, le serveur filtre déjà sur sa société.
import { api } from './api.ts';
import type { SocieteVisible } from './types.ts';

const CLE = 'gmflow.societe';

function lire(): number | null {
  try {
    const v = localStorage.getItem(CLE);
    const n = v ? Number(v) : NaN;
    return Number.isInteger(n) && n > 0 ? n : null;
  } catch {
    return null;
  }
}

class SocieteChoisie {
  /** Id de la société choisie, `null` pour toutes. */
  id = $state<number | null>(lire());
  /** Sociétés visibles par l'utilisateur (`GET /api/societes`). */
  liste = $state<SocieteVisible[]>([]);
  charge = $state(false);

  /** Société choisie (objet), `null` pour toutes. */
  get courante(): SocieteVisible | null {
    return this.liste.find((s) => s.id === this.id) ?? null;
  }

  choisir(id: number | null) {
    this.id = id;
    try {
      if (id === null) localStorage.removeItem(CLE);
      else localStorage.setItem(CLE, String(id));
    } catch {
      /* navigation privée : choix gardé pour la session seulement */
    }
  }

  async charger() {
    try {
      this.liste = await api.get<SocieteVisible[]>('/api/societes');
      // Une société disparue (archivée, droits changés) : retour à « Toutes ».
      if (this.id !== null && !this.liste.some((s) => s.id === this.id)) this.choisir(null);
    } catch {
      this.liste = [];
    }
    this.charge = true;
  }

  /** Paramètre à ajouter aux appels d'API : `societe=3`, ou '' pour toutes. */
  get parametre(): string {
    return this.id === null ? '' : `societe=${this.id}`;
  }

  /** Ajoute `societe=` à un chemin d'API s'il y a une société choisie. */
  url(chemin: string): string {
    if (this.id === null) return chemin;
    return `${chemin}${chemin.includes('?') ? '&' : '?'}societe=${this.id}`;
  }
}

export const societe = new SocieteChoisie();
