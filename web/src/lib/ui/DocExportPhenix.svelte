<script lang="ts" module>
  import type { EtatExportPhenix } from '#lib/types.ts';

  /** Résumé du bloc replié : « 12 articles, 2 lots détaillés, 3 alertes ». */
  export function resumeExport(e: EtatExportPhenix | null): string {
    if (!e) return '';
    if (e.en_cours) return 'recherche au magasin en cours';
    if (e.erreur) return 'recherche au magasin en échec';
    const m = [`${e.lignes} ligne${e.lignes > 1 ? 's' : ''}`];
    if (e.lots) m.push(`${e.lots} lot${e.lots > 1 ? 's' : ''} détaillé${e.lots > 1 ? 's' : ''}`);
    const n = alertes(e).reduce((t, g) => t + g.messages.length, 0);
    if (n) m.push(`${n} alerte${n > 1 ? 's' : ''}`);
    return m.join(', ');
  }

  /** Alertes des deux fichiers, sans doublon, regroupées par ligne du BL (générales d'abord). */
  export function alertes(e: EtatExportPhenix) {
    const groupes = new Map<number, { rang: number | null; article: string | null; messages: string[] }>();
    for (const a of [...e.matrice.alertes, ...e.trame.alertes]) {
      const k = a.rang ?? -1;
      const g = groupes.get(k) ?? { rang: a.rang, article: a.article, messages: [] };
      if (!g.messages.includes(a.message)) g.messages.push(a.message);
      groupes.set(k, g);
    }
    return [...groupes.entries()].sort(([a], [b]) => a - b).map(([, g]) => g);
  }
</script>

<script lang="ts">
  // Bloc « Export Phénix » d'une fiche BL : recherche des articles au magasin (lots détaillés),
  // alertes, et les deux fichiers : matrice d'intégration (commande) et trame BO Phénix
  // (fiches produits). Endpoints /api/documents/{id}/export-phenix.
  import { onDestroy } from 'svelte';
  import { api, message, sonder } from '#lib/api.ts';
  import Bandeau from './Bandeau.svelte';
  import { jourHeure } from './DocOutils.ts';
  import Icone from './Icone.svelte';

  let {
    id,
    etat = $bindable(null),
    onerreur
  }: { id: number; etat?: EtatExportPhenix | null; onerreur: (m: string) => void } = $props();

  let demande = $state(false);
  let arret: (() => void) | null = null;

  async function charger() {
    etat = await api.get<EtatExportPhenix>(`/api/documents/${id}/export-phenix`);
    return etat.en_cours;
  }

  function suivre() {
    arret?.();
    arret = sonder(charger, 2500);
  }

  $effect(() => {
    void id;
    charger()
      .then((enCours) => enCours && suivre())
      .catch((e) => onerreur(message(e)));
  });
  onDestroy(() => arret?.());

  async function verifier() {
    demande = true;
    try {
      await api.post(`/api/documents/${id}/export-phenix/verifier`);
      await charger();
      suivre();
    } catch (e) {
      onerreur(message(e));
    } finally {
      demande = false;
    }
  }

  let liste = $derived(etat ? alertes(etat) : []);
</script>

{#if etat}
  <div class="export">
    {#if etat.en_cours}
      <Bandeau ton="info" role="status">Recherche des articles au magasin en cours…</Bandeau>
    {:else if etat.erreur}
      <Bandeau ton="erreur" titre="Recherche au magasin impossible.">
        {etat.erreur}
        {#snippet action()}<button type="button" class="btn sm" onclick={verifier} disabled={demande}>Relancer</button>{/snippet}
      </Bandeau>
    {:else if etat.serveur && !etat.verifie_le}
      <Bandeau ton="attention" titre="Articles pas encore cherchés au magasin.">
        Les lots ne sont détaillés qu’après cette recherche.
        {#snippet action()}<button type="button" class="btn sm" onclick={verifier} disabled={demande}>Chercher au magasin</button>{/snippet}
      </Bandeau>
    {/if}
    {#if etat.a_relire}
      <Bandeau ton="attention">BL lu avant l’export : ni gencod ni colisage. « Relire en détail » pour les lire, s’ils sont imprimés.</Bandeau>
    {/if}

    <div class="boutons">
      {#if etat.pret}
        <a class="btn sm" href="/api/documents/{id}/export-phenix/matrice" download><Icone nom="telecharger" />Matrice d’intégration</a>
        <a class="btn sm" href="/api/documents/{id}/export-phenix/trame" download><Icone nom="telecharger" />Trame BO Phénix</a>
      {:else}
        <button type="button" class="btn sm" disabled><Icone nom="telecharger" />Matrice d’intégration</button>
        <button type="button" class="btn sm" disabled><Icone nom="telecharger" />Trame BO Phénix</button>
      {/if}
    </div>
    <p class="t2 petit">
      Matrice : la commande (gencod, référence, quantité, prix d’achat), {etat.matrice.rangees} ligne{etat.matrice.rangees > 1 ? 's' : ''}, fournisseur à choisir dans Phénix.
      Trame : les fiches produits, {etat.trame.rangees} ligne{etat.trame.rangees > 1 ? 's' : ''}{etat.code_fournisseur ? `, fournisseur ${etat.code_fournisseur}` : ''}.
    </p>
    {#if etat.verifie_le && !etat.erreur}
      <p class="t2 petit">
        Magasin, le {jourHeure(etat.verifie_le)} : {etat.trouves} article{etat.trouves > 1 ? 's' : ''} sur {etat.lignes} retrouvé{etat.trouves > 1 ? 's' : ''}{etat.lots ? `, ${etat.lots} lot${etat.lots > 1 ? 's' : ''} remplacé${etat.lots > 1 ? 's' : ''} par ${etat.composants} composants` : ''}.
        <button type="button" class="btn-lien" onclick={verifier} disabled={demande || etat.en_cours}>Chercher de nouveau</button>
      </p>
    {/if}
    {#if liste.length}
      <ul class="alertes">
        {#each liste as g (g.rang ?? -1)}
          <li>
            {#if g.rang !== null}<span class="quelle">Ligne {g.rang + 1}{g.article ? `, ${g.article}` : ''}</span>{' : '}{/if}{g.messages.join(' ; ')}.
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .export :global(.bandeau) {
    margin-bottom: var(--e2);
  }
  .boutons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
    margin: 0 0 var(--e2);
  }
  p {
    margin: 0 0 var(--e2);
  }
  .alertes {
    margin: 0;
    padding-left: 18px;
    color: var(--text2);
  }
  .alertes li {
    margin-bottom: 2px;
  }
  .quelle {
    color: var(--text);
    font-weight: 500;
  }
</style>
