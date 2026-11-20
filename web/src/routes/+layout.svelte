<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Moi } from '#lib/types.ts';

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

  async function deconnecter() {
    await api.post('/api/deconnexion');
    session.moi = null;
    session.charge = false;
    await goto('/connexion');
  }

  const liens = [
    { href: '/', texte: 'Tableau de bord' },
    { href: '/mails', texte: 'Mails' },
    { href: '/documents', texte: 'Documents' },
    { href: '/depots', texte: 'Dépôts' },
    { href: '/rapprochement', texte: 'Rapprochement' },
    { href: '/fournisseurs', texte: 'Fournisseurs' }
  ];
  const liensAdmin = [
    { href: '/admin/structures', texte: 'Structures' },
    { href: '/admin/boites', texte: 'Boîtes surveillées' },
    { href: '/admin/utilisateurs', texte: 'Utilisateurs' },
    { href: '/admin/expedition', texte: 'Expédition' },
    { href: '/admin/claude', texte: 'Claude' },
    { href: '/admin/journal', texte: 'Journal' }
  ];
  function actif(href: string) {
    return href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
  }
</script>

{#if publique}
  <main class="seul">{@render children()}</main>
{:else if session.moi}
  <div class="cadre">
    <nav>
      <div class="marque">GMFlow</div>
      {#each liens as l (l.href)}
        <a href={l.href} class:actif={actif(l.href)}>{l.texte}</a>
      {/each}
      {#if session.moi.role === 'admin'}
        <div class="section">Administration</div>
        {#each liensAdmin as l (l.href)}
          <a href={l.href} class:actif={actif(l.href)}>{l.texte}</a>
        {/each}
      {/if}
      <div class="pied">
        <div class="discret">{session.moi.identifiant}</div>
        <button class="second" onclick={deconnecter}>Se déconnecter</button>
      </div>
    </nav>
    <main>{@render children()}</main>
  </div>
{/if}

<style>
  .cadre { display: grid; grid-template-columns: 220px 1fr; min-height: 100vh; }
  nav { background: var(--surface); border-right: 1px solid var(--trait); padding: 1rem 0.8rem; display: flex; flex-direction: column; gap: 0.15rem; position: sticky; top: 0; height: 100vh; }
  .marque { font-weight: 700; font-size: 1.1rem; color: var(--accent); padding: 0 0.5rem 0.9rem; letter-spacing: 0.01em; }
  nav a { color: var(--encre); text-decoration: none; padding: 0.38rem 0.5rem; border-radius: var(--rayon); }
  nav a:hover { background: var(--fond); }
  nav a.actif { background: var(--accent-clair); color: var(--accent); font-weight: 600; }
  .section { font-size: 0.78rem; color: var(--gris); padding: 1rem 0.5rem 0.3rem; }
  .pied { margin-top: auto; padding: 0.5rem; display: grid; gap: 0.5rem; }
  main { padding: 1.5rem 2rem; max-width: 1200px; }
  main.seul { margin: 8vh auto; max-width: 400px; padding: 0 1rem; }
  @media (max-width: 760px) {
    .cadre { grid-template-columns: 1fr; }
    nav { position: static; height: auto; flex-direction: row; flex-wrap: wrap; }
    .pied { margin-top: 0; }
    main { padding: 1rem; }
  }
</style>
