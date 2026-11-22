<script lang="ts">
  // Touche(s) de raccourci : <Kbd t="J" />, <Kbd t={['Ctrl', 'Enter']} />. Masquées sur téléphone.
  let { t }: { t: string | string[] } = $props();

  const NOMS: Record<string, string> = {
    Enter: '↵',
    Escape: 'Échap',
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Backspace: '⌫',
    Tab: 'Tab',
    Maj: 'Maj',
    Ctrl: 'Ctrl'
  };
  let touches = $derived((Array.isArray(t) ? t : t.split('+')).map((k) => NOMS[k] ?? (k.length === 1 ? k.toUpperCase() : k)));
</script>

{#if touches.length === 1}
  <kbd>{touches[0]}</kbd>
{:else}
  <span class="touches">{#each touches as k, i (i)}<kbd>{k}</kbd>{/each}</span>
{/if}
