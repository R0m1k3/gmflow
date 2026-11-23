<script lang="ts">
  // Réglages (admins) : mise en route en tête, puis une section par sujet, Avancé replié.
  // Ancres : #societes, #boites, #claude, #envoi, #utilisateurs, #traitements, #avance.
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
      if (id) document.getElementById(id)?.scrollIntoView({ block: 'start' });
    });
  });

  // ---------- Navigation à gauche : section visible ----------
  let zone = $state<HTMLElement>();
  let visible = $state('mise-en-route');
  const SECTIONS = ['mise-en-route', 'societes', 'boites', 'claude', 'envoi', 'utilisateurs', 'traitements', 'avance'];
  function suivre() {
    if (!zone) return;
    // Section courante : la dernière dont le titre a passé le premier tiers de la zone visible.
    const haut = Math.max(zone.getBoundingClientRect().top, 48) + Math.max(96, zone.clientHeight / 3);
    let courante = SECTIONS[0];
    for (const id of SECTIONS) {
      const el = document.getElementById(id);
      if (el && el.getBoundingClientRect().top <= haut) courante = id;
    }
    // Tout en bas : la dernière section, même courte.
    if (zone.scrollHeight - zone.scrollTop - zone.clientHeight < 4 && zone.scrollHeight > zone.clientHeight) {
      courante = SECTIONS[SECTIONS.length - 1];
    }
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

  let faites = $derived(etat ? etat.etapes.filter((e) => e.fait).length : 0);
  let nav = $derived<{ id: string; titre: string; icone: NomIcone; compte?: number }[]>([
    { id: 'societes', titre: RUBRIQUES.societes, icone: 'batiment', compte: societes.filter((s) => !s.archivee).length },
    { id: 'boites', titre: RUBRIQUES.boites, icone: 'boite', compte: boites.length },
    { id: 'claude', titre: RUBRIQUES.claude, icone: 'prise' },
    { id: 'envoi', titre: RUBRIQUES.envoi, icone: 'envoyer' },
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
        <a href="#mise-en-route" class:cur={visible === 'mise-en-route'} aria-current={visible === 'mise-en-route' ? 'true' : undefined}>
          <Icone nom="drapeau" />Mise en route
          {#if etat}<span class="st num">{faites}/{etat.etapes.length}</span>{/if}
        </a>
        <div class="grp">Par sujet</div>
        {#each nav as l (l.id)}
          <a href="#{l.id}" class:cur={visible === l.id} aria-current={visible === l.id ? 'true' : undefined}>
            <Icone nom={l.icone} />{l.titre}
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
            <MiseEnRoute {etat} {boites} {societes} {agir} {aller} />
            <Societes bind:this={refSocietes} {societes} onchange={() => rafraichir(true)} />
            <Boites {boites} {societes} />
            <Claude bind:this={refClaude} onchange={() => rafraichir()} />
            <Envoi bind:this={refEnvoi} {societes} {modifierSociete} onchange={() => rafraichir()} />
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
  /* Mise en page de l'écran 4 de docs/maquette-b.html (réglages). */
  .set {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .set-nav {
    width: 240px;
    flex: none;
    border-right: 1px solid var(--line);
    background: var(--pane);
    padding: var(--e4) var(--e2);
    overflow: auto;
  }
  .set-nav h1 {
    font-size: 16px;
    font-weight: 600;
    margin: 0 var(--e2) var(--e3);
  }
  .set-nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 var(--e2);
    border-radius: var(--r2);
    color: var(--text2);
    text-decoration: none;
    margin-bottom: 2px;
    white-space: nowrap;
  }
  .set-nav a:hover {
    background: var(--sunk);
    color: var(--text);
  }
  .set-nav a.cur {
    background: var(--white);
    color: var(--black);
    font-weight: 500;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .set-nav a .st {
    margin-left: auto;
    color: var(--muted);
  }
  .set-nav .grp {
    margin: var(--e5) var(--e2) 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .set-main {
    position: relative; /* garde les éléments .sr (position absolue) dans la zone qui défile */
    flex: 1;
    overflow: auto;
    padding: var(--e6) var(--e10) var(--e10);
    scroll-behavior: smooth;
  }
  .set-col {
    max-width: 760px;
  }

  /* Classes communes aux sections (composants de ce dossier). */
  .reglages :global(.sujet) {
    padding-top: var(--e10);
    scroll-margin-top: 0;
  }
  .reglages :global(.sujet-h) {
    display: flex;
    align-items: center;
    gap: var(--e2);
    min-height: 32px;
  }
  .reglages :global(.sujet-h h2),
  .reglages :global(.mer h2) {
    font-size: 20px;
    line-height: 28px;
    font-weight: 600;
    margin: 0;
  }
  .reglages :global(.sujet .lead),
  .reglages :global(.mer .lead) {
    color: var(--text2);
    margin: var(--e1) 0 var(--e5);
    max-width: 60ch;
  }
  .reglages :global(.sujet > .bandeau) {
    margin-bottom: var(--e4);
  }
  .reglages :global(.ligne.fixe) {
    cursor: default;
  }
  .reglages :global(.ligne.fixe:hover) {
    background: none;
  }
  .reglages :global(.edition) {
    padding: var(--e4) var(--e2) var(--e5);
    background: var(--white);
  }
  .reglages :global(.edition h3) {
    font-size: 14px;
    margin-bottom: var(--e3);
  }
  .reglages :global(.edition > .bandeau) {
    margin-bottom: var(--e3);
  }
  .reglages :global(.form-f) {
    display: flex;
    gap: var(--e2);
    align-items: center;
    margin-top: var(--e4);
    flex-wrap: wrap;
  }
  .reglages :global(.sous-titre) {
    font-size: 14px;
    font-weight: 500;
    margin: var(--e6) 0 var(--e2);
  }

  @media (max-width: 760px) {
    .set {
      flex-direction: column;
    }
    .set-nav {
      position: sticky;
      top: var(--haut-barre);
      z-index: 5;
      width: auto;
      display: flex;
      gap: 2px;
      padding: var(--e2);
      border-right: 0;
      border-bottom: 1px solid var(--line);
      overflow-x: auto;
    }
    .set-nav h1,
    .set-nav .grp,
    .set-nav a .st {
      display: none;
    }
    .set-nav a {
      flex: none;
      margin: 0;
    }
    .set-main {
      overflow: visible;
      padding: var(--e4) var(--e4) var(--e8);
    }
    .reglages :global(.sujet) {
      scroll-margin-top: calc(var(--haut-barre) + 52px);
    }
  }
</style>
