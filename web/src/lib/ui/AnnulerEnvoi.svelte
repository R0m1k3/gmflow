<script lang="ts">
  // Messages « Annuler » en bas de l'écran après une validation : « Facture Vileda F-12 validée.
  // Envoi à Dext dans 8 s. Annuler ». Ne bloque rien : la pièce suivante s'ouvre comme avant.
  // Les trois derniers seulement ; Ctrl+Z annule le plus récent. Posé une fois dans la mise en page.
  import { annulations, MAX_VISIBLES } from '#lib/annulations.svelte.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import Bandeau from './Bandeau.svelte';

  let maintenant = $state(Date.now());
  const visibles = $derived(annulations.liste.slice(-MAX_VISIBLES));

  $effect(() => {
    if (annulations.liste.length === 0) return;
    const t = setInterval(() => (maintenant = Date.now()), 250);
    return () => clearInterval(t);
  });

  $effect(() =>
    raccourcis({
      'Ctrl+z': () => {
        const m = annulations.dernier;
        if (!m) return false;
        void annulations.annuler(m.cle);
      }
    })
  );

  function reste(fin: number): number {
    return Math.max(0, Math.ceil((fin - maintenant) / 1000));
  }
</script>

<div class="pile" aria-live="polite" role="status">
  {#each visibles as m (m.cle)}
    <div class="message">
      {#if m.etat === 'fini'}
        <Bandeau ton={m.ton === 'info' ? 'ok' : m.ton}>
          <span>{m.resultat}</span>
          {#snippet action()}
            <button type="button" class="btn sm fantome" onclick={() => annulations.fermer(m.cle)}>Fermer</button>
          {/snippet}
        </Bandeau>
      {:else}
        <Bandeau ton="ok" titre="{m.titre}.">
          <span>
            {#if m.etat === 'annulation'}Annulation…{:else}Envoi à {m.outil} dans <span class="n">{reste(m.fin)}&nbsp;s</span>.{/if}
          </span>
          {#snippet action()}
            <button
              type="button"
              class="btn sm"
              class:en-cours={m.etat === 'annulation'}
              disabled={m.etat !== 'attente'}
              title="Annuler la validation (Ctrl+Z)"
              onclick={() => annulations.annuler(m.cle)}>Annuler</button
            >
          {/snippet}
        </Bandeau>
      {/if}
    </div>
  {/each}
</div>

<style>
  .pile {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(560px, calc(100vw - 32px));
    pointer-events: none;
  }
  .message {
    pointer-events: auto;
    border-radius: var(--r2);
    box-shadow: var(--ombre-page);
    background: var(--pane);
  }
  .message :global(.bandeau) {
    margin-bottom: 0;
    align-items: center;
  }
  .message :global(.action-bandeau) {
    margin: -4px -6px -4px 0;
  }
  .n {
    font-variant-numeric: tabular-nums;
  }
  /* Téléphone : au-dessus des onglets du bas ; boutons de 44 px. */
  @media (max-width: 760px) {
    .pile {
      bottom: calc(72px + env(safe-area-inset-bottom));
    }
    .message :global(.btn.sm) {
      min-height: 44px;
    }
  }
</style>
