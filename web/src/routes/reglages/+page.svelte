<script lang="ts">
  // Réglages (admins) : mise en route en tête, puis une section par sujet, Avancé replié.
  // Ancres : #societes, #boites, #claude, #envoi, #serveur-api, #utilisateurs, #traitements, #avance.
  import { goto } from '$app/navigation';
  import { tick } from 'svelte';
  import { api, message, miseEnRoute } from '#lib/api.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Boite, CleEtape, MiseEnRoute as Etat, Societe, Utilisateur } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Icone, { type NomIcone } from '#lib/ui/Icone.svelte';
  import Avance from './Avance.svelte';
  import Boites from './Boites.svelte';
  import Claude from './Claude.svelte';
  import Envoi from './Envoi.svelte';
  import ServeurApi from './ServeurApi.svelte';
  import MiseEnRoute from './MiseEnRoute.svelte';
  import Societes from './Societes.svelte';
  import Traitements from './Traitements.svelte';
  import Utilisateurs from './Utilisateurs.svelte';

  let admin = $derived(session.moi?.role === 'admin');
  $effect(() => {
    if (session.moi && !admin) void goto('/', { replaceState: true });
  });

  let societes = $state<Societe[]>([]);
  let boites = $state<Boite[]>([]);
  let utilisateurs = $state<Utilisateur[]>([]);
  let etat = $state<Etat | null>(null);
  let charge = $state(false);
  let erreur = $state('');

  async function chargerListes() {
    [societes, boites, utilisateurs] = await Promise.all([
      api.get<Societe[]>('/api/admin/societes'),
      api.get<Boite[]>('/api/admin/boites'),
      api.get<Utilisateur[]>('/api/admin/utilisateurs')
    ]);
  }
  async function chargerEtat() {
    try {
      etat = await miseEnRoute();
    } catch {
      /* la mise en route est un plus : les sections restent utilisables */
    }
  }
  async function charger() {
    erreur = '';
    try {
      await Promise.all([chargerListes(), chargerEtat()]);
      charge = true;
    } catch (e) {
      erreur = message(e);
    }
  }
  /** Après une modification dans une section : listes, mise en route et sélecteur de société. */
  async function rafraichir(avecSocietes = false) {
    await Promise.all([chargerListes().catch((e) => (erreur = message(e))), chargerEtat()]);
    if (avecSocietes) void societe.charger();
    // Claude connecté, boîte réparée… : la pastille d'état de la barre du haut suit.
    void rafraichirCompteurs();
  }

  $effect(() => {
    if (!admin) return;
    void charger().then(async () => {
      // Arrivée par une ancre (/reglages#boites, redirection d'une ancienne page) :
      // les sections n'existent qu'une fois les données là.
      await tick();
      const id = location.hash.slice(1);
      if (id === 'mise-en-route') merOuvert = true;
      await tick();
      if (id) document.getElementById(id)?.scrollIntoView({ block: 'start' });
    });
  });

  // ---------- Navigation à gauche : section visible ----------
  let zone = $state<HTMLElement>();
  let visible = $state('mise-en-route');
  const SECTIONS = ['mise-en-route', 'societes', 'boites', 'claude', 'envoi', 'serveur-api', 'utilisateurs', 'traitements', 'avance'];
  function suivre() {
    if (!zone) return;
    // Section courante : la dernière dont le titre a passé le premier tiers de la zone visible.
    // Ordinateur : la zone défile elle-même ; téléphone : c'est la fenêtre qui défile.
    const propre = zone.scrollHeight > zone.clientHeight + 4;
    const debut = propre ? zone.getBoundingClientRect().top : (barre?.getBoundingClientRect().bottom ?? 0);
    const hauteur = propre ? zone.clientHeight : window.innerHeight - debut;
    const haut = debut + Math.max(96, hauteur / 3);
    let courante = SECTIONS[0];
    for (const id of SECTIONS) {
      const el = document.getElementById(id);
      if (el && el.getBoundingClientRect().top <= haut) courante = id;
    }
    // Tout en bas : la dernière section, même courte.
    const enBas = propre
      ? zone.scrollHeight - zone.scrollTop - zone.clientHeight < 4
      : window.scrollY > 0 && window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
    if (enBas) courante = SECTIONS[SECTIONS.length - 1];
    visible = courante;
  }
  $effect(() => {
    if (!charge || !zone) return;
    const z = zone;
    suivre();
    z.addEventListener('scroll', suivre, { passive: true });
    window.addEventListener('scroll', suivre, { passive: true });
    return () => {
      z.removeEventListener('scroll', suivre);
      window.removeEventListener('scroll', suivre);
    };
  });

  // Téléphone : la barre des sujets défile à l'horizontale, l'entrée courante y reste visible.
  let barre = $state<HTMLElement>();
  $effect(() => {
    void visible;
    if (!barre || barre.scrollWidth <= barre.clientWidth) return;
    const lien = barre.querySelector<HTMLElement>('a.cur');
    if (lien) barre.scrollTo({ left: lien.offsetLeft - 8, behavior: 'smooth' });
  });

  function aller(id: string) {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  // ---------- Actions de la mise en route ----------
  let refSocietes = $state<ReturnType<typeof Societes>>();
  let refUtilisateurs = $state<ReturnType<typeof Utilisateurs>>();
  let refEnvoi = $state<ReturnType<typeof Envoi>>();
  let refClaude = $state<ReturnType<typeof Claude>>();

  async function agir(cle: CleEtape) {
    if (cle === 'boites') return goto('/reglages/boites/nouvelle');
    if (cle === 'societes') refSocietes?.nouvelle();
    if (cle === 'utilisateurs') refUtilisateurs?.nouveau();
    if (cle === 'envoi') refEnvoi?.ouvrir();
    if (cle === 'claude') refClaude?.connecter();
    await tick();
    aller(cle);
  }
  async function modifierSociete(id: number) {
    refSocietes?.modifier(id);
    await tick();
    aller('societes');
  }

  // Détail des étapes de la mise en route : replié sous le bandeau, ouvert par « Voir les étapes » ou le lien du menu.
  let merOuvert = $state(false);

  let faites = $derived(etat ? etat.etapes.filter((e) => e.fait).length : 0);
  let nav = $derived<{ id: string; titre: string; icone: NomIcone; compte?: number }[]>([
    { id: 'societes', titre: RUBRIQUES.societes, icone: 'batiment', compte: societes.filter((s) => !s.archivee).length },
    { id: 'boites', titre: RUBRIQUES.boites, icone: 'boite', compte: boites.length },
    { id: 'claude', titre: RUBRIQUES.claude, icone: 'prise' },
    { id: 'envoi', titre: RUBRIQUES.envoi, icone: 'envoyer' },
    { id: 'serveur-api', titre: RUBRIQUES.serveurApi, icone: 'lien' },
    { id: 'utilisateurs', titre: RUBRIQUES.utilisateurs, icone: 'utilisateurs', compte: utilisateurs.filter((u) => u.actif).length },
    { id: 'traitements', titre: RUBRIQUES.traitements, icone: 'rafraichir' },
    { id: 'avance', titre: RUBRIQUES.avance, icone: 'curseurs' }
  ]);
</script>

<svelte:head><title>Réglages · GMFlow</title></svelte:head>

{#if admin}
  <div class="pleine fixe reglages">
    <div class="set">
      <nav class="set-nav" aria-label="Réglages" bind:this={barre}>
        <h1>Réglages</h1>
        <a href="#mise-en-route" onclick={() => (merOuvert = true)} class:cur={visible === 'mise-en-route'} aria-current={visible === 'mise-en-route' ? 'true' : undefined}>
          <Icone nom="drapeau" /><span class="lib">Mise en route</span>
          {#if etat}<span class="st num">{faites}/{etat.etapes.length}</span>{/if}
        </a>
        <div class="grp">Par sujet</div>
        {#each nav as l (l.id)}
          <a href="#{l.id}" class:cur={visible === l.id} aria-current={visible === l.id ? 'true' : undefined}>
            <Icone nom={l.icone} /><span class="lib">{l.titre}</span>
            {#if charge && l.compte}<span class="st num">{l.compte}</span>{/if}
          </a>
        {/each}
      </nav>

      <div class="set-main" bind:this={zone}>
        <div class="set-col">
          {#if erreur}
            <Bandeau ton="erreur" titre="Réglages pas chargés.">
              {erreur}
              {#snippet action()}<button class="btn sm" onclick={charger}>Réessayer</button>{/snippet}
            </Bandeau>
          {/if}
          {#if !charge && !erreur}
            <Chargement texte="Chargement des réglages" />
          {:else if charge}
            <MiseEnRoute {etat} {boites} {societes} {agir} {aller} bind:ouvert={merOuvert} />
            <Societes bind:this={refSocietes} {societes} onchange={() => rafraichir(true)} />
            <Boites {boites} {societes} />
            <Claude bind:this={refClaude} onchange={() => rafraichir()} />
            <Envoi bind:this={refEnvoi} {societes} {modifierSociete} onchange={() => rafraichir()} />
            <ServeurApi />
            <Utilisateurs bind:this={refUtilisateurs} {utilisateurs} {societes} onchange={() => rafraichir()} />
            <Traitements />
            <Avance onchange={() => rafraichir()} />
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  /* Mise en page de docs/maquette-sauge/S6-Reglages.html : sujets à gauche, une page qui défile. */
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
    scroll-behavior: smooth;
  }
  .set-col {
    max-width: 800px;
    display: flex;
    flex-direction: column;
    gap: var(--e10);
  }

  /* ---------- Classes communes aux sections (composants de ce dossier) ---------- */
  /* Une section : titre 22 px, phrase d'aide, contenu ; un filet la sépare de la suivante. */
  .reglages :global(.sujet) {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    padding-bottom: var(--e10);
    border-bottom: 1px solid var(--line-faint);
    scroll-margin-top: var(--e6);
  }
  .reglages :global(.sujet:last-child) {
    border-bottom: 0;
    padding-bottom: 0;
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

  /* < 1024 px : les sujets passent en barre horizontale au-dessus du contenu. */
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
