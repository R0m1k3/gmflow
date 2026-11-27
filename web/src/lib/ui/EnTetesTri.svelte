<script lang="ts">
  // En-têtes de tri d'une liste : un lien par colonne triable (#lib/tri.ts). Clic = tri par cette
  // colonne dans son sens habituel, second clic = sens inverse ; le tri vit dans l'adresse et le
  // serveur trie (la pagination suit le même ordre). La colonne active porte une flèche et son
  // sens en toutes lettres (infobulle et lecteurs d'écran), pas seulement une couleur.
  import { lienTri, sensLisible, type ColonneTri, type TriChoisi } from '#lib/tri.ts';
  import { page } from '$app/state';
  import Icone from './Icone.svelte';

  let {
    colonnes,
    tri,
    retirer = []
  }: {
    colonnes: ColonneTri[];
    tri: TriChoisi;
    /** Paramètres retirés de l'adresse au clic (la fiche ouverte : `doc`, `mail`, `fiche`). */
    retirer?: string[];
  } = $props();
</script>

<div class="tri" role="group" aria-label="Trier la liste">
  <span class="tri-l" aria-hidden="true">Trier par</span>
  {#each colonnes as c (c.cle)}
    {@const actif = tri.cle === c.cle}
    {@const sens = sensLisible(c.genre, actif ? tri.croissant : c.croissant)}
    <a
      class="tri-c"
      class:actif
      href={lienTri(page.url, colonnes, c.cle, retirer)}
      data-sveltekit-replacestate
      data-sveltekit-noscroll
      aria-current={actif ? 'true' : undefined}
      title={actif ? `Trié ${sens} : cliquer pour inverser` : `Trier ${sens}`}
    >
      {c.libelle}{#if actif}<Icone nom={tri.croissant ? 'haut' : 'bas'} /><span class="sr-seul">, trié {sens}</span>{/if}
    </a>
  {/each}
</div>

<style>
  .tri {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 4px;
    padding: 0 var(--e4) var(--e2);
    font-size: 14px;
  }
  .tri-l {
    color: var(--muted);
    margin-right: 4px;
  }
  .tri-c {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    min-height: 32px;
    padding: 0 8px;
    border-radius: var(--r2);
    color: var(--text2);
    text-decoration: none;
  }
  .tri-c:hover {
    background: var(--sunk);
    color: var(--text);
  }
  .tri-c.actif {
    color: var(--text);
    font-weight: 600;
    background: var(--sunk);
  }
  .tri-c :global(svg.i) {
    width: 14px;
    height: 14px;
  }
  .tri-c:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
