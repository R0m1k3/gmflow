<script lang="ts">
  // Bloc repliable d'une fiche : icône, titre, résumé d'une ligne, chevron.
  // Mettre plusieurs <Repli> dans un <div class="replis"> (filets entre eux).
  import type { Snippet } from 'svelte';
  import Icone, { type NomIcone } from './Icone.svelte';

  let {
    titre,
    resume,
    icone,
    ouvert = $bindable(false),
    children
  }: { titre: string; resume?: string; icone?: NomIcone; ouvert?: boolean; children: Snippet } = $props();
</script>

<details class="repli" bind:open={ouvert}>
  <summary>
    {#if icone}<Icone nom={icone} />{/if}
    <span>{titre}</span>
    {#if resume}<span class="resume-repli">{resume}</span>{/if}
    <Icone nom="droite" class="chev" />
  </summary>
  <div class="dedans">{@render children()}</div>
</details>
