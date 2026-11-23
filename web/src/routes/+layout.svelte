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
  import Icone, { type NomIcone } from '#lib/ui/Icone.svelte';
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

  // Écran étroit (< 1024 px) : barre compacte en haut et menu en tiroir ; téléphone (≤ 760 px) : onglets en bas.
  let etroit = $state(false);
  $effect(() => {
    const mq = window.matchMedia('(max-width: 1023px)');
    etroit = mq.matches;
    const suivre = (e: MediaQueryListEvent) => (etroit = e.matches);
    mq.addEventListener('change', suivre);
    return () => mq.removeEventListener('change', suivre);
  });

  // ---------- Recherche ----------
  // Sur la page Recherche, son propre champ remplace celui de la barre (une seule barre).
  let recherche = $state('');
  let champRecherche = $state<HTMLInputElement>();
  let surRecherche = $derived(page.url.pathname === '/recherche');
  $effect(() => {
    if (surRecherche) recherche = '';
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
        // Champ de la barre latérale masqué (écran étroit, page Recherche) : celui de la page Recherche.
        if (surRecherche) document.getElementById('champ-recherche')?.focus();
        else if (!etroit && champRecherche) champRecherche.focus();
        else void goto('/recherche');
      }
    });
  });

  // ---------- Menu du compte et tiroir (écran étroit) ----------
  let menuOuvert = $state(false);
  let tiroir = $state(false);
  $effect(() => {
    void page.url.pathname;
    menuOuvert = false;
    tiroir = false;
  });
  $effect(() => {
    if (!etroit) tiroir = false;
  });
  $effect(() => {
    if (!menuOuvert && !tiroir) return;
    const fermer = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent) {
        if (e.key !== 'Escape') return;
        menuOuvert = false;
        tiroir = false;
        return;
      }
      if (!(e.target as HTMLElement).closest('[data-menu]')) menuOuvert = false;
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
  // Rangé par groupes : le courrier, les pièces, le suivi. `tel` : présent dans les onglets du bas sur téléphone.
  type Lien = { href: string; texte: string; court: string; icone: NomIcone; tel: boolean; compteur?: () => number };
  type Groupe = { titre: string; liens: Lien[] };
  const groupes: Groupe[] = [
    { titre: '', liens: [{ href: '/', texte: 'Accueil', court: 'Accueil', icone: 'boite', tel: true }] },
    {
      titre: 'Courrier',
      liens: [
        { href: '/mails', texte: 'Mails', court: 'Mails', icone: 'mail', tel: true },
        { href: '/a-repondre', texte: 'À répondre', court: 'Répondre', icone: 'envoyer', tel: true, compteur: () => compteurs.aRepondre }
      ]
    },
    {
      titre: 'Pièces',
      liens: [
        { href: '/factures', texte: 'Factures', court: 'Factures', icone: 'fichier', tel: true },
        { href: '/bl', texte: 'Bons de livraison', court: 'BL', icone: 'camion', tel: true },
        { href: '/a-traiter', texte: 'À valider', court: 'Valider', icone: 'coche', tel: false, compteur: () => compteurs.aValider }
      ]
    },
    {
      titre: 'Suivi',
      liens: [
        { href: '/fournisseurs', texte: 'Fournisseurs', court: 'Fournisseurs', icone: 'batiment', tel: false },
        { href: '/bilan', texte: 'Bilan du mois', court: 'Bilan', icone: 'graphique', tel: false }
      ]
    }
  ];
  const onglets = groupes.flatMap((g) => g.liens).filter((l) => l.tel);
  function actif(href: string) {
    const p = page.url.pathname;
    if (href === '/') return p === '/';
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
  <div class="cadre">
    <!-- Écran étroit : barre compacte, le menu s'ouvre en tiroir. -->
    <header class="barre">
      <button type="button" class="btn-icone" aria-label="Ouvrir le menu" aria-expanded={tiroir} aria-controls="menu-lateral" onclick={() => (tiroir = true)}>
        <Icone nom="menu" />
      </button>
      <a class="logo" href="/" aria-label="GMFlow, accueil"><span class="marque" aria-hidden="true"></span>GMFlow</a>
      <span class="sp"></span>
      {#if etat && !etat.ok}
        <a class="etat-court" href="/a-traiter#bloque" title={etat.texte} aria-label={etat.texte}><span class="point"></span>{compteurs.problemes}</a>
      {/if}
      <a class="btn-icone" href="/recherche" aria-label="Chercher"><Icone nom="chercher" /></a>
    </header>

    {#if tiroir}
      <button type="button" class="voile" aria-label="Fermer le menu" onclick={() => (tiroir = false)}></button>
    {/if}

    <aside id="menu-lateral" class="lateral" class:ouvert={tiroir} aria-label="Menu principal">
      <div class="lat-h">
        <a class="logo" href="/" aria-label="GMFlow, accueil"><span class="marque" aria-hidden="true"></span>GMFlow</a>
        <button type="button" class="btn-icone fermer-tiroir" aria-label="Fermer le menu" onclick={() => (tiroir = false)}><Icone nom="fermer" /></button>
      </div>

      {#if admin && societe.liste.length > 0}
        <label class="sel">
          <span class="sr">Société</span>
          <select value={societe.id === null ? '' : String(societe.id)} onchange={(e) => societe.choisir(e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
            <option value="">Toutes les sociétés</option>
            {#each societe.liste as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
          <Icone nom="bas" />
        </label>
      {/if}

      <form class="recherche" class:cache={surRecherche || etroit} role="search" onsubmit={chercher}>
        <Icone nom="chercher" />
        <input
          bind:this={champRecherche}
          bind:value={recherche}
          type="search"
          placeholder="Chercher"
          aria-label="Chercher un document ou un mail (fournisseur, n°, montant)"
          onkeydown={(e) => {
            if (e.key === 'Escape') champRecherche?.blur();
          }}
        />
        <Kbd t="/" />
      </form>
      {#if etroit}
        <a class="lien" href="/recherche" class:cur={surRecherche}><Icone nom="chercher" /><span class="t">Chercher</span></a>
      {/if}

      <nav class="groupes">
        {#each groupes as g (g.titre)}
          <div class="groupe">
            {#if g.titre}<div class="g-titre">{g.titre}</div>{/if}
            {#each g.liens as l (l.href)}
              {@const n = l.compteur?.() ?? 0}
              <a class="lien" href={l.href} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
                <Icone nom={l.icone} /><span class="t">{l.texte}</span>
                {#if n > 0}<span class="compteur">{n}</span>{/if}
              </a>
            {/each}
          </div>
        {/each}
      </nav>

      <div class="lat-b">
        {#if etat}
          <a class="lien etat" class:ok={etat.ok} href={etat.ok ? '/' : '/a-traiter#bloque'} title={etat.ok ? 'Boîtes, Claude et envois fonctionnent' : 'Voir ce qui est bloqué'}>
            <span class="point"></span><span class="t">{etat.texte}</span>
          </a>
        {/if}
        {#if admin}
          <a class="lien" href="/reglages" class:cur={reglagesActif} aria-current={reglagesActif ? 'page' : undefined}><Icone nom="reglages" /><span class="t">Réglages</span></a>
        {/if}
        <div class="moi" data-menu>
          <button type="button" class="compte" aria-haspopup="menu" aria-expanded={menuOuvert} onclick={() => (menuOuvert = !menuOuvert)}>
            <span class="initiales">{initiales(session.moi.identifiant)}</span>
            <span class="qui-c">
              <span class="ident">{session.moi.identifiant}</span>
              <span class="muted petit">{libelleRole(session.moi.role)}</span>
            </span>
            <Icone nom="haut" />
          </button>
          {#if menuOuvert}
            <div class="deroulant" role="menu">
              <button type="button" class="item" role="menuitem" onclick={deconnecter}><Icone nom="sortir" />Se déconnecter</button>
            </div>
          {/if}
        </div>
      </div>
    </aside>

    <main class="contenu">{@render children()}</main>
  </div>

  <nav class="onglets" aria-label="Raccourcis">
    {#each onglets as l (l.href)}
      {@const n = l.compteur?.() ?? 0}
      <a href={l.href} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
        <Icone nom={l.icone} />
        <span>{l.court}{#if n > 0}&nbsp;<span class="compteur">{n}</span>{/if}</span>
      </a>
    {/each}
  </nav>
{/if}

<style>
  /* Ordinateur (≥ 1024 px) : barre latérale fixe à gauche, contenu à droite. */
  .cadre {
    display: grid;
    grid-template-columns: var(--larg-lateral) minmax(0, 1fr);
    min-height: 100dvh;
  }
  .barre {
    display: none;
  }
  .lateral {
    position: sticky;
    top: 0;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    gap: var(--e2);
    padding: var(--e3) var(--e3) var(--e2);
    border-right: 1px solid var(--line);
    background: var(--white);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .lat-h {
    display: flex;
    align-items: center;
    height: 36px;
    padding: 0 var(--e2);
  }
  .fermer-tiroir {
    display: none;
    margin-left: auto;
  }
  .logo {
    display: flex;
    align-items: center;
    gap: var(--e2);
    font-weight: 600;
    font-size: 15px;
    color: var(--black);
    text-decoration: none;
    border-radius: var(--r1);
  }

  .sel {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .sel select {
    width: 100%;
    height: 34px;
    padding: 0 30px 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--text);
    font-weight: 500;
    cursor: pointer;
    text-overflow: ellipsis;
  }
  .sel select:hover {
    border-color: var(--line2);
  }
  .sel :global(svg) {
    position: absolute;
    right: 9px;
    pointer-events: none;
  }

  .recherche {
    display: flex;
    align-items: center;
    gap: var(--e2);
    height: 34px;
    padding: 0 var(--e2) 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--muted);
    transition:
      border-color var(--vite) var(--ease),
      box-shadow var(--vite) var(--ease);
  }
  .recherche.cache {
    display: none;
  }
  .recherche:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-weak);
  }
  .recherche input {
    flex: 1;
    min-width: 0;
    height: 32px;
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

  .groupes {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    margin-top: var(--e2);
  }
  .groupe {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .g-titre {
    padding: 0 10px var(--e1);
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }
  .lien {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r2);
    color: var(--text2);
    font-size: 14px;
    font-weight: 500;
    text-decoration: none;
    white-space: nowrap;
    transition:
      background-color var(--vite) var(--ease),
      color var(--vite) var(--ease);
  }
  .lien :global(svg) {
    flex: none;
    color: var(--muted);
  }
  .lien .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lien:hover {
    background: var(--pane);
    color: var(--text);
  }
  .lien.cur {
    background: var(--sunk);
    color: var(--black);
    font-weight: 600;
  }
  .lien.cur :global(svg) {
    color: var(--accent);
  }

  .lat-b {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: var(--e2);
    border-top: 1px solid var(--line);
  }
  .etat {
    color: var(--err);
  }
  .etat .point {
    margin: 0 4px;
  }
  .etat:hover {
    background: var(--err-bg);
    color: var(--err);
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
  .compte {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 44px;
    padding: 0 var(--e2) 0 6px;
    border: 0;
    border-radius: var(--r2);
    background: none;
    text-align: left;
    color: var(--muted);
    transition: background-color var(--vite) var(--ease);
  }
  .compte:hover,
  .compte[aria-expanded='true'] {
    background: var(--pane);
  }
  .initiales {
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--sunk);
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 600;
    color: var(--text2);
  }
  .qui-c {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 16px;
  }
  .ident {
    color: var(--text);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .deroulant {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 6px);
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
      transform: translateY(4px);
    }
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r2);
    background: none;
    color: var(--text);
    text-align: left;
  }
  .item:hover {
    background: var(--pane);
  }
  .item :global(svg) {
    color: var(--text2);
  }

  .contenu {
    min-width: 0;
    min-height: calc(100dvh - var(--haut-barre));
  }
  main.seul {
    margin: 8vh auto;
    max-width: 400px;
    padding: 0 var(--e4);
  }
  .onglets,
  .voile {
    display: none;
  }

  /* Tablette et téléphone : barre compacte en haut, la barre latérale devient un tiroir. */
  @media (max-width: 1023px) {
    .cadre {
      display: block;
    }
    .barre {
      position: sticky;
      top: 0;
      z-index: 20;
      display: flex;
      align-items: center;
      gap: var(--e2);
      height: var(--haut-barre);
      padding: 0 var(--e3);
      border-bottom: 1px solid var(--line);
      background: var(--white);
    }
    .barre .logo {
      margin-left: var(--e1);
    }
    .sp {
      flex: 1;
    }
    .etat-court {
      display: flex;
      align-items: center;
      gap: 6px;
      height: 32px;
      padding: 0 10px;
      border-radius: var(--r2);
      background: var(--err-bg);
      color: var(--err);
      font-weight: 600;
      text-decoration: none;
    }
    .lateral {
      position: fixed;
      top: 0;
      left: 0;
      bottom: 0;
      z-index: 40;
      width: min(300px, 86vw);
      height: auto;
      padding-bottom: calc(var(--e2) + env(safe-area-inset-bottom));
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.16);
      transform: translateX(-104%);
      visibility: hidden;
      transition:
        transform var(--moyen) var(--ease),
        visibility 0s linear var(--moyen);
    }
    .lateral.ouvert {
      transform: none;
      visibility: visible;
      transition: transform var(--moyen) var(--ease);
    }
    .fermer-tiroir {
      display: inline-grid;
    }
    .voile {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 35;
      border: 0;
      padding: 0;
      background: rgba(0, 0, 0, 0.28);
      animation: voile-entre var(--moyen) var(--ease);
    }
    @keyframes voile-entre {
      from {
        opacity: 0;
      }
    }
    .lien,
    .item {
      height: 40px;
    }
  }

  /* Téléphone : onglets en bas pour les pages de tous les jours. */
  @media (max-width: 760px) {
    .sel select {
      font-size: 16px;
      height: 40px;
    }
    .onglets {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 15;
      display: grid;
      grid-template-columns: repeat(5, 1fr);
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

  @media (prefers-reduced-motion: reduce) {
    .lateral,
    .lateral.ouvert {
      transition: none;
    }
  }
</style>
