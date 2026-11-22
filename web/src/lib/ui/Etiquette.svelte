<script lang="ts">
  // État en texte coloré (pas de pastille) : <Etiquette ton="erreur">Envoi échoué</Etiquette>.
  import type { Snippet } from 'svelte';
  import type { Ton } from '#lib/libelles.ts';

  let { ton = 'neutre', point = false, title, children }: { ton?: Ton; point?: boolean; title?: string; children: Snippet } =
    $props();
</script>

<span class="etiquette {ton === 'neutre' ? '' : ton}" {title}>{#if point}<span class="point"></span>{/if}{@render children()}</span>
