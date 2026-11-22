<script lang="ts">
  // Fiche fournisseur en plein écran (même contenu que le volet de /fournisseurs).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { raccourcis } from '#lib/clavier.ts';
  import FournFiche from '#lib/ui/FournFiche.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  let id = $derived(/^\d+$/.test(page.params.id ?? '') ? Number(page.params.id) : null);

  $effect(() => raccourcis({ Escape: () => void goto(id === null ? '/fournisseurs' : `/fournisseurs?fiche=${id}`) }));
</script>

<svelte:head><title>Fournisseur · GMFlow</title></svelte:head>

<div class="page etroite">
  <div class="page-h">
    <a class="retour" href={id === null ? '/fournisseurs' : `/fournisseurs?fiche=${id}`}><Icone nom="gauche" />Fournisseurs <Kbd t="Escape" /></a>
  </div>
  {#if id === null}
    <Vide titre="Fournisseur introuvable" icone="camion"><a class="btn" href="/fournisseurs">Voir les fournisseurs</a></Vide>
  {:else}
    {#key id}
      <FournFiche {id} onfusion={(cible) => goto(`/fournisseurs/${cible}`, { replace: true })} />
    {/key}
  {/if}
</div>
