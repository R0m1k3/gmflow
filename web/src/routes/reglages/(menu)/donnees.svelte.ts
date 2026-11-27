// Données partagées des vues de réglages (U10) : sociétés, boîtes, utilisateurs et mise en
// route, chargées une fois par la mise en page et relues après une modification. Les vues les
// lisent par le contexte (`donneesReglages()`).
import { getContext, setContext } from 'svelte';
import { api, message, miseEnRoute } from '#lib/api.ts';
import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
import { societe } from '#lib/societe.svelte.ts';
import type { Boite, MiseEnRoute, Societe, Utilisateur } from '#lib/types.ts';

export class DonneesReglages {
  societes = $state<Societe[]>([]);
  boites = $state<Boite[]>([]);
  utilisateurs = $state<Utilisateur[]>([]);
  etat = $state<MiseEnRoute | null>(null);
  charge = $state(false);
  erreur = $state('');

  private async listes() {
    [this.societes, this.boites, this.utilisateurs] = await Promise.all([
      api.get<Societe[]>('/api/admin/societes'),
      api.get<Boite[]>('/api/admin/boites'),
      api.get<Utilisateur[]>('/api/admin/utilisateurs')
    ]);
  }

  private async miseEnRoute() {
    try {
      this.etat = await miseEnRoute();
    } catch {
      /* la mise en route est un plus : les sections restent utilisables */
    }
  }

  charger = async () => {
    this.erreur = '';
    try {
      await Promise.all([this.listes(), this.miseEnRoute()]);
      this.charge = true;
    } catch (e) {
      this.erreur = message(e);
    }
  };

  /** Après une modification dans une section : listes, mise en route et sélecteur de société. */
  rafraichir = async (avecSocietes = false) => {
    await Promise.all([this.listes().catch((e) => (this.erreur = message(e))), this.miseEnRoute()]);
    if (avecSocietes) void societe.charger();
    // Claude connecté, boîte réparée… : la pastille d'état de la barre du haut suit.
    void rafraichirCompteurs();
  };
}

const CLE = Symbol('reglages');

export function fournirDonneesReglages(d: DonneesReglages) {
  setContext(CLE, d);
}

export function donneesReglages(): DonneesReglages {
  return getContext<DonneesReglages>(CLE);
}
