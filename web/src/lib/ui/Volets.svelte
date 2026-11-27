<script lang="ts" module>
  // Liste repliée ou non, choisie à la main (par page) : vaut jusqu'au rechargement de la page.
  // Sans choix, la liste se replie d'elle-même quand une pièce est ouverte sur un écran de moins de 1600 px.
  const replisChoisis = $state<Record<string, boolean>>({});
</script>

<script lang="ts">
  // Volets d'une page de travail : la file à gauche, l'élément ouvert à droite (fiche puis aperçu pour une pièce).
  // La page ne défile jamais : chaque volet défile dans sa colonne (`.liste-s`, la fiche, l'aperçu).
  //   <Volets ouvert={!!courant}>
  //     {#snippet liste()} <div class="liste-h">…</div><div class="liste-s">…</div><div class="liste-f">…</div> {/snippet}
  //     {#snippet detail()} <div class="volet-h">…</div><div class="volet-b">…</div><BarreAction …/> {/snippet}
  //   </Volets>
  // `tableau` : rien d'ouvert, la liste prend toute la largeur (tableau des pièces, écran A) et `vide` n'est pas montré.
  // Sans `tableau` ni `detail`, le volet de droite montre `vide` ou rien.
  // Sur téléphone, un seul volet à la fois : la liste, ou le détail quand `ouvert`.
  import type { Snippet } from 'svelte';
  import { largeurGardee } from '#lib/largeurs.ts';
  import Icone from './Icone.svelte';
  import Poignee from './Poignee.svelte';

  let {
    ouvert = false,
    tableau = false,
    largeur,
    libelleListe = 'Liste',
    liste,
    detail,
    vide
  }: {
    ouvert?: boolean;
    /** Rien d'ouvert : la liste en tableau pleine largeur (ordinateur). */
    tableau?: boolean;
    /** Largeur du volet liste, en px (320 par défaut). */
    largeur?: number;
    libelleListe?: string;
    liste: Snippet;
    detail?: Snippet;
    vide?: Snippet;
  } = $props();

  // La liste s'élargit à la souris (poignée sur sa bordure droite), largeur gardée par page.
  // Clé `volet.` (avant : `liste.`) : les largeurs choisies pour l'ancienne disposition ne reviennent pas.
  let cle = $derived(`volet.${libelleListe}`);
  let defaut = $derived(largeur ?? 320);
  let larg = $state(320);
  let choisie = $state(false);
  $effect.pre(() => {
    larg = largeurGardee(cle, defaut);
    choisie = largeurGardee(cle, 0) > 0;
  });

  // Écran de 1600 px et plus : la liste reste à côté de la pièce ouverte ; en dessous, elle se replie.
  let large = $state(true);
  $effect(() => {
    const mq = window.matchMedia('(min-width: 1600px)');
    large = mq.matches;
    const suivre = (e: MediaQueryListEvent) => (large = e.matches);
    mq.addEventListener('change', suivre);
    return () => mq.removeEventListener('change', suivre);
  });
  let plein = $derived(tableau && !ouvert);
  let replie = $derived(ouvert && !!detail && (replisChoisis[libelleListe] ?? !large));
  function replier(v: boolean) {
    replisChoisis[libelleListe] = v;
  }
</script>

<div class="volets" class:ouvert class:plein class:replie style:--larg-liste={largeur ? `${largeur}px` : undefined}>
  {#if replie}
    <div class="rail-liste">
      <button type="button" class="btn-icone" aria-label="Montrer la liste ({libelleListe})" title="Montrer la liste (J, K : pièce suivante, précédente)" onclick={() => replier(false)}>
        <Icone nom="droite" />
      </button>
      <span class="rail-t" aria-hidden="true">{libelleListe}</span>
    </div>
  {/if}
  <aside class="volet-liste" aria-label={libelleListe} class:choisie={choisie && !plein} style:--larg-choisie="{larg}px">
    {#if ouvert && detail}
      <button type="button" class="btn-icone replier-liste" aria-label="Replier la liste" title="Replier la liste (J, K marchent toujours)" onclick={() => replier(true)}>
        <Icone nom="gauche" />
      </button>
    {/if}
    {@render liste()}
  </aside>
  {#if !plein && !replie}
    <Poignee bind:largeur={larg} bind:choisie {defaut} min={240} max={720} cote="gauche" {cle} libelle="Largeur de la liste" />
  {/if}
  <section class="volet">
    {#if ouvert && detail}{@render detail()}{:else if vide}{@render vide()}{/if}
  </section>
</div>
