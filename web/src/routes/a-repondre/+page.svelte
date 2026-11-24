<script lang="ts">
  // Onglet À répondre : les mails qui attendent une réponse (GET /api/a-repondre), urgents
  // d'abord, puis les plus anciens. Fiche du mail et brouillon à droite (?mail=ID).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { requete, sonder } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { libelleBrouillon, libellePriorite, nomCourtSociete } from '#lib/libelles.ts';
  import { Liste } from '#lib/liste.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { LigneARepondre } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import MailFiche, { dateMail, nomLisible } from '#lib/ui/MailFiche.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import Volets from '#lib/ui/Volets.svelte';

  let mailId = $derived(Number(page.url.searchParams.get('mail')) || null);
  const lst = new Liste<LigneARepondre>((p) => `/api/a-repondre${requete({ societe: societe.id, page: p })}`);

  $effect(() => {
    void societe.id;
    untrack(() => void lst.lire());
    return sonder(async () => {
      if (document.visibilityState === 'visible') await lst.lire(true);
    }, 60_000);
  });

  const lien = (id: number | null) => `/a-repondre${requete({ mail: id })}`;

  /** Après un envoi ou un classement : la liste est relue, puis le mail suivant (ou la liste). */
  async function apresMail(suivant: number | null) {
    await lst.lire(true);
    await goto(lien(suivant), { replace: true });
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
    else
      carte.Enter = () => {
        if (lst.elements[0]) void goto(lien(lst.elements[0].id));
      };
    return raccourcis(carte, { dansLesChamps: [] });
  });
</script>

<svelte:head><title>À répondre · GMFlow</title></svelte:head>

<Volets ouvert={!!mailId} libelleListe="Mails à répondre" largeur={360}>
  {#snippet liste()}
    <div class="liste-h">
      <h1>À répondre <span class="num">{lst.total}</span></h1>
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
        <Vide compact icone="coche" titre="Aucune réponse en attente" texte="Les questions reçues par mail arrivent ici." />
      {:else}
        <div class="groupe">
          {#each lst.elements as m (m.id)}
            <a class="ligne tuile" aria-current={mailId === m.id ? 'true' : undefined} href={lien(m.id)} data-id={m.id}>
              <span class="t1">
                <span class="de">{nomLisible(m.expediteur) || '(expéditeur inconnu)'}</span>
                <span class="h num">{dateMail(m.date_mail, false)}</span>
              </span>
              <span class="t2"><span class="objet">{m.objet || '(sans objet)'}</span></span>
              {#if m.resume}<span class="resume">{m.resume}</span>{/if}
              <span class="t3">
                {#if libellePriorite(m.priorite)}<Etiquette ton="erreur">{libellePriorite(m.priorite)}</Etiquette>{/if}
                {#if m.brouillon}<span class="brouillon"><Icone nom="crayon" />{libelleBrouillon(m.brouillon)}</span>{/if}
                <span>{m.structures.length ? m.structures.map((s: string) => nomCourtSociete(s)).join(', ') : 'Question'}</span>
              </span>
            </a>
          {/each}
        </div>
        {#if lst.reste > 0}
          <button type="button" class="plus" disabled={lst.enCours} onclick={() => lst.plus()}>
            {lst.enCours ? 'Chargement…' : `Afficher la suite (${lst.reste})`}
          </button>
        {/if}
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      <span>{#if mailId}<Kbd t={['Ctrl', 'Enter']} /> envoyer{:else}<Kbd t="Enter" /> commencer{/if}</span>
    </div>
  {/snippet}

  {#snippet detail()}
    {#if mailId}<MailFiche id={mailId} volet retour={{ href: '/a-repondre', libelle: 'À répondre' }} ouvrir={apresMail} />{/if}
  {/snippet}

  {#snippet vide()}
    <Vide icone="envoyer" titre="Choisissez un mail" texte="La question, le résumé et la réponse proposée s’ouvrent ici." />
  {/snippet}
</Volets>

<style>
  .groupe {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: var(--e4);
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
  .resume {
    font-size: 14px;
    line-height: 1.4;
    color: var(--muted);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .t3 {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e2);
    font-size: 13px;
    color: var(--muted);
  }
  .brouillon {
    display: inline-flex;
    align-items: center;
    gap: var(--e1);
    color: var(--text2);
  }
  .brouillon :global(svg) {
    width: 14px;
    height: 14px;
  }
</style>
