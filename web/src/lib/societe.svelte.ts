// Société choisie dans la barre du haut, partagée par toutes les pages.
// Admin : `null` = toutes les sociétés, sinon l'id choisi, gardé dans localStorage.
// Responsable : pas de sélecteur, le serveur filtre déjà sur sa société.
import { api } from './api.ts';
import type { SocieteVisible } from './types.ts';

const CLE = 'gmflow.societe';

/** « SAS Frouard Distribution » : « Frouard Distribution ». */
export function sansFormeJuridique(raison: string): string {
  return raison.replace(/^(SASU|SAS|SARL|EURL|SA|SNC|SCI)\s+/i, '').trim();
}

/** Mots du début communs à tous les noms (au moins deux noms, et il reste un mot à chacun). */
function debutCommun(noms: string[]): string {
  if (noms.length < 2) return '';
  const mots = noms.map((n) => n.split(/\s+/));
  const commun: string[] = [];
  for (let i = 0; mots.every((m) => i < m.length - 1 && m[i] === mots[0][i]); i++) commun.push(mots[0][i]);
  return commun.join(' ');
}

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

  /** Début commun des raisons sociales (« La Foir'Fouille »), retiré des noms courts. */
  prefixe = $derived(debutCommun(this.liste.map((s) => sansFormeJuridique(s.raison_sociale))));

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
