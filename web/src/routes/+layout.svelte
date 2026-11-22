<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Moi, Tableau } from '#lib/types.ts';

  let { children } = $props();

  const publiques = ['/connexion', '/premier-compte'];
  let publique = $derived(publiques.includes(page.url.pathname));

  $effect(() => {
    if (publique || session.charge) return;
    api
      .get<Moi>('/api/moi')
      .then((m) => {
        session.moi = m;
        session.charge = true;
      })
      .catch(() => {
        /* 401 : api.ts redirige vers /connexion */
      });
  });

  // Compteurs de la navigation : seulement ce qui attend une action, relus à chaque changement de page.
  let aFaire = $state({ repondre: 0, verifier: 0, bloques: 0 });
  $effect(() => {
    void page.url.pathname;
    if (!session.moi) return;
    api
      .get<Tableau>('/api/tableau-de-bord')
      .then((t) => (aFaire = { repondre: t.a_repondre, verifier: t.documents.a_verifier, bloques: t.depots.bloques }))
      .catch(() => {});
  });

  async function deconnecter() {
    await api.post('/api/deconnexion');
    session.moi = null;
    session.charge = false;
    await goto('/connexion');
  }

  type Lien = { href: string; texte: string; racine?: string; n?: () => number; alerte?: boolean };
  const groupes: { titre: string; liens: Lien[] }[] = [
    { titre: 'Traiter', liens: [
      { href: '/a-repondre', texte: 'À répondre', n: () => aFaire.repondre },
      { href: '/documents?verifie=non&etat=lu', racine: '/documents', texte: 'Documents à vérifier', n: () => aFaire.verifier }
    ] },
    { titre: 'Suivre', liens: [
      { href: '/mails', texte: 'Mails reçus' },
      { href: '/depots', texte: 'Dépôts Dext / Yooz', n: () => aFaire.bloques, alerte: true },
      { href: '/rapprochement', texte: 'Factures et bons de livraison' }
    ] },
    { titre: 'Référentiel', liens: [{ href: '/fournisseurs', texte: 'Fournisseurs' }] }
  ];
  const liensAdmin: Lien[] = [
    { href: '/admin/structures', texte: 'Sociétés' },
    { href: '/admin/boites', texte: 'Boîtes mail' },
    { href: '/admin/utilisateurs', texte: 'Utilisateurs' },
    { href: '/admin/expedition', texte: 'Envoi des dépôts' },
    { href: '/admin/claude', texte: 'Lecture par Claude' },
    { href: '/admin/journal', texte: 'Historique' }
  ];
  function actif(l: Lien) {
    const base = l.racine ?? l.href;
    return base === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(base);
  }
  let dansAdmin = $derived(page.url.pathname.startsWith('/admin'));
  let titrePage = $derived(
    [{ href: '/', texte: 'Aujourd’hui' }, ...groupes.flatMap((g) => g.liens), ...liensAdmin].find((l) => actif(l))?.texte ?? 'GMFlow'
  );

  // Tiroir du téléphone
  let ouvert = $state(false);
  $effect(() => {
    void page.url.pathname;
    ouvert = false;
  });
  function touche(e: KeyboardEvent) {
    if (e.key === 'Escape') ouvert = false;
  }
</script>

<svelte:window onkeydown={touche} />

{#if publique}
  <main class="seul">{@render children()}</main>
{:else if session.moi}
  <div class="cadre">
    <header class="barre">
      <button class="second menu" aria-expanded={ouvert} aria-controls="navigation" onclick={() => (ouvert = !ouvert)}>
        <span class="trois" aria-hidden="true"></span>
        Menu
      </button>
      <span class="titre-barre">{titrePage}</span>
    </header>
    {#if ouvert}<button class="voile" aria-label="Fermer le menu" tabindex="-1" onclick={() => (ouvert = false)}></button>{/if}
    <nav id="navigation" aria-label="Navigation principale" class:ouvert>
      <a class="marque" href="/">GMFlow</a>
      <a href="/" class="lien" class:actif={page.url.pathname === '/'} aria-current={page.url.pathname === '/' ? 'page' : undefined}>Aujourd’hui</a>
      {#each groupes as g (g.titre)}
        <div class="groupe">{g.titre}</div>
        {#each g.liens as l (l.href)}
          {@const n = l.n?.() ?? 0}
          <a href={l.href} class="lien" class:actif={actif(l)} aria-current={actif(l) ? 'page' : undefined}>
            <span>{l.texte}</span>
            {#if n > 0}<span class="compte" class:alerte={l.alerte}>{n}<span class="sr"> à traiter</span></span>{/if}
          </a>
        {/each}
      {/each}
      {#if session.moi.role === 'admin'}
        <details class="admin" open={dansAdmin}>
          <summary>Administration</summary>
          {#each liensAdmin as l (l.href)}
            <a href={l.href} class="lien" class:actif={actif(l)} aria-current={actif(l) ? 'page' : undefined}>{l.texte}</a>
          {/each}
        </details>
      {/if}
      <div class="pied">
        <span class="moi">{session.moi.identifiant}</span>
        <button class="discret petit" onclick={deconnecter}>Se déconnecter</button>
      </div>
    </nav>
    <main>{@render children()}</main>
  </div>
{/if}

<style>
  .cadre { display: grid; grid-template-columns: 248px minmax(0, 1fr); min-height: 100vh; }
  nav { background: var(--surface); border-right: 1px solid var(--trait); padding: var(--e5) var(--e3) var(--e4); display: flex; flex-direction: column; gap: 2px; position: sticky; top: 0; height: 100vh; overflow-y: auto; }
  .marque { font-family: var(--serif); font-weight: 600; font-size: 1.375rem; color: var(--encre); text-decoration: none; padding: 0 var(--e3); margin-bottom: var(--e4); line-height: 1.2; }
  .marque:hover { color: var(--encre); text-decoration: none; }
  .groupe { font-size: var(--t-petit); color: var(--gris); padding: var(--e6) var(--e3) var(--e2); }
  .lien { display: flex; align-items: center; justify-content: space-between; gap: var(--e2); min-height: 36px; padding: 0 var(--e3); color: var(--encre); text-decoration: none; border-radius: var(--rayon); border-left: 2px solid transparent; transition: background-color var(--vite) var(--courbe); }
  .lien:hover { background: var(--creux); color: var(--encre); text-decoration: none; }
  .lien.actif { background: var(--accent-clair); border-left-color: var(--accent); font-weight: 500; color: var(--accent-fonce); }
  .compte { font-variant-numeric: tabular-nums; font-size: var(--t-petit); font-weight: 600; color: var(--attention); background: var(--attention-clair); border: 1px solid var(--attention-trait); border-radius: 3px; min-width: 24px; text-align: center; padding: 0 6px; line-height: 20px; }
  .compte.alerte { color: var(--alerte); background: var(--alerte-clair); border-color: var(--alerte-trait); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .admin { margin-top: var(--e6); border-top: 1px solid var(--trait); padding-top: var(--e2); }
  .admin summary { cursor: pointer; font-size: var(--t-petit); color: var(--gris); min-height: 36px; padding: 0 var(--e3); display: flex; align-items: center; list-style: none; border-radius: var(--rayon); }
  .admin summary::-webkit-details-marker { display: none; }
  .admin summary::after { content: '+'; margin-left: auto; font-size: var(--t-base); }
  .admin[open] summary::after { content: '\2212'; }
  .admin summary:hover { background: var(--creux); }
  .pied { margin-top: auto; padding: var(--e4) var(--e3) 0; display: flex; flex-direction: column; align-items: flex-start; gap: var(--e1); border-top: 1px solid var(--trait); }
  .moi { font-size: var(--t-petit); color: var(--gris); overflow-wrap: anywhere; }
  .pied button { padding-left: 0; padding-right: 0; color: var(--gris); text-decoration: underline; text-underline-offset: 2px; }
  .pied button:hover:not(:disabled) { background: transparent; color: var(--encre); }
  main { padding: var(--e8) var(--e10) var(--e12); max-width: 1280px; width: 100%; }
  main.seul { margin: 0 auto; max-width: 440px; padding: 12vh var(--e4) var(--e8); }
  .barre, .voile { display: none; }

  @media (max-width: 860px) {
    .cadre { grid-template-columns: 1fr; }
    .barre { display: flex; align-items: center; gap: var(--e3); position: sticky; top: 0; z-index: 20; background: var(--surface); border-bottom: 1px solid var(--trait); padding: var(--e2) var(--e4); min-height: 56px; }
    .titre-barre { font-family: var(--serif); font-weight: 600; font-size: var(--t-moyen); }
    .menu { gap: var(--e2); }
    .trois { width: 16px; height: 2px; background: currentColor; box-shadow: 0 -5px currentColor, 0 5px currentColor; }
    nav { position: fixed; z-index: 40; top: 0; left: 0; bottom: 0; width: min(86vw, 320px); height: auto; visibility: hidden; transform: translateX(-12px); opacity: 0; transition: transform var(--vite) var(--courbe), opacity var(--vite) var(--courbe), visibility 0s var(--vite); }
    nav.ouvert { visibility: visible; transform: none; opacity: 1; transition-delay: 0s; }
    .lien { min-height: 44px; }
    .admin summary { min-height: 44px; }
    .voile { display: block; position: fixed; inset: 0; z-index: 30; background: rgba(28, 31, 36, 0.4); border: 0; border-radius: 0; min-height: 0; padding: 0; }
    .voile:hover:not(:disabled) { background: rgba(28, 31, 36, 0.4); }
    main { padding: var(--e5) var(--e4) var(--e10); }
  }
</style>
