<script lang="ts">
  // Bandeau d'erreur de l'API (U8) : ce qui s'est passé, que faire, et le bouton qui le fait
  // (réessayer, ouvrir les réglages concernés, se reconnecter). Traduction : #lib/erreurs.ts.
  // `titre` : ce qui n'a pas marché (« Liste non chargée. ») ; sinon le message de l'erreur.
  // `onreessayer` : relance l'action ; sans lui, « Réessayer » recharge la page.
  import { page } from '$app/state';
  import { expliquer } from '#lib/erreurs.ts';
  import { session } from '#lib/session.svelte.ts';
  import Bandeau from './Bandeau.svelte';

  let {
    erreur,
    titre,
    onreessayer,
    compact = false
  }: { erreur: unknown; titre?: string; onreessayer?: () => unknown; compact?: boolean } = $props();

  let x = $derived(expliquer(erreur, session.moi?.role === 'admin'));
  let action = $derived(x.action);
  /** Reconnexion : la page de connexion ramène ici ensuite. */
  let retour = $derived(`/connexion?retour=${encodeURIComponent(page.url.pathname + page.url.search)}`);
  let enCours = $state(false);

  async function reessayer() {
    if (!onreessayer) return location.reload();
    enCours = true;
    try {
      await onreessayer();
    } finally {
      enCours = false;
    }
  }
</script>

{#snippet boutons()}
  {#if action?.genre === 'reessayer'}
    <button type="button" class="btn sm" class:en-cours={enCours} disabled={enCours} onclick={reessayer}>{action.libelle}</button>
  {:else if action?.genre === 'lien'}
    <a class="btn sm" href={action.href}>{action.libelle}</a>
  {:else if action?.genre === 'reconnecter'}
    <a class="btn sm" href={retour}>{action.libelle}</a>
  {/if}
{/snippet}

<Bandeau ton="erreur" titre={titre ?? x.message} action={action ? boutons : undefined}>
  {#if titre}{x.message}{/if}
  {#if x.quoiFaire}<span class="quoi" class:compact>{x.quoiFaire}</span>{/if}
</Bandeau>

<style>
  .quoi {
    display: block;
    color: var(--text2);
  }
  .quoi.compact {
    display: inline;
  }
</style>
