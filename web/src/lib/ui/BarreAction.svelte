<script lang="ts" module>
  export interface Action {
    libelle: string;
    /** Raccourci affiché dans le bouton (le raccourci lui-même se pose avec `raccourcis`). */
    touche?: string | string[];
    onclick?: () => void;
    href?: string;
    desactive?: boolean;
    enCours?: boolean;
    danger?: boolean;
  }
</script>

<script lang="ts">
  // Barre du bas d'un volet ou d'une fiche : une action principale, une ou deux secondaires,
  // et « Ensuite : … » (l'élément qui s'ouvrira). Fixée en bas de l'écran sur téléphone :
  // donner la classe .avec-barre au contenu qui défile pour qu'elle ne le cache pas.
  import type { Snippet } from 'svelte';
  import Kbd from './Kbd.svelte';

  let {
    principal,
    secondaires = [],
    ensuite,
    toucheEnsuite,
    aligne = false,
    children
  }: {
    principal: Action;
    secondaires?: Action[];
    ensuite?: string;
    toucheEnsuite?: string;
    /** Fiche plein écran : aligne les boutons sur la colonne des champs. */
    aligne?: boolean;
    children?: Snippet;
  } = $props();
</script>

{#snippet bouton(a: Action, principale: boolean)}
  {#if a.href && !a.desactive}
    <a class="btn" class:principal={principale} class:danger={a.danger} href={a.href}>
      {a.libelle}{#if a.touche}<Kbd t={a.touche} />{/if}
    </a>
  {:else}
    <button
      type="button"
      class="btn"
      class:principal={principale}
      class:danger={a.danger}
      class:en-cours={a.enCours}
      disabled={a.desactive || a.enCours}
      aria-busy={a.enCours ? 'true' : undefined}
      onclick={a.onclick}
    >
      {a.libelle}{#if a.touche && !a.enCours}<Kbd t={a.touche} />{/if}
    </button>
  {/if}
{/snippet}

<div class="barre-action" class:aligne>
  {@render bouton(principal, true)}
  {#each secondaires as a (a.libelle)}{@render bouton(a, false)}{/each}
  {#if children}{@render children()}{/if}
  {#if ensuite}
    <div class="ensuite">Ensuite <b>{ensuite}</b>{#if toucheEnsuite}<Kbd t={toucheEnsuite} />{/if}</div>
  {/if}
</div>
