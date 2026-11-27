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
  import { quandCourt } from '#lib/ui/DocOutils.ts';
  import Icone, { type NomIcone } from '#lib/ui/Icone.svelte';
  import AideClavier from '#lib/ui/AideClavier.svelte';
  import AnnulerEnvoi from '#lib/ui/AnnulerEnvoi.svelte';
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
  let aideClavier = $state(false);
  $effect(() => {
    if (!session.moi) return;
    return raccourcis({
      '?': () => (aideClavier = !aideClavier),
      '/': () => {
        // Champ de la barre du haut masqué (écran étroit, page Recherche) : celui de la page Recherche.
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

  /** « michael.schal@… » → « Michael » : le prénom affiché en bas de la barre latérale. */
  function prenom(identifiant: string): string {
    const p = identifiant.split('@')[0].split(/[.\-_\s]+/).filter(Boolean)[0] ?? identifiant;
    return p.charAt(0).toUpperCase() + p.slice(1);
  }

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
  // Rangé dans l'ordre du trajet d'une pièce : 1 Courrier, 2 Pièces, 3 Fournisseurs, 4 Compta (étape F).
  // `tel` : présent dans les onglets du bas sur téléphone.
  // `compteur` : badge (choses à faire) ; `aFaire` : où mène le lien quand il y en a (le filtre « À contrôler »).
  type Lien = { href: string; texte: string; court: string; icone: NomIcone; tel: boolean; compteur?: () => number; aFaire?: string; echecs?: () => number; echec?: string };
  type Groupe = { titre: string; numero?: number; liens: Lien[] };
  const groupes: Groupe[] = [
    { titre: '', liens: [{ href: '/', texte: 'Accueil', court: 'Accueil', icone: 'maison', tel: true }] },
    {
      titre: 'Courrier',
      numero: 1,
      liens: [
        { href: '/mails', texte: 'Mails', court: 'Mails', icone: 'mail', tel: true },
        { href: '/a-repondre', texte: 'À répondre', court: 'Répondre', icone: 'envoyer', tel: true, compteur: () => compteurs.aRepondre }
      ]
    },
    {
      titre: 'Pièces',
      numero: 2,
      liens: [
        { href: '/factures', texte: 'Factures', court: 'Factures', icone: 'fichier', tel: true, compteur: () => compteurs.factures, aFaire: '/factures?filtre=a-valider' },
        { href: '/avoirs', texte: 'Avoirs', court: 'Avoirs', icone: 'avoir', tel: false, compteur: () => compteurs.avoirs, aFaire: '/avoirs?filtre=a-valider' },
        { href: '/bl', texte: 'Bons de livraison', court: 'BL', icone: 'camion', tel: true, compteur: () => compteurs.bl, aFaire: '/bl?filtre=a-valider' }
      ]
    },
    {
      titre: 'Fournisseurs',
      numero: 3,
      liens: [
        { href: '/fournisseurs', texte: 'Fournisseurs', court: 'Fournisseurs', icone: 'batiment', tel: false },
        { href: '/fournisseurs?onglet=nouveaux', texte: 'À confirmer', court: 'À confirmer', icone: 'coche', tel: false, compteur: () => compteurs.fournisseursAConfirmer }
      ]
    },
    {
      titre: 'Compta',
      numero: 4,
      liens: [
        { href: '/envois', texte: 'Envois Dext / Yooz', court: 'Envois', icone: 'envoyer', tel: false, echecs: () => compteurs.echecsFactures + compteurs.echecsAvoirs, echec: '/envois?filtre=echec' },
        { href: '/bl?filtre=valides', texte: 'Export Phénix', court: 'Phénix', icone: 'telecharger', tel: false },
        { href: '/bilan', texte: 'Bilan du mois', court: 'Bilan', icone: 'graphique', tel: false }
      ]
    }
  ];
  const tousLiens = groupes.flatMap((g) => g.liens);
  const onglets = tousLiens.filter((l) => l.tel);
  /** Le lien correspond-il à l'adresse ? Un lien avec paramètres (« À confirmer ») exige ces paramètres. */
  function correspond(href: string): boolean {
    const [chemin, requete] = href.split('?');
    const p = page.url.pathname;
    if (chemin === '/') return p === '/';
    if (p !== chemin && !p.startsWith(`${chemin}/`)) return false;
    if (!requete) return true;
    return [...new URLSearchParams(requete)].every(([k, v]) => page.url.searchParams.get(k) === v);
  }
  /** Un seul lien allumé : entre deux liens de la même page, celui qui a des paramètres l'emporte. */
  function actif(href: string) {
    if (!correspond(href)) return false;
    if (href.includes('?')) return true;
    return !tousLiens.some((l) => l.href !== href && l.href.includes('?') && l.href.split('?')[0] === href && correspond(l.href));
  }
  let reglagesActif = $derived(page.url.pathname.startsWith('/reglages') || page.url.pathname.startsWith('/admin'));
  let etat = $derived.by(() => {
    if (!admin || compteurs.problemes === null) return null;
    const n = compteurs.problemes;
    if (n > 0) return { ok: false, texte: `${n} problème${n > 1 ? 's' : ''}`, href: '/#bloque', titre: 'Voir ce qui est bloqué' };
    // Aucun blocage, mais aucune relève récente : pas de vert trompeur (la relève part toutes les 5 minutes).
    const r = compteurs.derniereReleve;
    const ancienne = !r || Date.now() - new Date(r).getTime() > 30 * 60_000;
    if (compteurs.boitesActives > 0 && ancienne)
      return {
        ok: false,
        texte: 'Pas de relève récente',
        href: '/reglages/traitements',
        titre: r ? `Dernière relève : ${quandCourt(r)}` : 'Aucune relève pour l’instant'
      };
    return {
      ok: true,
      texte: 'Tout fonctionne',
      href: '/',
      titre: r ? `Boîtes, Claude et envois fonctionnent. Dernière relève : ${quandCourt(r)}` : 'Boîtes, Claude et envois fonctionnent'
    };
  });
</script>

{#if publique}
  <main class="seul">{@render children()}</main>
{:else if session.moi}
  <div class="cadre" class:sur-accueil={page.url.pathname === '/'}>
    <!-- Barre du haut : la recherche seule (état, réglages et compte sont en bas de la barre latérale).
         Écran étroit : bouton du menu (tiroir), logo, loupe à la place du champ, et l'état s'il y a un problème. -->
    <header class="barre">
      <button type="button" class="btn-icone etroit-seul" aria-label="Ouvrir le menu" aria-expanded={tiroir} aria-controls="menu-lateral" onclick={() => (tiroir = true)}>
        <Icone nom="menu" />
      </button>
      <a class="logo etroit-seul" href="/" aria-label="GMFlow, accueil"><span class="marque" aria-hidden="true"></span>GMFlow</a>

      <form class="recherche" class:cache={surRecherche} role="search" onsubmit={chercher}>
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
      <span class="sp"></span>
      <a class="btn-icone etroit-seul" href="/recherche" aria-label="Chercher"><Icone nom="chercher" /></a>

      {#if etat && !etat.ok}
        <a class="etat etroit-seul" href={etat.href} title={etat.titre} aria-label={etat.texte}>
          <span class="pastille"></span><span class="etat-n">{compteurs.problemes || '!'}</span>
        </a>
      {/if}
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
          <span class="sr">Société affichée</span>
          <select value={societe.id === null ? '' : String(societe.id)} onchange={(e) => societe.choisir(e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
            <option value="">Toutes les sociétés</option>
            {#each societe.liste as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
          <Icone nom="bas" />
        </label>
      {/if}

      {#if etroit}
        <a class="lien" href="/recherche" class:cur={surRecherche}><Icone nom="chercher" /><span class="t">Chercher</span></a>
      {/if}

      <nav class="groupes">
        {#each groupes as g (g.titre)}
          <div class="groupe">
            {#if g.titre}<div class="g-titre">{#if g.numero}<span class="g-num" aria-hidden="true">{g.numero}</span>{/if}{g.titre}</div>{/if}
            {#each g.liens as l (l.href)}
              {@const n = l.compteur?.() ?? 0}
              {@const e = l.echecs?.() ?? 0}
              <a class="lien" class:en-groupe={!!g.numero} href={n > 0 && l.aFaire ? l.aFaire : e > 0 && l.echec ? l.echec : l.href} title={n > 0 && l.aFaire ? `${n} à contrôler` : undefined} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
                {#if !g.numero}<Icone nom={l.icone} />{/if}<span class="t">{l.texte}</span>
                {#if e > 0}<span class="echec-point" role="img" aria-label="{e} envoi{e > 1 ? 's' : ''} en échec" title="{e} envoi{e > 1 ? 's' : ''} en échec"></span>{/if}
                {#if n > 0}<span class="compteur">{n}</span>{/if}
              </a>
            {/each}
          </div>
        {/each}
      </nav>

      <span class="sp-lat"></span>
      <!-- Bas de la barre latérale : état du service, réglages, compte (menu ouvert vers le haut). -->
      <div class="bas">
        {#if etat}
          <a class="etat" class:ok={etat.ok} href={etat.href} title={etat.titre}>
            <span class="pastille"></span><span class="etat-t">{etat.texte}</span>
          </a>
        {/if}
        {#if admin}
          <a class="lien" href="/reglages" class:cur={reglagesActif} aria-current={reglagesActif ? 'page' : undefined}><Icone nom="curseurs" /><span class="t">Réglages</span></a>
        {/if}
        <div class="moi" data-menu>
          <button type="button" class="lien compte" aria-haspopup="menu" aria-expanded={menuOuvert} title={session.moi.identifiant} onclick={() => (menuOuvert = !menuOuvert)}>
            <span class="initiales" aria-hidden="true">{initiales(session.moi.identifiant)}</span><span class="t"><span class="sr-seul">Compte </span>{prenom(session.moi.identifiant)}</span>
          </button>
          {#if menuOuvert}
            <div class="deroulant" role="menu">
              <div class="qui">
                <div class="ident">{session.moi.identifiant}</div>
                <div class="muted petit">{libelleRole(session.moi.role)}</div>
              </div>
              <a class="item" role="menuitem" href="/compte"><Icone nom="utilisateur" />Mon compte</a>
              {#if admin}<a class="item" role="menuitem" href="/reglages"><Icone nom="reglages" />Réglages</a>{/if}
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
      {@const e = l.echecs?.() ?? 0}
      <a href={n > 0 && l.aFaire ? l.aFaire : e > 0 && l.echec ? l.echec : l.href} class:cur={actif(l.href)} aria-current={actif(l.href) ? 'page' : undefined}>
        <Icone nom={l.icone} />
        <span>{l.court}{#if e > 0}&nbsp;<span class="echec-point" role="img" aria-label="{e} envoi{e > 1 ? 's' : ''} en échec"></span>{/if}{#if n > 0}&nbsp;<span class="compteur">{n}</span>{/if}</span>
      </a>
    {/each}
  </nav>
{/if}

{#if session.moi && !publique}<AideClavier bind:ouvert={aideClavier} /><AnnulerEnvoi />{/if}

<style>
  /* Ordinateur (≥ 1024 px) : barre latérale sauge à gauche, colonne blanche à droite (maquette Sauge). */
  .cadre {
    display: grid;
    grid-template-columns: var(--larg-lateral) minmax(0, 1fr);
    grid-template-rows: var(--haut-barre) minmax(0, 1fr);
    min-height: 100dvh;
    background: var(--bg);
  }
  .barre {
    grid-column: 2;
    grid-row: 1;
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: var(--e4);
    height: var(--haut-barre);
    padding: 0 var(--e6);
    border-bottom: 1px solid var(--line);
    border-left: 1px solid var(--line);
    background: var(--white);
  }
  .barre > .etroit-seul {
    display: none;
  }
  .sp {
    flex: 1;
  }
  .contenu {
    grid-column: 2;
    grid-row: 2;
    position: relative; /* un élément en absolu (.sr-seul) reste dans le contenu */
    background: var(--white);
    border-left: 1px solid var(--line);
  }
  /* Pages à volets (Factures, BL, Mails…) et fiche plein écran : la page ne défile jamais,
     seules les colonnes défilent ; ce qui dépasserait est coupé au lieu d'allonger la page. */
  @media (min-width: 761px) {
    .contenu:has(> :global(.volets)),
    .contenu:has(> :global(.pleine.fixe)) {
      height: calc(100dvh - var(--haut-barre));
      min-height: 0;
      overflow: clip;
    }
  }
  /* Accueil : pas de colonne blanche, des cartes sur le fond sauge. */
  .cadre.sur-accueil .barre {
    position: static;
    height: auto;
    min-height: var(--haut-barre);
    padding: var(--e6) var(--e10) 0;
    border: 0;
    background: none;
  }
  .cadre.sur-accueil .contenu {
    background: none;
    border-left: 0;
  }
  .lateral {
    grid-column: 1;
    grid-row: 1 / 3;
    position: sticky;
    top: 0;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    gap: var(--e5);
    padding: var(--e5) var(--e4) var(--e4);
    background: var(--bg);
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .sp-lat {
    flex: 1 0 0;
    margin-top: calc(-1 * var(--e5));
  }
  /* bas de la barre latérale : état, Réglages, compte */
  .bas {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: var(--e3);
    border-top: 1px solid var(--line);
  }
  .lat-h {
    flex: none;
    display: flex;
    align-items: center;
    height: 32px;
    padding: 0 var(--e3);
  }
  .fermer-tiroir {
    display: none;
    margin-left: auto;
  }
  .logo {
    display: flex;
    align-items: center;
    gap: var(--e3);
    font-weight: 600;
    font-size: 18px;
    color: var(--text);
    text-decoration: none;
    border-radius: var(--r1);
  }
  .logo:hover {
    color: var(--text);
  }

  .sel {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--muted);
  }
  .sel select {
    width: 100%;
    height: 44px;
    padding: 0 40px 0 16px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--text);
    font-size: 16px;
    cursor: pointer;
    text-overflow: ellipsis;
  }
  .sel select:hover {
    border-color: var(--champ);
  }
  .sel :global(svg) {
    position: absolute;
    right: 14px;
    pointer-events: none;
  }

  .recherche {
    display: flex;
    align-items: center;
    gap: var(--e3);
    flex: 0 1 480px;
    height: 44px;
    padding: 0 var(--e4);
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--muted);
    transition:
      border-color var(--vite) var(--ease),
      box-shadow var(--vite) var(--ease);
  }
  .recherche.cache {
    visibility: hidden;
  }
  .recherche:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-weak);
  }
  .recherche input {
    flex: 1;
    min-width: 0;
    height: 40px;
    padding: 0;
    border: 0;
    background: none;
    box-shadow: none;
    color: var(--text);
    font-size: 17px;
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
    gap: var(--e5);
  }
  .groupe {
    display: flex;
    flex-direction: column;
    gap: var(--e1);
  }
  /* Écran peu haut (portable 768 px) : les quatre étapes tiennent sans faire défiler la barre. */
  @media (max-height: 860px) {
    .lateral {
      gap: var(--e3);
      padding-top: var(--e4);
    }
    .sp-lat {
      margin-top: calc(-1 * var(--e3));
    }
    .groupes {
      gap: var(--e3);
    }
    .groupe {
      gap: 0;
    }
    .groupes .lien,
    .bas .lien {
      height: 32px;
    }
    .bas .etat {
      height: 32px;
    }
    .sel select {
      height: 38px;
    }
    .g-titre {
      padding-bottom: 2px;
    }
  }
  /* pastille d'étape : 1 Courrier, 2 Pièces, 3 Fournisseurs, 4 Compta */
  .g-num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent-weak);
    color: var(--accent-ink);
    font-size: 13px;
    font-weight: 700;
  }
  .g-titre {
    display: flex;
    align-items: center;
    gap: var(--e2);
    padding: 0 var(--e3) var(--e1);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text2);
  }
  .lien {
    display: flex;
    align-items: center;
    gap: var(--e3);
    height: 40px;
    padding: 0 var(--e3);
    border-radius: var(--r2);
    color: var(--text);
    font-size: 16px;
    font-weight: 450;
    text-decoration: none;
    white-space: nowrap;
    transition:
      background-color var(--vite) var(--ease),
      color var(--vite) var(--ease);
  }
  .lien :global(svg) {
    flex: none;
  }
  .lien .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lien:hover {
    background: var(--sunk);
    color: var(--text);
  }
  /* lien d'un groupe : aligné sur le titre, après la pastille d'étape (12 + 22 + 8 px) */
  .lien.en-groupe {
    padding-left: 42px;
  }
  /* page ouverte : tuile blanche bordée, barre d'accent à gauche */
  .lien.cur {
    background: var(--white);
    color: var(--text);
    font-weight: 600;
    box-shadow:
      inset 3px 0 0 var(--accent),
      0 0 0 1px var(--line);
  }
  /* compteur en pastille ; ambre sur la page ouverte (des choses à contrôler) */
  .lien .compteur {
    margin-left: auto;
    display: inline-grid;
    place-items: center;
    min-width: 24px;
    height: 22px;
    padding: 0 7px;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--white);
    color: var(--text);
    font-size: 13px;
    font-weight: 600;
  }
  .lien.cur .compteur {
    background: var(--warn-p);
    border-color: var(--warn-line);
    color: var(--warn-ink);
  }
  .echec-point {
    flex: none;
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--err);
  }
  .lien .echec-point {
    margin-left: auto;
  }
  .lien .echec-point + .compteur {
    margin-left: var(--e2);
  }

  .etat {
    display: flex;
    align-items: center;
    gap: var(--e2);
    height: 36px;
    padding: 0 var(--e3);
    border-radius: var(--r2);
    background: var(--err-p);
    color: var(--err-ink);
    font-size: 15px;
    font-weight: 500;
    text-decoration: none;
    white-space: nowrap;
    transition: background-color var(--vite) var(--ease);
  }
  .etat:hover {
    color: var(--err-ink);
    box-shadow: 0 0 0 1px var(--err-line);
  }
  .pastille {
    width: 8px;
    height: 8px;
    border-radius: 4px;
    background: currentColor;
    flex: none;
  }
  .etat.ok {
    background: var(--ok-p);
    color: var(--ok-ink);
  }
  .etat.ok .pastille {
    background: var(--ok-point);
  }
  .etat.ok:hover {
    color: var(--ok-ink);
    box-shadow: 0 0 0 1px var(--line);
  }
  .etat-t {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .etat-n {
    font-weight: 600;
  }

  .moi {
    position: relative;
  }
  .lien.compte {
    width: 100%;
    border: 0;
    background: none;
    text-align: left;
  }
  .lien.compte[aria-expanded='true'] {
    background: var(--sunk);
  }
  .initiales {
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--sunk);
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
    color: var(--text2);
  }
  .lien.compte:hover .initiales,
  .lien.compte[aria-expanded='true'] .initiales {
    background: var(--line);
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
  /* menu du compte : s'ouvre vers le haut, au-dessus du bouton */
  .deroulant {
    position: absolute;
    left: 0;
    bottom: calc(100% + 8px);
    z-index: 30;
    min-width: 260px;
    padding: var(--e2);
    border: 0;
    border-radius: var(--r3);
    background: var(--white);
    box-shadow:
      0 0 0 1px var(--line),
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
    height: 40px;
    padding: 0 var(--e3);
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text);
    text-align: left;
    text-decoration: none;
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
    :global(:root) {
      --haut-barre: 60px;
    }
    .barre,
    .cadre.sur-accueil .barre {
      position: sticky;
      height: var(--haut-barre);
      min-height: 0;
      gap: var(--e1);
      padding: 0 var(--e3);
      border-left: 0;
      border-bottom: 1px solid var(--line);
      background: var(--white);
    }
    .contenu {
      border-left: 0;
    }
    .barre > .etroit-seul {
      display: inline-grid;
    }
    .barre > .logo {
      display: flex;
      margin-left: var(--e1);
    }
    .recherche {
      display: none;
    }
    .barre > .etat {
      display: inline-flex;
      gap: 6px;
      height: 40px;
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
    .item {
      height: 44px;
    }
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
      height: calc(60px + env(safe-area-inset-bottom));
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
      font-size: 13px;
      color: var(--text2);
      text-decoration: none;
    }
    .onglets a.cur {
      color: var(--accent);
      font-weight: 500;
    }
    .onglets .compteur {
      font-size: 12px;
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
