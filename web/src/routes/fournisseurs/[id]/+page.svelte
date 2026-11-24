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

<div class="barre-h">
  <a class="retour" href={id === null ? '/fournisseurs' : `/fournisseurs?fiche=${id}`}><Icone nom="gauche" />Fournisseurs</a>
  <span class="sp"></span>
  <a class="btn-h large-seul" href={id === null ? '/fournisseurs' : `/fournisseurs?fiche=${id}`}>Quitter le plein écran <Kbd t="Escape" /></a>
</div>
<div class="page-f">
  {#if id === null}
    <Vide titre="Fournisseur introuvable" icone="camion"><a class="btn" href="/fournisseurs">Voir les fournisseurs</a></Vide>
  {:else}
    {#key id}
      <FournFiche {id} onfusion={(cible) => goto(`/fournisseurs/${cible}`, { replace: true })} />
    {/key}
  {/if}
</div>

<style>
  .barre-h {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 56px;
    padding: 0 32px;
    border-bottom: 1px solid var(--line);
    background: var(--white);
    font-size: 15px;
    color: var(--text2);
  }
  .btn-h {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: 8px;
    color: var(--text2);
    text-decoration: none;
  }
  .btn-h:hover {
    background: var(--pane);
    color: var(--text);
  }
  .page-f {
    padding: 32px;
    background: var(--white);
    min-height: calc(100dvh - var(--haut-barre) - 56px);
  }
  @media (max-width: 760px) {
    .barre-h {
      padding: 0 var(--e4);
    }
    .page-f {
      padding: var(--e5) var(--e4) var(--e8);
    }
  }
</style>
