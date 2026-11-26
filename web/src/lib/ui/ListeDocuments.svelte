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
  import { deplacer, fleche, montrer, raccourcis, type Gestionnaire } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { libelleEtat, libelleStatutEnvoi, nomCourtSociete, tonEtat, tonStatutEnvoi, type Ton } from '#lib/libelles.ts';
  import { filtreGarde, garderFiltre } from '#lib/filtres.ts';
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

  let docId = $derived(Number(page.url.searchParams.get('doc')) || null);
  // Le dernier filtre choisi est gardé (par navigateur) et revient à l'ouverture de la liste.
  // Un lien vers un document précis (?doc=) ne l'applique pas : le document peut être hors de ce filtre.
  let memoire = $state<string | null>(untrack(() => filtreGarde(chemin)));
  let filtreUrl = $derived(page.url.searchParams.get('filtre'));
  $effect(() => {
    if (filtreUrl && filtres.some((f) => f.v === filtreUrl) && filtreUrl !== memoire) {
      memoire = filtreUrl;
      garderFiltre(chemin, filtreUrl === filtres[0].v ? null : filtreUrl);
    }
  });
  let memoireValide = $derived(memoire && filtres.some((f) => f.v === memoire) ? memoire : null);
  let filtreV = $derived(filtreUrl ?? (docId ? null : memoireValide) ?? filtres[0].v);
  let filtre = $derived(filtres.find((f) => f.v === filtreV) ?? filtres[0]);

  const lst = new Liste<LigneDocument>((p) => `/api/documents${requete({ societe: societe.id, ...filtre.params, page: p })}`);

  $effect(() => {
    void societe.id;
    void filtre;
    untrack(() => void lst.lire());
    return sonder(async () => {
      if (document.visibilityState !== 'visible') return;
      // Ne pas faire bouger les lignes sous la souris ou le focus : on relira au prochain passage.
      if (listeEl && (listeEl.matches(':hover') || listeEl.contains(document.activeElement))) return;
      const avant = lst.total;
      await lst.lire(true);
      if (lst.total !== avant) majListe = `Liste mise à jour : ${lst.total} ${lst.total > 1 ? 'éléments' : 'élément'}`;
    }, 60_000);
  });
  /** Annonce aux lecteurs d'écran, quand la liste change toute seule. */
  let majListe = $state('');

  let groupes = $derived(parJour(lst.elements, (d) => d.date_mail));

  function lien(id: number | null, f = filtreV) {
    // Le filtre d'origine reste implicite, sauf si un autre est gardé : il faut alors le nommer pour le retrouver.
    const implicite = f === filtres[0].v && !(memoireValide && memoireValide !== filtres[0].v);
    return `${chemin}${requete({ filtre: implicite ? '' : f, doc: id })}`;
  }

  /** Où en est le document, en deux mots. */
  function statut(d: LigneDocument): { texte: string; ton: Ton } | null {
    if (d.etat !== 'lu') return { texte: libelleEtat(d.etat), ton: tonEtat(d.etat) };
    if (d.type === 'bl') return d.verifie ? { texte: 'Validé', ton: 'ok' } : { texte: 'À contrôler', ton: 'alerte' };
    const e = d.type === 'avoir' ? '' : 'e';
    if (d.statut_depot === 'depose') return { texte: `Envoyé${e}`, ton: 'ok' };
    if (d.statut_depot === 'pret' && !e) return { texte: 'Validé', ton: tonStatutEnvoi('pret') };
    if (d.statut_depot) return { texte: libelleStatutEnvoi(d.statut_depot), ton: tonStatutEnvoi(d.statut_depot) };
    return d.verifie ? { texte: `Validé${e}`, ton: 'ok' } : { texte: 'À contrôler', ton: 'alerte' };
  }

  /** Étiquettes d'une ligne, la plus utile d'abord : l'état, puis ce qui demande un regard. */
  const MAX_ETIQUETTES = 2;
  function etiquettesDe(d: LigneDocument, s: { texte: string; ton: Ton } | null) {
    const l: { texte: string; ton: Ton; title?: string }[] = [];
    if (s) l.push({ texte: s.texte, ton: s.ton });
    if (d.articles_ecart)
      l.push({
        texte: `${d.articles_ecart} écart${d.articles_ecart > 1 ? 's' : ''} BL`,
        ton: 'alerte',
        title: 'Articles manquants, en trop ou à un autre prix entre facture et BL'
      });
    if (blAttente.has(d.id)) l.push({ texte: 'En attente de facture', ton: 'info' });
    if (d.remise_ht && d.remise_ht > 0.005)
      l.push({ texte: `Remise ${formatMontant(d.remise_ht)}`, ton: 'info', title: 'Remise déduite sur la facture : le total ne correspond pas aux BL' });
    return l;
  }

  /** Nom lu par les lecteurs d'écran : tout ce que la ligne montre, en une phrase. */
  function nomLigne(d: LigneDocument, e: { texte: string }[]): string {
    const p = [
      d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non reconnu',
      typeNumero(d.type, d.numero) + (d.societe ? `, ${nomCourtSociete(d.societe)}` : ''),
      d.montant_ttc !== null ? formatMontant(d.montant_ttc) : '',
      ...e.map((x) => x.texte)
    ];
    return p.filter(Boolean).join(', ');
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
    const carte: Record<string, Gestionnaire> = {
      j: () => deplacerDe(1),
      k: () => deplacerDe(-1),
      ArrowDown: fleche(() => deplacerDe(1)),
      ArrowUp: fleche(() => deplacerDe(-1))
    };
    if (docId) carte.Escape = () => void goto(lien(null));
    else
      carte.Enter = () => {
        if (!lst.elements[0]) return false;
        void goto(lien(lst.elements[0].id));
      };
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
    <div class="sr-seul" role="status" aria-live="polite">{majListe}</div>
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
            {@const etiquettes = etiquettesDe(d, s)}
            <a class="ligne" aria-label={nomLigne(d, etiquettes)} class:cur={docId === d.id} aria-current={docId === d.id ? 'true' : undefined} href={lien(d.id)} data-id={d.id}>
              <div class="a">{d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non reconnu'}</div>
              {#if blSansMontant}
                <div class="r doc-heure">{heureDe(d.date_mail)}</div>
              {:else}
                <div class="r">{#if d.montant_ttc !== null}<Montant valeur={d.montant_ttc} avoir={d.type === 'avoir'} />{/if}</div>
              {/if}
              <div class="b">
                <span class="num-d">{typeNumero(d.type, d.numero)}{d.societe ? ',' : ''}</span>
                {#if d.societe}<span class="txt">{nomCourtSociete(d.societe)}</span>{/if}
              </div>
              {#if !blSansMontant}
                <div class="r2">{d.type === 'bl' ? heureDe(d.date_mail) : dateFr(d.date_document, true)}</div>
              {/if}
              {#if etiquettes.length}
                <div class="c">
                  {#each etiquettes.slice(0, MAX_ETIQUETTES) as e (e.texte)}<Etiquette ton={e.ton} title={e.title}>{e.texte}</Etiquette>{/each}
                  {#if etiquettes.length > MAX_ETIQUETTES}
                    {@const reste = etiquettes.slice(MAX_ETIQUETTES)}
                    <Etiquette ton="pale" title={reste.map((e) => e.texte).join(', ')}>+{reste.length}</Etiquette>
                  {/if}
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
      <span><Kbd t="?" /> aide</span>
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
  .num-d {
    flex: none;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
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
