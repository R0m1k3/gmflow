<script lang="ts" module>
  export type FiltreDocuments = { v: string; l: string; params: Record<string, string> };
</script>

<script lang="ts">
  // Onglets Factures et BL (maquette Sauge S4, S5) : les documents reçus, les plus récents en haut,
  // groupés par jour de réception. Filtres en puces (paramètres de GET /api/documents), fiche du
  // document à droite (?doc=ID) : aperçu au centre, champs et actions dans la colonne de droite.
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { requete, sonder } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { libelleEtat, libelleStatutEnvoi, nomCourtSociete, tonEtat, tonStatutEnvoi, type Ton } from '#lib/libelles.ts';
  import { Liste, parJour } from '#lib/liste.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { LigneDocument } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Chargement from './Chargement.svelte';
  import DocFiche, { type ResultatFiche } from './DocFiche.svelte';
  import { blSansFacture, dateFr, nomCourtDocument, typeNumero } from './DocOutils.ts';
  import { formatMontant } from './Montant.svelte';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import Montant from './Montant.svelte';
  import Vide from './Vide.svelte';
  import Volets from './Volets.svelte';

  let {
    titre,
    chemin,
    filtres,
    rechercheType,
    videTitre,
    singulier,
    section,
    sansSuite,
    pied = 'chercher'
  }: {
    /** « Factures », « BL ». */
    titre: string;
    /** `/factures`, `/bl`. */
    chemin: string;
    /** Le premier est le filtre par défaut. */
    filtres: FiltreDocuments[];
    rechercheType: string;
    videTitre: string;
    /** « une facture », « un BL ». */
    singulier: string;
    /** Position dans la barre de l'aperçu : « Facture 1 sur 12 ». */
    section: string;
    /** Pied de la fiche quand rien ne suit : « Aucune autre facture dans cette liste ». */
    sansSuite: string;
    /** Rappel de touche du pied de liste, après J K. */
    pied?: 'chercher' | 'valider';
  } = $props();

  let filtreV = $derived(page.url.searchParams.get('filtre') ?? filtres[0].v);
  let filtre = $derived(filtres.find((f) => f.v === filtreV) ?? filtres[0]);
  let docId = $derived(Number(page.url.searchParams.get('doc')) || null);

  const lst = new Liste<LigneDocument>((p) => `/api/documents${requete({ societe: societe.id, ...filtre.params, page: p })}`);

  $effect(() => {
    void societe.id;
    void filtre;
    untrack(() => void lst.lire());
    return sonder(async () => {
      if (document.visibilityState === 'visible') await lst.lire(true);
    }, 60_000);
  });

  let groupes = $derived(parJour(lst.elements, (d) => d.date_mail));

  function lien(id: number | null, f = filtreV) {
    return `${chemin}${requete({ filtre: f === filtres[0].v ? '' : f, doc: id })}`;
  }

  /** Où en est le document, en deux mots. */
  function statut(d: LigneDocument): { texte: string; ton: Ton } | null {
    if (d.etat !== 'lu') return { texte: libelleEtat(d.etat), ton: tonEtat(d.etat) };
    if (d.type === 'bl') return d.verifie ? { texte: 'Validé', ton: 'ok' } : { texte: 'À valider', ton: 'alerte' };
    if (d.statut_depot === 'depose') return { texte: 'Envoyée', ton: 'ok' };
    if (d.statut_depot) return { texte: libelleStatutEnvoi(d.statut_depot), ton: tonStatutEnvoi(d.statut_depot) };
    return d.verifie ? { texte: 'Validée', ton: 'ok' } : { texte: 'À valider', ton: 'alerte' };
  }

  let annonce = $state('');
  let annoncePour = 0;
  $effect(() => {
    if (docId !== annoncePour) annonce = '';
  });
  /** Validé ou écarté : la liste est relue et le document suivant de la liste s'ouvre. */
  async function apres(r: ResultatFiche) {
    const i = lst.elements.findIndex((d) => d.id === r.id);
    const suivant = lst.elements[i + 1]?.id ?? null;
    void rafraichirCompteurs();
    await lst.lire(true);
    annonce = r.info;
    annoncePour = suivant ?? 0;
    await goto(lien(suivant), { replace: true });
  }

  const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
  const heureDe = (iso: string | null) => (iso ? heure.format(new Date(iso)) : '');

  // BL sans facture liée : étiquette « En attente de facture ».
  let blAttente = $state(new Set<number>());
  $effect(() => {
    const ids = lst.elements.filter((d) => d.type === 'bl' && d.etat === 'lu').map((d) => d.id);
    if (!ids.length) {
      blAttente = new Set();
      return;
    }
    void blSansFacture(ids).then((r) => (blAttente = r));
  });

  let listeEl = $state<HTMLElement>();
  let indice = $derived(lst.elements.findIndex((d) => d.id === docId));
  let position = $derived(indice >= 0 ? { section, n: indice + 1, total: lst.total } : null);
  let ensuite = $derived.by(() => {
    const s = indice >= 0 ? lst.elements[indice + 1] : undefined;
    return s ? nomCourtDocument({ fournisseur: s.fournisseur ?? s.fournisseur_nom, type: s.type, numero: s.numero }) : undefined;
  });
  function deplacerDe(sens: 1 | -1) {
    const i = deplacer(indice, sens, lst.elements.length);
    if (i < 0) return;
    const id = lst.elements[i].id;
    void goto(lien(id), { replace: true }).then(() => montrer(listeEl, `[data-id="${id}"]`));
  }
  $effect(() => {
    const carte: Record<string, () => void> = { j: () => deplacerDe(1), k: () => deplacerDe(-1) };
    if (docId) carte.Escape = () => void goto(lien(null));
    return raccourcis(carte, { dansLesChamps: [] });
  });
</script>

<svelte:head><title>{titre} · GMFlow</title></svelte:head>

<Volets ouvert={!!docId} libelleListe={titre}>
  {#snippet liste()}
    <div class="liste-h">
      <h1>{titre} <span class="num">{lst.total}</span></h1>
      <span class="sp"></span>
      <a class="btn-icone" href="/recherche?type={rechercheType}" aria-label="Chercher dans {titre.toLowerCase()}" title="Chercher dans {titre.toLowerCase()}"><Icone nom="chercher" /></a>
    </div>
    <div class="puces" role="group" aria-label="Filtre">
      {#each filtres as f (f.v)}
        <a class="btn sm" href={lien(null, f.v)} aria-current={filtreV === f.v ? "true" : undefined} data-sveltekit-replacestate>{f.l}</a>
      {/each}
    </div>
    <div class="liste-s" bind:this={listeEl}>
      {#if lst.erreur}
        <Bandeau ton="erreur" titre="Liste non chargée.">
          {lst.erreur}
          {#snippet action()}<button type="button" class="btn sm" onclick={() => lst.lire()}>Réessayer</button>{/snippet}
        </Bandeau>
      {/if}
      {#if !lst.charge}
        <Chargement texte="Chargement" />
      {:else if !lst.elements.length}
        <Vide compact icone="fichier" titre={videTitre} texte="Rien avec ce filtre pour l’instant." />
      {:else}
        {#each groupes as g (g.titre)}
          <div class="jour-h">{g.titre} <span class="num">{g.elements.length}</span></div>
          {#each g.elements as d (d.id)}
            {@const s = statut(d)}
            {@const blSansMontant = d.type === 'bl' && d.montant_ttc === null}
            <a class="ligne" class:cur={docId === d.id} aria-current={docId === d.id ? 'true' : undefined} href={lien(d.id)} data-id={d.id}>
              <div class="a">{d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non reconnu'}</div>
              {#if blSansMontant}
                <div class="r doc-heure">{heureDe(d.date_mail)}</div>
              {:else}
                <div class="r">{#if d.montant_ttc !== null}<Montant valeur={d.montant_ttc} avoir={d.type === 'avoir'} />{/if}</div>
              {/if}
              <div class="b"><span class="txt">{typeNumero(d.type, d.numero)}{d.societe ? `, ${nomCourtSociete(d.societe)}` : ''}</span></div>
              {#if !blSansMontant}
                <div class="r2">{d.type === 'bl' ? heureDe(d.date_mail) : dateFr(d.date_document, true)}</div>
              {/if}
              {#if s || d.articles_ecart || (d.remise_ht && d.remise_ht > 0.005) || blAttente.has(d.id)}
                <div class="c">
                  {#if s}<Etiquette ton={s.ton}>{s.texte}</Etiquette>{/if}
                  {#if blAttente.has(d.id)}<Etiquette ton="info">En attente de facture</Etiquette>{/if}
                  {#if d.articles_ecart}<Etiquette ton="alerte" title="Articles manquants, en trop ou à un autre prix entre facture et BL">{d.articles_ecart} écart{d.articles_ecart > 1 ? 's' : ''} BL</Etiquette>{/if}
                  {#if d.remise_ht && d.remise_ht > 0.005}<Etiquette ton="info" title="Remise déduite sur la facture : le total ne correspond pas aux BL">Remise {formatMontant(d.remise_ht)}</Etiquette>{/if}
                </div>
              {/if}
            </a>
          {/each}
        {/each}
        {#if lst.reste > 0}
          <button type="button" class="plus" disabled={lst.enCours} onclick={() => lst.plus()}>
            {lst.enCours ? 'Chargement…' : `Afficher les plus anciens (${lst.reste})`}
          </button>
        {/if}
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      {#if pied === 'valider'}<span><Kbd t="Enter" /> valider</span>{:else}<span><Kbd t="/" /> chercher</span>{/if}
    </div>
  {/snippet}

  {#snippet detail()}
    {#if docId}
      <DocFiche
        id={docId}
        retour={lien(null)}
        retourLibelle={titre}
        {position}
        {ensuite}
        {sansSuite}
        {annonce}
        onfait={apres}
        onchange={() => lst.lire(true)}
        onprecedent={indice > 0 ? () => deplacerDe(-1) : undefined}
        onsuivant={indice >= 0 && indice < lst.elements.length - 1 ? () => deplacerDe(1) : undefined}
      />
    {/if}
  {/snippet}

  {#snippet vide()}
    <Vide icone="fichier" titre="Choisissez {singulier}" texte="Cliquez sur une ligne pour voir le document, ses montants et le mail d’origine." />
  {/snippet}
</Volets>

<style>
  .txt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .doc-heure {
    font-size: 14px;
    font-weight: 400;
    color: var(--text2);
  }
</style>
