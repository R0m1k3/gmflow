<script lang="ts">
  // Onglet Mails : tous les mails reçus, les plus récents en haut, groupés par jour.
  // Filtre par catégorie (puces), fiche du mail à droite (?mail=ID). J/K, Échap.
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { requete, sonder } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { libelleCategorie, libelleStatutReponse, tonStatutReponse } from '#lib/libelles.ts';
  import { Liste, parJour } from '#lib/liste.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { LigneMail } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import MailFiche, { dateMail, nomExpediteur } from '#lib/ui/MailFiche.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import Volets from '#lib/ui/Volets.svelte';

  const FILTRES = [
    { v: 'hors_publicite', l: 'Tous sauf publicités' },
    { v: 'question', l: 'Questions' },
    { v: 'document_seul', l: 'Documents' },
    { v: 'information', l: 'Informations' },
    { v: 'notification', l: 'Notifications' },
    { v: 'publicite', l: 'Publicités' },
    { v: '', l: 'Tout' }
  ] as const;

  let filtre = $derived(page.url.searchParams.get('filtre') ?? 'hors_publicite');
  let mailId = $derived(Number(page.url.searchParams.get('mail')) || null);

  const lst = new Liste<LigneMail>((p) => `/api/mails${requete({ societe: societe.id, categorie: filtre, page: p })}`);

  $effect(() => {
    void societe.id;
    void filtre;
    untrack(() => void lst.lire());
    return sonder(async () => {
      if (document.visibilityState === 'visible') await lst.lire(true);
    }, 60_000);
  });

  let groupes = $derived(parJour(lst.elements, (m) => m.date_mail));

  function lien(id: number | null, f = filtre) {
    return `/mails${requete({ filtre: f === 'hors_publicite' ? '' : f, mail: id })}`;
  }

  let listeEl = $state<HTMLElement>();
  let indice = $derived(lst.elements.findIndex((m) => m.id === mailId));
  function deplacerDe(sens: 1 | -1) {
    const i = deplacer(indice, sens, lst.elements.length);
    if (i < 0) return;
    const id = lst.elements[i].id;
    void goto(lien(id), { replace: true }).then(() => montrer(listeEl, `[data-id="${id}"]`));
  }
  $effect(() => {
    const carte: Record<string, () => void> = { j: () => deplacerDe(1), k: () => deplacerDe(-1) };
    if (mailId) carte.Escape = () => void goto(lien(null));
    return raccourcis(carte, { dansLesChamps: [] });
  });

  /** Après un envoi ou un classement : la liste est relue, le mail reste ouvert. */
  async function apresMail() {
    await lst.lire(true);
  }
</script>

<svelte:head><title>Mails · GMFlow</title></svelte:head>

<Volets ouvert={!!mailId} libelleListe="Mails reçus" largeur={460}>
  {#snippet liste()}
    <div class="liste-h">
      <h1>Mails</h1>
      <span class="num">{lst.total}</span>
      <span class="sp"></span>
      <a class="btn sm fantome" href="/recherche?type=mail"><Icone nom="chercher" />Chercher</a>
    </div>
    <div class="puces" role="group" aria-label="Catégorie">
      {#each FILTRES as f (f.v)}
        <a class="btn sm" href={lien(null, f.v)} aria-current={filtre === f.v ? "true" : undefined} data-sveltekit-replacestate>{f.l}</a>
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
        <Chargement texte="Chargement des mails" />
      {:else if !lst.elements.length}
        <Vide compact icone="mail" titre="Aucun mail" texte="Rien dans cette catégorie pour l’instant." />
      {:else}
        {#each groupes as g (g.titre)}
          <div class="jour-h">{g.titre} <span class="num muted">{g.elements.length}</span></div>
          {#each g.elements as m (m.id)}
            <a
              class="ligne"
              class:cur={mailId === m.id}
              class:pub={m.categorie === 'publicite'}
              aria-current={mailId === m.id ? 'true' : undefined}
              href={lien(m.id)}
              data-id={m.id}
            >
              <div class="a">{nomExpediteur(m.expediteur) || '(expéditeur inconnu)'}</div>
              <div class="r">
                {#if m.statut_reponse === 'a_repondre'}
                  <Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>
                {:else if m.categorie}
                  <span class="muted petit">{libelleCategorie(m.categorie)}</span>
                {/if}
              </div>
              <div class="b">
                {#if m.nb_pieces}<span class="pj" title="{m.nb_pieces} pièce{m.nb_pieces > 1 ? 's' : ''} jointe{m.nb_pieces > 1 ? 's' : ''}"><Icone nom="trombone" /></span>{/if}
                {m.objet || '(sans objet)'}
              </div>
              <div class="r2"><span class="num">{dateMail(m.date_mail, false)}</span></div>
            </a>
          {/each}
        {/each}
        {#if lst.reste > 0}
          <button type="button" class="plus" disabled={lst.enCours} onclick={() => lst.plus()}>
            {lst.enCours ? 'Chargement…' : `Afficher les mails plus anciens (${lst.reste})`}
          </button>
        {/if}
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      <span><Kbd t="/" /> chercher</span>
    </div>
  {/snippet}

  {#snippet detail()}
    {#if mailId}<MailFiche id={mailId} volet retour={{ href: lien(null), libelle: 'Mails' }} ouvrir={apresMail} />{/if}
  {/snippet}

  {#snippet vide()}
    <Vide icone="mail" titre="Choisissez un mail" texte="Cliquez sur un mail de la liste pour le lire ici, avec ses pièces jointes." />
  {/snippet}
</Volets>

<style>
  .pj {
    display: inline-flex;
    vertical-align: -3px;
    margin-right: 4px;
    color: var(--muted);
  }
  .pj :global(svg) {
    width: 14px;
    height: 14px;
  }
</style>
