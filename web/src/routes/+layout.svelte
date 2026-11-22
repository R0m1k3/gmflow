<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { compteurs, rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import { libelleRole } from '#lib/libelles.ts';
  import type { Moi } from '#lib/types.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';

  let { children } = $props();

  const publiques = ['/connexion', '/premier-compte'];
  let publique = $derived(publiques.includes(page.url.pathname));
  let admin = $derived(session.moi?.role === 'admin');

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

  // Sociétés du sélecteur (admins seulement : un responsable n'en voit qu'une).
  $effect(() => {
    if (admin && !societe.charge) void societe.charger();
  });

  // Badge et pastille : relus à chaque page, à chaque changement de société, et chaque minute.
  $effect(() => {
    void page.url.pathname;
    if (!session.moi) return;
    void rafraichirCompteurs(admin ? societe.parametre : '', admin);
  });
  $effect(() => {
    if (!session.moi) return;
    const minuteur = setInterval(() => {
      if (document.visibilityState === 'visible') void rafraichirCompteurs();
    }, 60_000);
    return () => clearInterval(minuteur);
  });

  // Téléphone : libellés courts dans la barre compacte.
  let tel = $state(false);
  $effect(() => {
    const mq = window.matchMedia('(max-width: 760px)');
    tel = mq.matches;
    const suivre = (e: MediaQueryListEvent) => (tel = e.matches);
    mq.addEventListener('change', suivre);
    return () => mq.removeEventListener('change', suivre);
  });

  // ---------- Recherche ----------
  let recherche = $state('');
  let champRecherche = $state<HTMLInputElement>();
  $effect(() => {
    if (page.url.pathname === '/recherche') recherche = page.url.searchParams.get('q') ?? '';
  });
  function chercher(e: SubmitEvent) {
    e.preventDefault();
    const q = recherche.trim();
    champRecherche?.blur();
    void goto(q ? `/recherche?q=${encodeURIComponent(q)}` : '/recherche');
  }
  $effect(() => {
    if (!session.moi) return;
    return raccourcis({
      '/': () => {
        // Champ masqué sur téléphone : on va directement à la page de recherche.
        if (champRecherche && champRecherche.offsetParent !== null) champRecherche.focus();
        else void goto('/recherche');
      }
    });
  });

  // ---------- Menu utilisateur et menu téléphone ----------
  let menuOuvert = $state(false);
  let menuTel = $state(false);
  $effect(() => {
    void page.url.pathname;
    menuOuvert = false;
    menuTel = false;
  });
  $effect(() => {
    if (!menuOuvert && !menuTel) return;
    const fermer = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent) {
        if (e.key !== 'Escape') return;
      } else if ((e.target as HTMLElement).closest('[data-menu]')) return;
      menuOuvert = false;
      menuTel = false;
    };
    window.addEventListener('pointerdown', fermer);
    window.addEventListener('keydown', fermer);
    return () => {
      window.removeEventListener('pointerdown', fermer);
      window.removeEventListener('keydown', fermer);
    };
  });

  function initiales(identifiant: string): string {
    const nom = identifiant.split('@')[0];
    const morceaux = nom.split(/[.\-_\s]+/).filter(Boolean);
    const lettres = morceaux.length >= 2 ? morceaux[0][0] + morceaux[1][0] : nom.slice(0, 2);
    return lettres.toUpperCase();
  }

  async function deconnecter() {
    await api.post('/api/deconnexion');
    session.moi = null;
    session.charge = false;
    await goto('/connexion');
  }

  // ---------- Menu ----------
  const liens = [
    { href: '/', texte: 'À traiter', court: 'À traiter', icone: 'boite' },
    { href: '/fournisseurs', texte: 'Fournisseurs', court: 'Fournisseurs', icone: 'camion' },
    { href: '/bilan', texte: 'Bilan du mois', court: 'Bilan', icone: 'graphique' }
  ] as const;
  function actif(href: string) {
    const p = page.url.pathname;
    // La fiche d'un document ou d'un mail s'ouvre depuis À traiter : l'entrée reste allumée.
    if (href === '/') return p === '/' || p.startsWith('/documents/') || p.startsWith('/mails/');
    return p === href || p.startsWith(`${href}/`);
  }
  let reglagesActif = $derived(page.url.pathname.startsWith('/reglages') || page.url.pathname.startsWith('/admin'));
  let etat = $derived.by(() => {
    if (!admin || compteurs.problemes === null) return null;
    const n = compteurs.problemes;
    return n === 0 ? { ok: true, texte: 'Tout fonctionne' } : { ok: false, texte: `${n} problème${n > 1 ? 's' : ''}` };
  });
</script>

{#if publique}
  <main class="seul">{@render children()}</main>
{:else if session.moi}
  <header class="barre">
    <a class="logo" href="/" aria-label="GMFlow, À traiter"><span class="marque" aria-hidden="true"></span>GMFlow</a>

    <nav class="menu" aria-label="Menu principal">
      {#each liens as l (l.href)}
        <a href={l.href} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
          {l.texte}
          {#if l.href === '/' && compteurs.aTraiter > 0}<span class="compteur">{compteurs.aTraiter}</span>{/if}
        </a>
      {/each}
    </nav>

    <span class="sp"></span>

    <form class="recherche" role="search" onsubmit={chercher}>
      <Icone nom="chercher" />
      <input
        bind:this={champRecherche}
        bind:value={recherche}
        type="search"
        placeholder="Fournisseur, n°, montant"
        aria-label="Chercher un document ou un mail"
        onkeydown={(e) => {
          if (e.key === 'Escape') champRecherche?.blur();
        }}
      />
      <Kbd t="/" />
    </form>
    <a class="btn-icone tel" href="/recherche" aria-label="Chercher"><Icone nom="chercher" /></a>

    {#if admin && societe.liste.length > 0}
      <label class="sel">
        <span class="sr">Société</span>
        <select value={societe.id === null ? '' : String(societe.id)} onchange={(e) => societe.choisir(e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
          <option value="">{tel ? 'Toutes' : 'Toutes les sociétés'}</option>
          {#each societe.liste as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
        </select>
        <Icone nom="bas" />
      </label>
    {/if}

    {#if etat}
      <a class="etat" class:ok={etat.ok} href={etat.ok ? '/' : '/#bloque'} title={etat.ok ? 'Boîtes, Claude et envois fonctionnent' : 'Voir ce qui est bloqué'}>
        <span class="point"></span>{etat.texte}
      </a>
    {/if}

    {#if admin}
      <a class="btn-icone large" href="/reglages" aria-label="Réglages" title="Réglages" aria-current={reglagesActif ? 'page' : undefined}><Icone nom="reglages" /></a>
    {/if}

    <div class="moi large" data-menu>
      <button type="button" class="initiales" aria-haspopup="menu" aria-expanded={menuOuvert} aria-label="Compte {session.moi.identifiant}" onclick={() => (menuOuvert = !menuOuvert)}>
        {initiales(session.moi.identifiant)}
      </button>
      {#if menuOuvert}
        <div class="deroulant" role="menu">
          <div class="qui">
            <div class="ident">{session.moi.identifiant}</div>
            <div class="muted petit">{libelleRole(session.moi.role)}</div>
          </div>
          <button type="button" class="item" role="menuitem" onclick={deconnecter}><Icone nom="sortir" />Se déconnecter</button>
        </div>
      {/if}
    </div>

    <div class="moi tel" data-menu>
      <button type="button" class="btn-icone" aria-haspopup="menu" aria-expanded={menuTel} aria-label="Menu" onclick={() => (menuTel = !menuTel)}>
        <Icone nom={menuTel ? 'fermer' : 'menu'} />
      </button>
      {#if menuTel}
        <div class="deroulant" role="menu">
          <div class="qui">
            <div class="ident">{session.moi.identifiant}</div>
            <div class="muted petit">{libelleRole(session.moi.role)}</div>
          </div>
          {#if etat}
            <a class="item" class:rouge={!etat.ok} role="menuitem" href={etat.ok ? '/' : '/#bloque'}><span class="point"></span>{etat.texte}</a>
          {/if}
          {#if admin}<a class="item" role="menuitem" href="/reglages"><Icone nom="reglages" />Réglages</a>{/if}
          <button type="button" class="item" role="menuitem" onclick={deconnecter}><Icone nom="sortir" />Se déconnecter</button>
        </div>
      {/if}
    </div>
  </header>

  <main class="contenu">{@render children()}</main>

  <nav class="onglets" aria-label="Menu principal">
    {#each liens as l (l.href)}
      <a href={l.href} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
        <Icone nom={l.icone} />
        <span>{l.court}{#if l.href === '/' && compteurs.aTraiter > 0}&nbsp;<span class="compteur">{compteurs.aTraiter}</span>{/if}</span>
      </a>
    {/each}
  </nav>
{/if}

<style>
  .barre {
    position: sticky;
    top: 0;
    z-index: 20;
    height: var(--haut-barre);
    display: flex;
    align-items: center;
    gap: var(--e2);
    padding: 0 var(--e4);
    border-bottom: 1px solid var(--line);
    background: var(--white);
  }
  .logo {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin-right: var(--e5);
    font-weight: 600;
    font-size: 15px;
    color: var(--black);
    text-decoration: none;
    border-radius: var(--r1);
  }
  .menu {
    display: flex;
    gap: 2px;
  }
  .menu a {
    display: flex;
    align-items: center;
    gap: var(--e2);
    height: 32px;
    padding: 0 10px;
    border-radius: var(--r2);
    color: var(--text2);
    text-decoration: none;
    font-weight: 500;
    white-space: nowrap;
    transition:
      background-color var(--vite) var(--ease),
      color var(--vite) var(--ease);
  }
  .menu a:hover {
    background: var(--pane);
    color: var(--text);
  }
  .menu a.cur {
    background: var(--pane);
    color: var(--black);
  }

  .recherche {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--e2);
    width: 340px;
    height: 32px;
    padding: 0 var(--e2) 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--bg);
    color: var(--muted);
    transition:
      border-color var(--vite) var(--ease),
      box-shadow var(--vite) var(--ease),
      background-color var(--vite) var(--ease);
  }
  .recherche:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-weak);
    background: var(--white);
  }
  .recherche input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0;
    border: 0;
    background: none;
    box-shadow: none;
    color: var(--text);
  }
  .recherche input:focus {
    box-shadow: none;
  }
  .recherche input::-webkit-search-cancel-button {
    display: none;
  }
  .recherche:focus-within :global(kbd) {
    visibility: hidden;
  }

  .sel {
    position: relative;
    display: flex;
    align-items: center;
    height: 32px;
    color: var(--muted);
  }
  .sel select {
    height: 32px;
    width: auto;
    max-width: 220px;
    padding: 0 30px 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--text);
    cursor: pointer;
    text-overflow: ellipsis;
    field-sizing: content;
  }
  .sel select:hover {
    border-color: var(--line2);
  }
  .sel :global(svg) {
    position: absolute;
    right: 8px;
    pointer-events: none;
  }

  .etat {
    display: flex;
    align-items: center;
    gap: var(--e2);
    height: 32px;
    padding: 0 10px;
    border-radius: var(--r2);
    color: var(--err);
    font-weight: 500;
    text-decoration: none;
    white-space: nowrap;
    transition: background-color var(--vite) var(--ease);
  }
  .etat:hover {
    background: var(--err-bg);
  }
  .etat.ok {
    color: var(--text2);
    font-weight: 400;
  }
  .etat.ok .point {
    color: var(--ok);
  }
  .etat.ok:hover {
    background: var(--pane);
  }

  .moi {
    position: relative;
  }
  .initiales {
    width: 28px;
    height: 28px;
    margin-left: var(--e1);
    border-radius: 50%;
    border: 0;
    padding: 0;
    background: var(--sunk);
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 600;
    color: var(--text2);
    transition: background-color var(--vite) var(--ease);
  }
  .initiales:hover,
  .initiales[aria-expanded='true'] {
    background: var(--line);
    color: var(--text);
  }
  .deroulant {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    min-width: 240px;
    padding: var(--e1);
    border: 1px solid var(--line);
    border-radius: var(--r3);
    background: var(--white);
    box-shadow:
      0 2px 4px rgba(0, 0, 0, 0.04),
      0 8px 24px rgba(0, 0, 0, 0.08);
    animation: menu-entre var(--moyen) var(--ease);
  }
  @keyframes menu-entre {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .qui {
    padding: var(--e2) 10px 10px;
    border-bottom: 1px solid var(--line);
    margin-bottom: var(--e1);
  }
  .ident {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r2);
    background: none;
    color: var(--text);
    text-decoration: none;
    text-align: left;
  }
  .item:hover {
    background: var(--pane);
  }
  .item :global(svg) {
    color: var(--text2);
  }
  .item.rouge {
    color: var(--err);
    font-weight: 500;
  }
  .item .point {
    margin: 0 5px;
  }

  .contenu {
    min-height: calc(100dvh - var(--haut-barre));
  }
  main.seul {
    margin: 8vh auto;
    max-width: 400px;
    padding: 0 var(--e4);
  }

  .tel,
  .onglets {
    display: none;
  }

  @media (max-width: 1180px) {
    .recherche {
      width: 240px;
    }
  }
  @media (max-width: 960px) {
    .recherche {
      width: 180px;
    }
    .logo {
      margin-right: var(--e2);
    }
    .sel select {
      max-width: 160px;
    }
  }

  /* Téléphone : barre compacte, menu, onglets en bas */
  @media (max-width: 760px) {
    .barre {
      padding: 0 var(--e3) 0 var(--e4);
    }
    .logo {
      margin-right: auto;
    }
    .menu,
    .recherche,
    .etat,
    .large {
      display: none;
    }
    .sp {
      display: none;
    }
    .tel {
      display: inline-grid;
    }
    .moi.tel {
      display: block;
    }
    .sel select {
      max-width: 150px;
      font-size: 14px;
      height: 32px;
    }
    .item {
      height: 44px;
    }
    .onglets {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 15;
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      height: calc(56px + env(safe-area-inset-bottom));
      padding-bottom: env(safe-area-inset-bottom);
      border-top: 1px solid var(--line);
      background: var(--white);
    }
    .onglets a {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 2px;
      font-size: 11px;
      color: var(--muted);
      text-decoration: none;
    }
    .onglets a.cur {
      color: var(--black);
      font-weight: 500;
    }
    .onglets .compteur {
      font-size: 10px;
      line-height: 14px;
      padding: 0 4px;
    }
    .contenu {
      padding-bottom: calc(56px + env(safe-area-inset-bottom));
    }
    /* Une fiche ouverte a sa propre barre d'action en bas : pas d'onglets par-dessus. */
    :global(body:has(.barre-action)) .onglets,
    :global(body:has(.volets.ouvert)) .onglets {
      display: none;
    }
    :global(body:has(.barre-action)) .contenu,
    :global(body:has(.volets.ouvert)) .contenu {
      padding-bottom: 0;
    }
  }
</style>
