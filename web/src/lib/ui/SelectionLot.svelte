<script lang="ts">
  // Validation par lot (audit U2) : « Tout sélectionner » en tête de liste, barre collée en bas
  // « Valider N pièces » dès qu'une pièce est cochée, et le résultat (« 14 validées, 3 laissées
  // à contrôler (alertes) ») annoncé aux lecteurs d'écran. Les cases des lignes sont dans
  // `ListeDocuments.svelte` ; l'état est un `Selection` (`#lib/selection.svelte.ts`).
  import { annulations } from '#lib/annulations.svelte.ts';
  import { message } from '#lib/api.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { nomOutil } from '#lib/libelles.ts';
  import { MAX_LOT, resumeLot, type Selection } from '#lib/selection.svelte.ts';
  import Bandeau from './Bandeau.svelte';

  let {
    sel,
    ids,
    partie,
    onvalide
  }: {
    sel: Selection;
    /** Lignes chargées, dans l'ordre de la liste. */
    ids: number[];
    /** `tete` : case « Tout sélectionner » ; `barre` : barre d'action et résultat. */
    partie: 'tete' | 'barre';
    /** Après la validation : relire la liste. */
    onvalide?: () => void;
  } = $props();

  let cochables = $derived(sel.cochables(ids));
  let n = $derived(sel.coches.size);
  let tout = $derived(cochables.length > 0 && cochables.slice(0, MAX_LOT).every((id) => sel.coches.has(id)));
  let enCours = $state(false);
  let resultat = $state('');
  let ton = $state<'ok' | 'attention' | 'erreur'>('ok');

  // Lot annulé depuis le message du bas : son résumé n'est plus vrai.
  $effect(() => annulations.surAnnulation(() => (resultat = '')));

  function basculerTout() {
    if (tout) sel.rien();
    else sel.toutCocher(ids);
  }

  async function valider() {
    if (!n || enCours) return;
    enCours = true;
    resultat = '';
    try {
      const r = await sel.valider(ids);
      resultat = resumeLot(r);
      ton = r.laissees ? 'attention' : 'ok';
      if (r.annulation) {
        const quoi = `${r.validees} pièce${r.validees > 1 ? 's' : ''}`;
        annulations.annoncer(r.annulation, quoi, `${quoi} validée${r.validees > 1 ? 's' : ''}`, nomOutil(r.outil));
      }
      void rafraichirCompteurs();
      onvalide?.();
    } catch (e) {
      resultat = `Validation impossible : ${message(e)}`;
      ton = 'erreur';
    } finally {
      enCours = false;
    }
  }
</script>

{#if partie === 'tete'}
  {#if ids.length}
    <div class="tete-lot">
      <label class="case">
        <input type="checkbox" checked={tout} indeterminate={!tout && n > 0} disabled={!cochables.length} onchange={basculerTout} />
        Tout sélectionner
      </label>
      <span class="aide-lot">
        {cochables.length
          ? `${cochables.length} sans alerte sur ${ids.length}${cochables.length > MAX_LOT ? `, ${MAX_LOT} au plus à la fois` : ''}`
          : 'Aucune sans alerte : à ouvrir une par une'}
      </span>
    </div>
  {/if}
{:else}
  <div class="sr-seul" role="status" aria-live="polite">{resultat}</div>
  {#if n > 0 || resultat}
    <div class="pied-lot">
      {#if resultat}
        <div class="resultat-lot" aria-hidden="true">
          <Bandeau {ton}>
            {resultat}
            {#snippet action()}<button type="button" class="btn sm fantome" onclick={() => (resultat = '')}>Fermer</button>{/snippet}
          </Bandeau>
        </div>
      {/if}
      {#if n > 0}
        <div class="barre-lot">
          <div class="haut-barre">
            <span class="compte">{n} cochée{n > 1 ? 's' : ''}</span>
            <button type="button" class="btn-lien" onclick={() => sel.rien()} disabled={enCours}>Tout décocher</button>
          </div>
          <button type="button" class="btn principal" class:en-cours={enCours} disabled={enCours} onclick={valider}>
            Valider {n} pièce{n > 1 ? 's' : ''}
          </button>
        </div>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .tete-lot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--e2) var(--e4);
    padding: var(--e2) 16px;
    margin-bottom: var(--e2);
    border-bottom: 1px solid var(--line-faint);
  }
  .tete-lot .case {
    align-items: center;
    font-weight: 500;
    min-height: 32px;
  }
  .tete-lot .case input {
    margin-top: 0;
  }
  .aide-lot {
    color: var(--muted);
    font-size: 14px;
  }
  .pied-lot {
    position: sticky;
    bottom: 0;
    z-index: 5;
    margin-top: var(--e3);
    display: grid;
    gap: var(--e2);
  }
  .barre-lot {
    display: grid;
    gap: var(--e2);
    padding: 12px 16px;
    border-radius: var(--r2);
    background: var(--pane);
    box-shadow: var(--ombre-page);
  }
  .resultat-lot :global(.bandeau) {
    margin-bottom: 0;
    box-shadow: var(--ombre-page);
  }
  .haut-barre {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--e3);
  }
  .barre-lot .btn.principal {
    width: 100%;
    justify-content: center;
  }
  .compte {
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 760px) {
    .pied-lot {
      bottom: calc(68px + env(safe-area-inset-bottom));
    }
    .tete-lot .case {
      min-height: 44px;
    }
    .barre-lot .btn {
      min-height: 44px;
    }
  }
</style>
