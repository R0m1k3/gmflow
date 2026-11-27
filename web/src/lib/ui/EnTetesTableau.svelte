<script lang="ts" module>
  export interface ColonneTableau {
    libelle: string;
    /** Clé de tri (une des `colonnesTri`) : l'en-tête devient le lien qui trie. */
    tri?: string;
    /** Montants : aligné à droite. */
    droite?: boolean;
  }
</script>

<script lang="ts">
  // En-têtes collants d'un tableau pleine largeur (.tableau, app.css) : ce sont eux qui trient.
  // Clic = tri par cette colonne dans son sens habituel, second clic = sens inverse (#lib/tri.ts,
  // comme EnTetesTri). Colonne active en vert foncé avec ▾ / ▴ et son sens en toutes lettres.
  import { page } from '$app/state';
  import { lienTri, sensLisible, type ColonneTri, type TriChoisi } from '#lib/tri.ts';

  let {
    colonnes,
    colonnesTri,
    tri,
    retirer = [],
    caseLot = false
  }: {
    colonnes: ColonneTableau[];
    colonnesTri: ColonneTri[];
    tri: TriChoisi;
    /** Paramètres retirés de l'adresse au clic (la fiche ouverte). */
    retirer?: string[];
    /** Une première colonne vide au-dessus des cases à cocher. */
    caseLot?: boolean;
  } = $props();
</script>

<div class="t-tete" role="group" aria-label="Trier la liste">
  {#if caseLot}<span></span>{/if}
  {#each colonnes as c (c.libelle)}
    {@const col = c.tri ? colonnesTri.find((x) => x.cle === c.tri) : undefined}
    {#if col}
      {@const actif = tri.cle === col.cle}
      {@const sens = sensLisible(col.genre, actif ? tri.croissant : col.croissant)}
      <a
        class="t-th"
        class:droite={c.droite}
        class:actif
        href={lienTri(page.url, colonnesTri, col.cle, retirer)}
        data-sveltekit-replacestate
        data-sveltekit-noscroll
        aria-current={actif ? 'true' : undefined}
        title={actif ? `Trié ${sens} : cliquer pour inverser` : `Trier ${sens}`}
        >{c.libelle}{#if actif}<span aria-hidden="true">&nbsp;{tri.croissant ? '▴' : '▾'}</span><span class="sr-seul">, trié {sens}</span>{/if}</a
      >
    {:else}
      <span class="t-th" class:droite={c.droite}>{c.libelle}</span>
    {/if}
  {/each}
</div>
