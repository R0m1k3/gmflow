<script lang="ts">
  // État : pastille saturée et texte foncé pour les tons de sens, texte seul pour le neutre : <Etiquette ton="erreur">Envoi échoué</Etiquette>.
  import type { Snippet } from 'svelte';
  import type { Ton } from '#lib/libelles.ts';

  let { ton = 'neutre', point = false, title, children }: { ton?: Ton; point?: boolean; title?: string; children: Snippet } =
    $props();
</script>

<span class="etiquette {ton === 'neutre' ? '' : ton}" {title}>{#if point}<span class="point"></span>{/if}{@render children()}</span>
