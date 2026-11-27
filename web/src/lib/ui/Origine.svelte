<script lang="ts" module>
  import type { GenreOrigine } from '#lib/origine.ts';
  /** Signe de chaque origine : une forme différente, jamais la couleur seule. */
  export const SIGNES: Record<GenreOrigine, string> = { sur: '✓', doute: '?', corrige: '✎', calcule: '=', modifie: '•' };
</script>

<script lang="ts">
  // Pastille d'origine d'une valeur de la fiche (U9) : un signe discret (✓ ? ✎ = •) à côté du
  // libellé ; au survol son libellé, au clic le détail et l'historique des corrections du champ
  // (qui, quand, avant → après). Calcul de l'origine : #lib/origine.ts.
  import { quandCorrection, type Origine } from '#lib/origine.ts';

  let {
    origine,
    libelle,
    formater = (v: unknown) => (v === null || v === undefined || v === '' ? 'vide' : String(v))
  }: {
    origine: Origine | null;
    /** Nom du champ, pour les lecteurs d'écran : « N° ». */
    libelle: string;
    /** Valeur lisible d'une correction (montant, date, nom de la société…). */
    formater?: (v: unknown) => string;
  } = $props();

  const uid = $props.id();
  let ouvert = $state(false);
  let racine = $state<HTMLElement>();

  $effect(() => {
    if (!ouvert) return;
    const fermer = (e: Event) => {
      if (e instanceof KeyboardEvent) {
        if (e.key === 'Escape') {
          e.stopPropagation();
          ouvert = false;
        }
        return;
      }
      if (racine && !racine.contains(e.target as Node)) ouvert = false;
    };
    document.addEventListener('pointerdown', fermer, true);
    document.addEventListener('keydown', fermer, true);
    return () => {
      document.removeEventListener('pointerdown', fermer, true);
      document.removeEventListener('keydown', fermer, true);
    };
  });
</script>

{#if origine}
  <span class="origine" bind:this={racine}>
    <button
      type="button"
      class="pastille {origine.genre}"
      title={origine.libelle}
      aria-label="{libelle} : {origine.libelle}"
      aria-expanded={ouvert}
      aria-controls="{uid}-o"
      onclick={(e) => {
        e.stopPropagation();
        ouvert = !ouvert;
      }}
    >
      <span aria-hidden="true">{SIGNES[origine.genre]}</span>
    </button>
    {#if ouvert}
      <div class="bulle" id="{uid}-o" role="dialog" aria-label="Origine : {libelle}">
        <p class="o-l"><span class="signe {origine.genre}" aria-hidden="true">{SIGNES[origine.genre]}</span>{origine.libelle}</p>
        {#if origine.historique.length}
          <ol class="o-h">
            {#each origine.historique as c, i (i)}
              <li>
                <span class="o-q">{quandCorrection(c.quand)}, {c.par ?? 'compte supprimé'}</span>
                <span class="o-v"><span class="num">{formater(c.avant)}</span> → <span class="num">{formater(c.apres)}</span></span>
              </li>
            {/each}
          </ol>
        {:else if origine.genre !== 'modifie'}
          <p class="o-vide">Jamais corrigée.</p>
        {/if}
      </div>
    {/if}
  </span>
{/if}

<style>
  .origine {
    position: relative;
    display: inline-flex;
    vertical-align: middle;
  }
  .pastille,
  .signe {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: none;
    border-radius: 10px;
    border: 1px solid var(--line2);
    background: var(--white);
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
    padding: 0;
    cursor: pointer;
  }
  .pastille:hover,
  .pastille[aria-expanded='true'] {
    border-color: var(--text2);
    color: var(--text);
  }
  .pastille:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  /* Doute : carré ambre ; corrigée : pleine ; calculée : pointillée ; modifiée : pleine accent. */
  .doute {
    border-radius: 4px;
    border-color: var(--warn-line);
    background: var(--warn-bg);
    color: var(--warn-ink);
  }
  .corrige {
    border-color: var(--text2);
    background: var(--text2);
    color: var(--white);
  }
  .calcule {
    border-style: dashed;
    color: var(--text2);
  }
  .modifie {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--white);
  }
  .bulle {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    left: -8px;
    width: max-content;
    max-width: min(340px, 80vw);
    padding: var(--e3) var(--e4);
    background: var(--white);
    border-radius: var(--r2);
    box-shadow:
      0 4px 16px rgba(0, 0, 0, 0.12),
      0 0 0 1px var(--line);
    font-size: 15px;
    font-weight: 400;
    color: var(--text);
    text-align: left;
    white-space: normal;
  }
  .o-l {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin: 0;
  }
  .o-l .signe {
    cursor: default;
  }
  .o-h {
    list-style: none;
    margin: var(--e2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .o-h li {
    display: flex;
    flex-direction: column;
    padding-top: var(--e2);
    border-top: 1px solid var(--line-faint);
  }
  .o-q {
    color: var(--text2);
    font-size: 14px;
  }
  .o-vide {
    margin: var(--e2) 0 0;
    color: var(--text2);
    font-size: 14px;
  }
</style>
