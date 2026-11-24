<script lang="ts">
  // Fournisseurs : liste de la société choisie à gauche (ligne d'état, recherche, onglets),
  // fiche ouverte à droite par `?fiche=ID` (même adresse que l'ancienne page). Plein écran : /fournisseurs/[id].
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, fournisseurs, message } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { LigneFournisseur } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import FournEtat, { aDesAlertes, texteEtat } from '#lib/ui/FournEtat.svelte';
  import FournDoublons from '#lib/ui/FournDoublons.svelte';
  import FournFiche from '#lib/ui/FournFiche.svelte';
  import FournNouveau from '#lib/ui/FournNouveau.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import Volets from '#lib/ui/Volets.svelte';

  type Onglet = 'tous' | 'nouveaux' | 'doublons';
  const PAR_PAGE = 50;

  let admin = $derived(session.moi?.role === 'admin');
  // Responsable : uniquement sa société (le serveur rendrait sinon tout le référentiel).
  let societeChoisie = $derived(admin ? societe.id : (session.moi?.societe_id ?? null));

  let q = $state('');
  let qActif = $state('');
  let onglet = $state<Onglet>('tous');
  let lignes = $state<LigneFournisseur[]>([]);
  let total = $state(0);
  let totaux = $state<Record<Onglet, number>>({ tous: 0, nouveaux: 0, doublons: 0 });
  let numeroPage = $state(1);
  let charge = $state(false);
  let enCours = $state(false);
  let erreur = $state('');
  let conteneur = $state<HTMLElement>();

  let fiche = $derived(page.url.searchParams.get('fiche'));
  let nouveau = $derived(admin && fiche === 'nouveau');
  let idOuvert = $derived(fiche && /^\d+$/.test(fiche) ? Number(fiche) : null);
  let indice = $derived(idOuvert === null ? -1 : lignes.findIndex((l) => l.id === idOuvert));

  // Recherche instantanée : on attend que la frappe se calme.
  $effect(() => {
    const v = q.trim();
    const t = setTimeout(() => (qActif = v), 200);
    return () => clearTimeout(t);
  });

  let jeton = 0;
  async function charger(suite = false) {
    const moi = ++jeton;
    enCours = true;
    const p = suite ? numeroPage + 1 : 1;
    const base = { societe: societeChoisie, q: qActif || undefined };
    try {
      const [courante, autre] = await Promise.all([
        fournisseurs({ ...base, statut: onglet === 'nouveaux' ? 'a_confirmer' : undefined, page: p }),
        suite ? null : fournisseurs({ ...base, statut: onglet === 'nouveaux' ? undefined : 'a_confirmer' })
      ]);
      if (moi !== jeton) return;
      lignes = suite ? [...lignes, ...courante.elements.filter((l) => !lignes.some((x) => x.id === l.id))] : courante.elements;
      total = courante.total;
      numeroPage = p;
      if (autre) {
        totaux =
          onglet === 'nouveaux'
            ? { ...totaux, tous: autre.total, nouveaux: courante.total }
            : { ...totaux, tous: courante.total, nouveaux: autre.total };
      }
      erreur = '';
    } catch (e) {
      if (moi === jeton) erreur = message(e);
    } finally {
      if (moi === jeton) {
        enCours = false;
        charge = true;
      }
    }
  }

  $effect(() => {
    void qActif;
    void onglet;
    void societeChoisie;
    if (!session.moi) return;
    void charger();
  });

  // Nombre de doublons probables, pour l'onglet (admin).
  $effect(() => {
    if (!admin) return;
    api
      .get<unknown[]>('/api/admin/fournisseurs/doublons')
      .then((v) => (totaux.doublons = v.length))
      .catch(() => {});
  });

  function ouvrir(id: number | 'nouveau' | null, remplacer = true) {
    const u = id === null ? '/fournisseurs' : `/fournisseurs?fiche=${id}`;
    return goto(u, { replace: remplacer, reset: false });
  }

  function bouger(sens: 1 | -1) {
    const i = deplacer(indice, sens, lignes.length);
    if (i < 0) return;
    void ouvrir(lignes[i].id).then(() => montrer(conteneur, `[data-id="${lignes[i]?.id}"]`));
  }

  $effect(() =>
    raccourcis({
      j: () => bouger(1),
      k: () => bouger(-1),
      ArrowDown: () => bouger(1),
      ArrowUp: () => bouger(-1),
      Enter: () => {
        if (idOuvert === null) return false;
        void goto(`/fournisseurs/${idOuvert}`);
      },
      f: () => {
        if (idOuvert === null) return false;
        void goto(`/fournisseurs/${idOuvert}`);
      },
      Escape: () => {
        if (fiche === null) return false;
        void ouvrir(null);
      }
    })
  );

  const court = new Intl.DateTimeFormat('fr-FR', { day: '2-digit', month: '2-digit', timeZone: 'UTC' });
  const courtAnnee = new Intl.DateTimeFormat('fr-FR', { day: '2-digit', month: '2-digit', year: '2-digit', timeZone: 'UTC' });
  function dateCourte(j: string | null): string {
    if (!j) return '';
    const d = new Date(`${j}T12:00:00Z`);
    return (d.getUTCFullYear() === new Date().getFullYear() ? court : courtAnnee).format(d);
  }

  let titreVide = $derived(
    qActif
      ? `Aucun fournisseur pour « ${qActif} »`
      : onglet === 'nouveaux'
        ? 'Aucun nouveau fournisseur à confirmer'
        : societeChoisie !== null
          ? 'Aucun fournisseur pour cette société'
          : 'Aucun fournisseur'
  );
</script>

<svelte:head><title>Fournisseurs · GMFlow</title></svelte:head>

<Volets ouvert={idOuvert !== null || nouveau} libelleListe="Fournisseurs" largeur={340}>
  {#snippet liste()}
    <div class="liste-h">
      <h1>Fournisseurs {#if charge}<span class="num">{totaux.tous}</span>{/if}</h1>
      {#if admin && societe.courante}<span class="muted petit societe-nom">{nomCourtSociete(societe.courante.raison_sociale)}</span>{/if}
      {#if admin}
        <a class="btn nouveau" href="/fournisseurs?fiche=nouveau" aria-current={nouveau ? 'true' : undefined}>Nouveau</a>
      {/if}
    </div>
    <label class="recherche">
      <span class="sr">Filtrer les fournisseurs</span>
      <Icone nom="chercher" />
      <input type="search" placeholder="Nom, code BO, SIRET, TVA, domaine" bind:value={q} autocomplete="off" />
    </label>
    <div class="puces" role="group" aria-label="Filtrer">
      <button type="button" class="btn" aria-pressed={onglet === 'tous'} onclick={() => (onglet = 'tous')}>
        Tous {#if charge}<span class="num">{totaux.tous}</span>{/if}
      </button>
      <button type="button" class="btn" aria-pressed={onglet === 'nouveaux'} onclick={() => (onglet = 'nouveaux')}>
        À confirmer {#if totaux.nouveaux}<span class="num">{totaux.nouveaux}</span>{/if}
      </button>
      {#if admin}
        <button type="button" class="btn" aria-pressed={onglet === 'doublons'} onclick={() => (onglet = 'doublons')}>
          Doublons {#if totaux.doublons}<span class="num">{totaux.doublons}</span>{/if}
        </button>
      {/if}
    </div>
    <div class="liste-s" bind:this={conteneur} aria-busy={enCours}>
      {#if onglet === 'doublons'}
        <FournDoublons
          ouvert={idOuvert}
          onouvrir={(id) => ouvrir(id)}
          onchange={(n) => {
            totaux.doublons = n;
            void charger();
          }}
        />
      {:else}
        {#if erreur}
          <Bandeau ton="erreur">
            {erreur}
            {#snippet action()}<button type="button" class="btn sm" onclick={() => charger()}>Réessayer</button>{/snippet}
          </Bandeau>
        {:else if !charge}
          <Chargement />
        {/if}
        <div class="tuiles">
          {#each lignes as l (l.id)}
            <a
              class="ligne tuile"
              href="/fournisseurs?fiche={l.id}"
              data-id={l.id}
              data-sveltekit-reset="false"
              aria-current={l.id === idOuvert ? 'true' : undefined}
              title={texteEtat(l)}
            >
              <div class="a">
                {l.nom}{#if l.code_gestion}{' '}<span class="code num">{l.code_gestion}</span>{/if}
              </div>
              <div class="r date num">{dateCourte(l.dernier_document)}</div>
              <div class="b">
                <FournEtat etat={l} quoi="texte" />{#if l.groupe}<span class="groupe">{' · groupe '}{l.groupe}</span>{/if}
              </div>
              {#if aDesAlertes(l) || l.statut === 'a_confirmer'}
                <div class="c">
                  {#if l.statut === 'a_confirmer'}<span class="etiquette att">À confirmer</span>{/if}
                  <FournEtat etat={l} quoi="etiquettes" />
                </div>
              {/if}
            </a>
          {:else}
            {#if charge && !erreur}<Vide compact titre={titreVide} />{/if}
          {/each}
        </div>
        {#if lignes.length < total}
          <button type="button" class="plus" disabled={enCours} onclick={() => charger(true)}>
            Voir <span class="num">{Math.min(PAR_PAGE, total - lignes.length)}</span> de plus (<span class="num">{total - lignes.length}</span> restants)
          </button>
        {/if}
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      <span><Kbd t="Enter" /> plein écran</span>
    </div>
  {/snippet}

  {#snippet detail()}
    <div class="volet-h">
      <a class="retour tel-seul" href="/fournisseurs"><Icone nom="gauche" />Fournisseurs</a>
      {#if indice >= 0}
        <span class="pos large-seul"><span class="num">{indice + 1}</span> sur <span class="num">{total}</span></span>
      {/if}
      <span class="sp"></span>
      <div class="outils-h large-seul">
        {#if idOuvert !== null}
          <a class="btn-h" href="/fournisseurs/{idOuvert}">Plein écran <Kbd t="F" /></a>
        {/if}
        <button type="button" class="btn-h" onclick={() => ouvrir(null)}>Fermer <Kbd t="Escape" /></button>
      </div>
    </div>
    <div class="volet-b">
      <div class="contenu-fiche fourn">
        {#if nouveau}
          <FournNouveau
            oncree={(id) => {
              void ouvrir(id);
              void charger();
            }}
            onannule={() => ouvrir(null)}
          />
        {:else if idOuvert !== null}
          {#key idOuvert}
            <FournFiche id={idOuvert} onmodifie={() => charger()} onfusion={(cible) => ouvrir(cible)} />
          {/key}
        {/if}
      </div>
    </div>
  {/snippet}

  {#snippet vide()}
    <Vide icone="camion" titre="Choisissez un fournisseur" texte="Ses factures, BL et mails mois par mois, et ce qui reste à rapprocher." />
  {/snippet}
</Volets>

<style>
  .liste-h {
    align-items: center;
  }
  .societe-nom {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nouveau {
    height: 36px;
    flex: none;
  }
  .recherche {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    margin: 0 4px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    color: var(--muted);
  }
  .recherche:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .recherche input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    box-shadow: none;
    font-size: 15px;
    color: var(--text);
  }
  .tuiles {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tuile {
    row-gap: 4px;
  }
  .tuile .code {
    font-size: 14px;
    font-weight: 400;
    color: var(--muted);
  }
  .tuile .date {
    font-size: 14px;
    font-weight: 400;
    color: var(--text2);
  }
  .tuile .b {
    font-size: 15px;
  }
  .tuile .c {
    margin-top: 0;
    gap: 4px;
  }
  .groupe {
    color: var(--muted);
  }
  .outils-h {
    display: flex;
    gap: 4px;
  }
  .btn-h {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text2);
    font-size: 15px;
    text-decoration: none;
  }
  .btn-h:hover {
    background: var(--pane);
    color: var(--text);
  }
  .volet-h {
    padding: 0 32px;
  }
  .contenu-fiche.fourn {
    padding: 32px;
  }
  @media (max-width: 760px) {
    .volet-h {
      padding: 0 var(--e3) 0 var(--e4);
    }
    .contenu-fiche.fourn {
      padding: var(--e5) var(--e4) var(--e8);
    }
  }
</style>
