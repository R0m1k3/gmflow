<script lang="ts">
  // État vide : une phrase qui dit où on en est, éventuellement une action.
  import type { Snippet } from 'svelte';
  import Icone, { type NomIcone } from './Icone.svelte';

  let {
    titre,
    texte,
    icone,
    compact = false,
    children
  }: { titre: string; texte?: string; icone?: NomIcone; compact?: boolean; children?: Snippet } = $props();
</script>

<div class="vide" class:compact>
  {#if icone}<Icone nom={icone} />{/if}
  <div class="vide-titre">{titre}</div>
  {#if texte}<p class="vide-texte">{texte}</p>{/if}
  {#if children}<div class="vide-actions">{@render children()}</div>{/if}
</div>
