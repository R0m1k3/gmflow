<script lang="ts">
  // Petit « ? » qui explique une notion métier : se touche, se clique ou s'ouvre au clavier
  // (Entrée, Espace), se ferme avec Échap, un clic ailleurs ou le même bouton. Le texte vient de
  // `AIDES` (#lib/libelles.ts) : <Aide sujet="ecart_bl" />. `droite` : bulle alignée sur le bord droit
  // du bouton (pour un « ? » placé à droite d'un tableau).
  import { AIDES, type SujetAide } from '#lib/libelles.ts';

  let { sujet, droite = false }: { sujet: SujetAide; droite?: boolean } = $props();

  let ouvert = $state(false);
  let racine = $state<HTMLElement>();
  const uid = $props.id();
  let a = $derived(AIDES[sujet]);

  function dehors(e: Event) {
    if (ouvert && racine && e.target instanceof Node && !racine.contains(e.target)) ouvert = false;
  }
  function touche(e: KeyboardEvent) {
    if (e.key === 'Escape' && ouvert) {
      ouvert = false;
      e.stopPropagation();
    }
  }
</script>

<svelte:window onpointerdown={dehors} />

<span class="aide" bind:this={racine} onkeydown={touche} role="presentation">
  <button
    type="button"
    class="aide-b"
    aria-label="Explication : {a.titre}"
    aria-expanded={ouvert}
    aria-controls="{uid}-t"
    onclick={() => (ouvert = !ouvert)}
  >?</button>
  {#if ouvert}
    <span class="aide-t" class:droite id="{uid}-t" role="note"><b>{a.titre}.</b> {a.texte}</span>
  {/if}
</span>

<style>
  .aide {
    position: relative;
    display: inline-flex;
    vertical-align: middle;
    margin-left: var(--e1);
    font-weight: 400;
  }
  .aide-b {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--line2);
    border-radius: 50%;
    background: var(--white);
    color: var(--text2);
    font: 500 13px/1 var(--sans);
    cursor: pointer;
    position: relative;
  }
  /* zone de toucher de 44 px autour du petit bouton */
  .aide-b::after {
    content: '';
    position: absolute;
    inset: -11px;
  }
  .aide-b:hover,
  .aide-b[aria-expanded='true'] {
    background: var(--sunk);
    color: var(--text);
  }
  .aide-b:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .aide-t {
    position: absolute;
    z-index: 20;
    top: calc(100% + var(--e1));
    left: 0;
    width: max-content;
    max-width: min(280px, calc(100vw - 32px));
    padding: var(--e3);
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--text);
    box-shadow: var(--ombre-carte);
    font-size: 14px;
    line-height: 1.45;
    font-weight: 400;
    text-align: left;
    white-space: normal;
  }
  .aide-t.droite {
    left: auto;
    right: 0;
  }
  .aide-t b {
    font-weight: 500;
  }
</style>
