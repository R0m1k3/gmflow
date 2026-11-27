<script lang="ts">
  // Onglet Mails : tous les mails reçus, les plus récents en haut, groupés par jour.
  // Filtre par catégorie (puces), fiche du mail à droite (?mail=ID). J/K, Échap.
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { requete, sonder } from '#lib/api.ts';
  import { deplacer, fleche, montrer, raccourcis, type Gestionnaire } from '#lib/clavier.ts';
  import { libelleStatutReponse, tonStatutReponse } from '#lib/libelles.ts';
  import { Liste, parJour } from '#lib/liste.svelte.ts';
  import { lireTri, parametresTri, type ColonneTri } from '#lib/tri.ts';
  import EnTetesTri from '#lib/ui/EnTetesTri.svelte';
  import ErreurAction from '#lib/ui/ErreurAction.svelte';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Categorie, LigneMail } from '#lib/types.ts';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import MailFiche, { dateMail, nomLisible } from '#lib/ui/MailFiche.svelte';
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

  /** Colonnes triables (liste blanche du serveur : `TRI_MAILS`). */
  const TRI: ColonneTri[] = [
    { cle: 'date', libelle: 'Date', croissant: false, genre: 'date' },
    { cle: 'expediteur', libelle: 'Expéditeur', croissant: true, genre: 'texte' },
    { cle: 'objet', libelle: 'Objet', croissant: true, genre: 'texte' }
  ];
  let tri = $derived(lireTri(page.url.searchParams, TRI));
  let paramsTri = $derived(parametresTri(tri));
  let triCle = $derived(JSON.stringify(paramsTri));
  const lst = new Liste<LigneMail>((p) => `/api/mails${requete({ societe: societe.id, categorie: filtre, ...paramsTri, page: p })}`);

  $effect(() => {
    void societe.id;
    void filtre;
    void triCle;
    untrack(() => void lst.lire());
    return sonder(async () => {
      if (document.visibilityState !== 'visible') return;
      // Ne pas faire bouger les lignes sous la souris ou le focus : on relira au prochain passage.
      if (listeEl && (listeEl.matches(':hover') || listeEl.contains(document.activeElement))) return;
      await lst.lire(true);
    }, 60_000);
  });

  /** Catégorie courte de la tuile (maquette : « Information », « Notification », « Pièce jointe »). */
  const CATEGORIE_TUILE: Record<Categorie, string> = {
    question: 'Question',
    information: 'Information',
    document_seul: 'Pièce jointe',
    publicite: 'Publicité',
    notification: 'Notification'
  };

  // Par jour quand la liste est triée par date ; sinon une seule suite.
  let groupes = $derived(tri.cle === 'date' ? parJour(lst.elements, (m) => m.date_mail) : [{ titre: '', elements: lst.elements }]);

  function lien(id: number | null, f = filtre) {
    return `/mails${requete({ filtre: f === 'hors_publicite' ? '' : f, mail: id, ...paramsTri })}`;
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
    const carte: Record<string, Gestionnaire> = {
      j: () => deplacerDe(1),
      k: () => deplacerDe(-1),
      ArrowDown: fleche(() => deplacerDe(1)),
      ArrowUp: fleche(() => deplacerDe(-1))
    };
    if (mailId) carte.Escape = () => void goto(lien(null));
    else
      carte.Enter = () => {
        if (!lst.elements[0]) return false;
        void goto(lien(lst.elements[0].id));
      };
    return raccourcis(carte, { dansLesChamps: [] });
  });

  /** Après un envoi ou un classement : la liste est relue, le mail reste ouvert. */
  async function apresMail() {
    await lst.lire(true);
  }
</script>

<svelte:head><title>Mails · GMFlow</title></svelte:head>

<Volets ouvert={!!mailId} libelleListe="Mails reçus" largeur={360}>
  {#snippet liste()}
    <div class="liste-h">
      <h1>Mails <span class="num">{lst.total}</span></h1>
      <a class="btn-icone chercher" href="/recherche?type=mail" title="Chercher dans les mails" aria-label="Chercher dans les mails"><Icone nom="chercher" /></a>
    </div>
    <div class="puces" role="group" aria-label="Catégorie">
      {#each FILTRES as f (f.v)}
        <a class="btn sm" href={lien(null, f.v)} aria-current={filtre === f.v ? 'true' : undefined} data-sveltekit-replacestate>{f.l}</a>
      {/each}
    </div>
    <EnTetesTri colonnes={TRI} {tri} retirer={['mail']} />
    <div class="liste-s" bind:this={listeEl}>
      {#if lst.erreur}
        <ErreurAction erreur={lst.erreurBrute} titre="Liste non chargée." onreessayer={() => lst.lire()} />
      {/if}
      {#if !lst.charge}
        <Chargement texte="Chargement des mails" />
      {:else if !lst.elements.length}
        <Vide compact icone="mail" titre="Aucun mail" texte="Rien dans cette catégorie pour l’instant." />
      {:else}
        {#each groupes as g (g.titre)}
          <div class="groupe">
            {#if g.titre}<h2 class="jour-h">{g.titre} <span class="num">{g.elements.length}</span></h2>{/if}
            {#each g.elements as m (m.id)}
              <a
                class="ligne tuile"
                class:pub={m.categorie === 'publicite'}
                aria-current={mailId === m.id ? 'true' : undefined}
                href={lien(m.id)}
                data-id={m.id}
              >
                <span class="t1">
                  <span class="de">{nomLisible(m.expediteur) || '(expéditeur inconnu)'}</span>
                  <span class="h num">{dateMail(m.date_mail, false)}</span>
                </span>
                <span class="t2">
                  {#if m.nb_pieces}<span class="pj" title="{m.nb_pieces} pièce{m.nb_pieces > 1 ? 's' : ''} jointe{m.nb_pieces > 1 ? 's' : ''}"><Icone nom="trombone" /></span>{/if}
                  <span class="objet">{m.objet || '(sans objet)'}</span>
                </span>
                <span class="t3">
                  {#if m.categorie}<span>{CATEGORIE_TUILE[m.categorie] ?? m.categorie}</span>{:else}<span>Pas encore trié</span>{/if}
                  {#if m.statut_reponse === 'a_repondre'}
                    <Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>
                  {/if}
                </span>
              </a>
            {/each}
          </div>
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
  .chercher {
    margin-left: auto;
    align-self: center;
  }
  .groupe {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: var(--e4);
  }
  .groupe .jour-h {
    margin: 0;
    padding: 0 4px 4px;
  }
  .ligne.tuile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 16px;
    min-width: 0;
  }
  .t1 {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--e3);
    min-width: 0;
  }
  .de {
    font-size: 16px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .h {
    flex: none;
    font-size: 14px;
    color: var(--text2);
  }
  .t2 {
    display: flex;
    align-items: center;
    gap: var(--e2);
    min-width: 0;
    font-size: 15px;
    color: var(--text2);
  }
  .objet {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .pj {
    flex: none;
    display: inline-flex;
    color: var(--text2);
  }
  .pj :global(svg) {
    width: 16px;
    height: 16px;
  }
  .t3 {
    display: flex;
    align-items: center;
    gap: var(--e2);
    font-size: 13px;
    color: var(--muted);
  }
  .ligne.pub .de {
    font-weight: 400;
    color: var(--muted);
  }
</style>
