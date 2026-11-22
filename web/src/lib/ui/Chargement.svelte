<script lang="ts">
  // Indicateur de chargement : n'apparaît qu'après 300 ms, pour ne pas clignoter.
  let { texte = 'Chargement', compact = false }: { texte?: string; compact?: boolean } = $props();
</script>

<div class="chargement" class:compact role="status" aria-live="polite">
  <span class="roue" aria-hidden="true"></span>{texte}
</div>
