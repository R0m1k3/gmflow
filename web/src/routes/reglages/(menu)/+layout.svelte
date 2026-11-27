<script lang="ts">
  // Réglages (U10) : un menu latéral groupé (Magasins et accès, Connexions, Maintenance, Mon
  // compte) et une vue par section à son adresse (`/reglages/<section>`, liste : #lib/reglages.ts).
  // Les sections sont réservées aux administrateurs : un autre compte ne voit que « Mon compte »
  // et un refus clair s'il arrive ici par un lien. Données partagées : donnees.svelte.ts.
  // Formulaire modifié et pas enregistré (U13) : indiqué en tête de la vue, et une confirmation
  // est demandée avant de changer de section, de page ou de fermer l'onglet.
  import { page } from '$app/state';
  import { tick, type Snippet } from 'svelte';
  import { garderAvantDeQuitter } from '#lib/modifications.svelte.ts';
  import { FormulairesTouches } from '#lib/quitter.ts';
  import { GROUPES, sectionDe, type CleSection } from '#lib/reglages.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Icone, { type NomIcone } from '#lib/ui/Icone.svelte';
  import Refus from '../Refus.svelte';
  import { DonneesReglages, fournirDonneesReglages } from './donnees.svelte.ts';

  let { children }: { children: Snippet } = $props();

  let admin = $derived(session.moi?.role === 'admin');
  const d = new DonneesReglages();
  fournirDonneesReglages(d);
  $effect(() => {
    if (admin && !d.charge) void d.charger();
  });

  const ENTREES: Record<CleSection, { titre: string; icone: NomIcone }> = {
    societes: { titre: RUBRIQUES.societes, icone: 'batiment' },
    utilisateurs: { titre: RUBRIQUES.utilisateurs, icone: 'utilisateurs' },
    boites: { titre: RUBRIQUES.boites, icone: 'boite' },
    claude: { titre: RUBRIQUES.claude, icone: 'prise' },
    envoi: { titre: RUBRIQUES.envoi, icone: 'envoyer' },
    'serveur-api': { titre: RUBRIQUES.serveurApi, icone: 'lien' },
    logiflow: { titre: RUBRIQUES.logiflow, icone: 'camion' },
    traitements: { titre: RUBRIQUES.traitements, icone: 'rafraichir' },
    avance: { titre: RUBRIQUES.avance, icone: 'curseurs' },
    journal: { titre: RUBRIQUES.journal, icone: 'fichier' },
    'zone-dangereuse': { titre: RUBRIQUES.zoneDangereuse, icone: 'alerte' }
  };
  let comptes = $derived<Partial<Record<CleSection, number>>>({
    societes: d.societes.filter((s) => !s.archivee).length,
    boites: d.boites.length,
    utilisateurs: d.utilisateurs.filter((u) => u.actif).length
  });

  let courante = $derived(sectionDe(page.url.pathname));
  let accueil = $derived(page.url.pathname.replace(/\/$/, '') === '/reglages');
  let faites = $derived(d.etat ? d.etat.etapes.filter((e) => e.fait).length : 0);
  let titre = $derived(courante ? ENTREES[courante].titre : 'Mise en route');

  // Téléphone : la barre des sections défile à l'horizontale, l'entrée courante y reste visible.
  let barre = $state<HTMLElement>();
  $effect(() => {
    void courante;
    if (!barre || barre.scrollWidth <= barre.clientWidth) return;
    const lien = barre.querySelector<HTMLElement>('a.cur');
    if (lien) barre.scrollTo({ left: lien.offsetLeft - 8, behavior: 'smooth' });
  });

  // ---------- Modifications non enregistrées (U13) ----------
  // Un formulaire est « touché » à la première saisie et oublié quand il est envoyé ou retiré
  // de la page (Annuler, enregistré) ; l'indicateur se met à jour après chaque saisie ou clic.
  const touches = new FormulairesTouches<HTMLFormElement>();
  let nonEnregistre = $state(false);
  async function relire() {
    await tick();
    nonEnregistre = touches.modifie;
  }
  function saisie(e: Event) {
    const f = (e.target as HTMLElement | null)?.closest?.('form');
    // Les dialogues de confirmation (mot à taper) ne sont pas des réglages.
    if (!f || f.closest('dialog')) return;
    touches.toucher(f);
    nonEnregistre = true;
  }
  function envoi(e: Event) {
    if (e.target instanceof HTMLFormElement) touches.oublier(e.target);
    void relire();
  }
  garderAvantDeQuitter(() => touches.modifie);
  // Autre section (vue démontée) : ses formulaires sont partis, plus rien à garder.
  $effect(() => {
    void page.url.pathname;
    touches.vider();
    nonEnregistre = false;
  });
</script>

<svelte:head><title>{titre} · Réglages · GMFlow</title></svelte:head>

{#if session.moi}
  <div class="pleine fixe reglages">
    <div class="set">
      <nav class="set-nav" aria-label="Réglages" bind:this={barre}>
        <h1>Réglages</h1>
        {#if admin}
          <a href="/reglages" class:cur={accueil} aria-current={accueil ? 'page' : undefined}>
            <Icone nom="drapeau" /><span class="lib">Mise en route</span>
            {#if d.etat}<span class="st num">{faites}/{d.etat.etapes.length}</span>{/if}
          </a>
          {#each GROUPES as g (g.titre)}
            <div class="grp">{g.titre}</div>
            {#each g.sections as s (s)}
              <a href="/reglages/{s}" class:cur={courante === s} aria-current={courante === s ? 'page' : undefined}>
                <Icone nom={ENTREES[s].icone} /><span class="lib">{ENTREES[s].titre}</span>
                {#if d.charge && comptes[s]}<span class="st num">{comptes[s]}</span>{/if}
              </a>
            {/each}
          {/each}
        {/if}
        <div class="grp">Vous</div>
        <a href="/compte"><Icone nom="utilisateur" /><span class="lib">Mon compte</span></a>
      </nav>

      <div
        class="set-main"
        oninputcapture={saisie}
        onchangecapture={saisie}
        onsubmitcapture={envoi}
        onclickcapture={() => void relire()}
      >
        <div class="set-col">
          {#if !admin}
            <Refus />
          {:else}
            {#if nonEnregistre}
              <p class="non-enregistre" role="status"><Icone nom="crayon" />Modifications non enregistrées</p>
            {/if}
            {#if d.erreur}
              <Bandeau ton="erreur" titre="Réglages pas chargés.">
                {d.erreur}
                {#snippet action()}<button class="btn sm" onclick={d.charger}>Réessayer</button>{/snippet}
              </Bandeau>
            {/if}
            {#if !d.charge && !d.erreur}
              <Chargement texte="Chargement des réglages" />
            {:else if d.charge}
              {@render children()}
            {/if}
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* Mise en page de docs/maquette-sauge/S6-Reglages.html : sections à gauche, la vue à droite. */
  .set {
    flex: 1;
    display: flex;
    min-height: 0;
    background: var(--white);
  }
  .set-nav {
    width: 280px;
    flex: none;
    border-right: 1px solid var(--line);
    background: var(--white);
    padding: var(--e6) var(--e3);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .set-nav h1 {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 500;
    margin: 0 0 var(--e4);
    padding: 0 var(--e3);
  }
  .set-nav a {
    display: flex;
    align-items: center;
    gap: var(--e3);
    height: 40px;
    flex: none;
    padding: 0 var(--e3);
    border-radius: var(--r2);
    color: var(--text2);
    font-size: 16px;
    text-decoration: none;
    white-space: nowrap;
    transition: background-color var(--vite) var(--ease);
  }
  .set-nav a :global(svg.i) {
    flex: none;
    color: var(--muted);
  }
  .set-nav a:hover {
    background: var(--pane);
    color: var(--text);
  }
  .set-nav a.cur {
    background: var(--sunk);
    color: var(--text);
    font-weight: 500;
  }
  .set-nav a.cur :global(svg.i) {
    color: var(--text);
  }
  .set-nav a:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .set-nav a .st {
    margin-left: auto;
    font-size: 15px;
    font-weight: 400;
    color: var(--muted);
  }
  .set-nav .grp {
    margin: var(--e4) 0 0;
    padding: 0 var(--e3) var(--e2);
    font-size: 14px;
    font-weight: 500;
    color: var(--muted);
  }
  .set-main {
    position: relative; /* garde les éléments .sr (position absolue) dans la zone qui défile */
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: var(--e8) var(--e10) 48px;
  }
  .set-col {
    max-width: 800px;
    display: flex;
    flex-direction: column;
    gap: var(--e10);
  }
  .non-enregistre {
    position: sticky;
    top: 0;
    z-index: 4;
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: var(--e2);
    margin: 0 0 calc(var(--e4) - var(--e10));
    padding: 4px var(--e3);
    border-radius: var(--r2);
    background: var(--warn-bg);
    box-shadow: 0 0 0 1px var(--warn-line);
    color: var(--warn-ink);
    font-size: 15px;
    font-weight: 500;
  }

  /* ---------- Classes communes aux sections (composants du dossier parent) ---------- */
  /* Une section par vue : titre 22 px, phrase d'aide, contenu (plus de filet entre sections). */
  .reglages :global(.sujet) {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    scroll-margin-top: var(--e6);
  }
  .reglages :global(.sujet > *) {
    margin-top: 0;
    margin-bottom: 0;
  }
  .reglages :global(.sujet-h) {
    display: flex;
    align-items: center;
    gap: var(--e3);
    flex-wrap: wrap;
    min-height: 32px;
  }
  .reglages :global(.sujet-h h2) {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 500;
    margin: 0;
  }
  .reglages :global(.sujet .lead) {
    color: var(--text2);
    font-size: 16px;
    line-height: 1.45;
    max-width: 560px;
    text-wrap: pretty;
  }
  .reglages :global(.sujet-h + .lead) {
    margin-top: calc(4px - var(--e4));
  }
  .reglages :global(.sous-titre) {
    font-size: 17px;
    font-weight: 500;
    line-height: 1.4;
  }
  .reglages :global(.sujet > .sous-titre ~ .sous-titre) {
    margin-top: var(--e2);
  }

  /* Encadré à lignes : rayon 12, bord #DDE2DA, filets #ECEFE9 entre les lignes. */
  .reglages :global(.carte-r) {
    display: flex;
    flex-direction: column;
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
    background: var(--white);
    overflow: hidden;
  }
  .reglages :global(.carte-r > * + *) {
    border-top: 1px solid var(--line-faint);
  }
  .reglages :global(.rang) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--e4);
    padding: var(--e4) var(--e5);
    flex-wrap: wrap;
    color: var(--text);
    text-decoration: none;
  }
  .reglages :global(a.rang),
  .reglages :global(button.rang) {
    transition: background-color var(--vite) var(--ease);
  }
  .reglages :global(a.rang:hover),
  .reglages :global(button.rang:hover) {
    background: var(--pane);
    color: var(--text);
  }
  .reglages :global(button.rang) {
    width: 100%;
    border: 0;
    background: none;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .reglages :global(.rang:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .reglages :global(.rang .g) {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1 1 260px;
  }
  .reglages :global(.rang .t) {
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .reglages :global(.rang .s) {
    font-size: 15px;
    line-height: 1.45;
    color: var(--text2);
    overflow-wrap: anywhere;
  }
  .reglages :global(.rang .m) {
    font-size: 14px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .reglages :global(.rang .d) {
    display: flex;
    align-items: center;
    gap: var(--e3);
    flex-wrap: wrap;
  }
  .reglages :global(.rang .chev) {
    color: var(--muted);
    flex: none;
  }
  .reglages :global(.rang .alerte-l) {
    font-size: 15px;
  }
  /* Action discrète en lien vert (Modifier, Se déconnecter, Supprimer). */
  .reglages :global(.lien-a) {
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    font-size: 15px;
    font-weight: 500;
    color: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    text-decoration: none;
  }
  .reglages :global(.lien-a:hover) {
    color: var(--accent-ink);
    text-decoration: underline;
  }
  .reglages :global(.lien-a:disabled) {
    color: var(--muted);
    cursor: default;
    text-decoration: none;
  }
  /* Repli dans un encadré : même marge que les lignes. */
  .reglages :global(.carte-r > .repli) {
    border-bottom: 0;
  }
  .reglages :global(.carte-r > .repli > summary) {
    padding: var(--e4) var(--e5);
    min-height: 0;
    font-size: 15px;
  }
  .reglages :global(.carte-r > .repli > summary:hover) {
    background: var(--pane);
  }
  .reglages :global(.carte-r > .repli > .dedans) {
    padding: var(--e1) var(--e5) var(--e5);
    overflow-x: auto;
  }
  .reglages :global(.ligne.fixe) {
    cursor: default;
  }
  .reglages :global(.ligne.fixe:hover) {
    background: none;
  }
  /* Formulaire ouvert en place (dans un encadré ou seul). */
  .reglages :global(.edition) {
    padding: var(--e5);
    background: var(--white);
  }
  .reglages :global(.sujet > .edition) {
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
  }
  .reglages :global(.edition h3) {
    font-size: 17px;
    font-weight: 500;
    margin: 0 0 var(--e4);
  }
  .reglages :global(.edition > .bandeau) {
    margin-bottom: var(--e4);
  }
  .reglages :global(.form-f) {
    display: flex;
    gap: var(--e3);
    align-items: center;
    margin-top: var(--e5);
    flex-wrap: wrap;
  }
  .reglages :global(.sujet > .form-f) {
    margin-top: 0;
  }

  /* < 1024 px : les sections passent en barre horizontale au-dessus de la vue. */
  @media (max-width: 1023px) {
    .set {
      flex-direction: column;
    }
    .set-nav {
      position: sticky;
      top: 0;
      z-index: 5;
      width: auto;
      flex-direction: row;
      gap: 4px;
      padding: var(--e2) var(--e4);
      border-right: 0;
      border-bottom: 1px solid var(--line);
      overflow-x: auto;
      scrollbar-width: none;
    }
    .set-nav h1,
    .set-nav .grp {
      display: none;
    }
    .set-nav a {
      gap: var(--e2);
      font-size: 15px;
    }
    .set-nav a .st {
      margin-left: 2px;
      font-size: 14px;
    }
    .set-main {
      padding: var(--e6) var(--e6) var(--e10);
    }
  }
  @media (max-width: 760px) {
    .set-nav {
      top: var(--haut-barre);
    }
    .set-nav a :global(svg.i) {
      display: none;
    }
    .set-main {
      overflow: visible;
      padding: var(--e5) var(--e4) var(--e10);
    }
    .reglages :global(.sujet) {
      scroll-margin-top: calc(var(--haut-barre) + 64px);
    }
    .reglages :global(.sujet-h h2) {
      font-size: 20px;
    }
    .reglages :global(.rang) {
      padding: var(--e4);
    }
    .reglages :global(.carte-r > .repli > summary) {
      padding: var(--e4);
    }
    .reglages :global(.carte-r > .repli > .dedans) {
      padding: var(--e1) var(--e4) var(--e4);
    }
    .reglages :global(.edition) {
      padding: var(--e4);
    }
  }
</style>
