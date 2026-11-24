<script lang="ts">
  // Deux volets : la file à gauche (300 px), l'élément ouvert à droite (aperçu et fiche pour une pièce).
  // Sur téléphone, un seul volet à la fois : la liste, ou le détail quand `ouvert`.
  //   <Volets ouvert={!!courant}>
  //     {#snippet liste()} <div class="liste-h">…</div><div class="liste-s">…</div><div class="liste-f">…</div> {/snippet}
  //     {#snippet detail()} <div class="volet-h">…</div><div class="volet-b">…</div><BarreAction …/> {/snippet}
  //   </Volets>
  // Sans `detail` (rien d'ouvert), le volet de droite montre `vide` ou rien.
  import type { Snippet } from 'svelte';

  let {
    ouvert = false,
    largeur,
    libelleListe = 'Liste',
    liste,
    detail,
    vide
  }: {
    ouvert?: boolean;
    /** Largeur du volet liste, en px (300 par défaut). */
    largeur?: number;
    libelleListe?: string;
    liste: Snippet;
    detail?: Snippet;
    vide?: Snippet;
  } = $props();
</script>

<div class="volets" class:ouvert style:--larg-liste={largeur ? `${largeur}px` : undefined}>
  <aside class="volet-liste" aria-label={libelleListe}>{@render liste()}</aside>
  <section class="volet">
    {#if ouvert && detail}{@render detail()}{:else if vide}{@render vide()}{/if}
  </section>
</div>
