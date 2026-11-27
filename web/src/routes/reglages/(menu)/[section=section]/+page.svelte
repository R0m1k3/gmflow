<script lang="ts">
  // Vue d'une section des réglages (`/reglages/<section>`, liste : #lib/reglages.ts).
  // `?ouvrir=1` (bouton de la mise en route) ouvre le formulaire de la section ; `?modifier=<id>`
  // ouvre la fiche d'une société. Le paramètre est retiré de l'adresse une fois servi.
  import { goto, replaceState } from '$app/navigation';
  import { page } from '$app/state';
  import { tick } from 'svelte';
  import type { CleSection } from '#lib/reglages.ts';
  import Avance from '../../Avance.svelte';
  import Boites from '../../Boites.svelte';
  import Claude from '../../Claude.svelte';
  import Envoi from '../../Envoi.svelte';
  import Logiflow from '../../Logiflow.svelte';
  import ServeurApi from '../../ServeurApi.svelte';
  import Societes from '../../Societes.svelte';
  import Traitements from '../../Traitements.svelte';
  import Utilisateurs from '../../Utilisateurs.svelte';
  import { donneesReglages } from '../donnees.svelte.ts';

  const d = donneesReglages();
  let section = $derived(page.params.section as CleSection);

  let refSocietes = $state<ReturnType<typeof Societes>>();
  let refUtilisateurs = $state<ReturnType<typeof Utilisateurs>>();
  let refEnvoi = $state<ReturnType<typeof Envoi>>();
  let refClaude = $state<ReturnType<typeof Claude>>();

  $effect(() => {
    const q = page.url.searchParams;
    const ouvrir = q.get('ouvrir') === '1';
    const modifier = Number(q.get('modifier'));
    if (!ouvrir && !modifier) return;
    const s = section;
    void tick().then(() => {
      if (modifier && s === 'societes') refSocietes?.modifier(modifier);
      else if (ouvrir && s === 'societes') refSocietes?.nouvelle();
      else if (ouvrir && s === 'utilisateurs') refUtilisateurs?.nouveau();
      else if (ouvrir && s === 'envoi') void refEnvoi?.ouvrir();
      else if (ouvrir && s === 'claude') refClaude?.connecter();
      const u = new URL(page.url.href);
      u.searchParams.delete('ouvrir');
      u.searchParams.delete('modifier');
      replaceState(u, page.state);
    });
  });
</script>

{#if section === 'societes'}
  <Societes bind:this={refSocietes} societes={d.societes} onchange={() => d.rafraichir(true)} />
{:else if section === 'utilisateurs'}
  <Utilisateurs bind:this={refUtilisateurs} utilisateurs={d.utilisateurs} societes={d.societes} onchange={() => d.rafraichir()} />
{:else if section === 'boites'}
  <Boites boites={d.boites} societes={d.societes} />
{:else if section === 'claude'}
  <Claude bind:this={refClaude} onchange={() => d.rafraichir()} />
{:else if section === 'envoi'}
  <Envoi
    bind:this={refEnvoi}
    societes={d.societes}
    modifierSociete={(id) => goto(`/reglages/societes?modifier=${id}`)}
    onchange={() => d.rafraichir()}
  />
{:else if section === 'serveur-api'}
  <ServeurApi />
{:else if section === 'logiflow'}
  <Logiflow />
{:else if section === 'traitements'}
  <Traitements />
{:else if section === 'avance'}
  <Avance onchange={() => d.rafraichir()} />
{:else if section === 'journal'}
  <Avance partie="journal" />
{:else if section === 'zone-dangereuse'}
  <Avance partie="danger" onchange={() => d.rafraichir()} />
{/if}
