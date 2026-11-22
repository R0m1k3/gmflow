<script lang="ts">
  // Message d'une phrase en tête de zone. `titre` en gras, `action` à droite (un bouton .btn.sm).
  import type { Snippet } from 'svelte';
  import Icone, { type NomIcone } from './Icone.svelte';

  type TonBandeau = 'attention' | 'erreur' | 'ok' | 'info' | 'neutre';
  let {
    ton = 'attention',
    titre,
    role,
    children,
    action
  }: { ton?: TonBandeau; titre?: string; role?: 'alert' | 'status'; children?: Snippet; action?: Snippet } = $props();

  const ICONES: Record<TonBandeau, NomIcone> = {
    attention: 'alerte',
    erreur: 'erreur',
    ok: 'coche',
    info: 'info',
    neutre: 'info'
  };
</script>

<div class="bandeau {ton === 'neutre' ? '' : ton}" role={role ?? (ton === 'erreur' ? 'alert' : undefined)}>
  <Icone nom={ICONES[ton]} />
  <div class="corps-bandeau">
    {#if titre}<b>{titre}</b>{/if}
    {#if children}{@render children()}{/if}
  </div>
  {#if action}<div class="action-bandeau">{@render action()}</div>{/if}
</div>
