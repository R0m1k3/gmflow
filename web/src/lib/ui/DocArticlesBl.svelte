<script lang="ts">
  // Articles d'un BL face à sa ou ses factures (proposition A, tableau ligne à ligne) : PU du BL,
  // PU facturé et écart de chaque ligne, d'après les écarts d'articles du rapprochement (retrouvés
  // par référence normalisée, sinon par désignation). Lignes en écart d'abord, 8 au plus ; les
  // articles facturés absents de tous les BL à la fin. Sans facture liée : Article, Qté, PU BL.
  import type { EcartLigne, LigneArticle } from '#lib/types.ts';
  import { ecartDeLigne } from './addition.ts';
  import { formatMontant } from './Montant.svelte';

  let {
    lignes,
    ecarts,
    factureLiee,
    plusieursBl
  }: {
    lignes: LigneArticle[];
    /** Écarts de toutes les factures liées, mis bout à bout. */
    ecarts: EcartLigne[];
    factureLiee: boolean;
    /** Une facture liée a au moins 2 BL : les quantités sont comparées sur l'ensemble. */
    plusieursBl: boolean;
  } = $props();

  const MAX = 8;
  let tout = $state(false);

  type Rangee = {
    cle: string;
    article: string;
    reference: string | null;
    qte: string;
    puBl: number | null;
    puFacture: number | null;
    ecart: string;
    enEcart: boolean;
  };

  const qteFr = (v: number | null) => (v === null ? '—' : v.toLocaleString('fr-FR', { maximumFractionDigits: 3 }));
  const genre = (e: EcartLigne) => e.genre ?? (e.qte_facture > e.qte_bl ? 'manquant' : 'en_trop');
  const signe = (v: number) => `${v > 0 ? '+' : ''}${formatMontant(v, '')}`;

  function puLigne(l: LigneArticle): number | null {
    if (l.prix_unitaire_ht !== null) return l.prix_unitaire_ht;
    return l.montant_ht !== null && l.quantite ? l.montant_ht / l.quantite : null;
  }

  function rangee(l: LigneArticle): Rangee {
    const r: Rangee = {
      cle: `l${l.rang}`,
      article: l.designation ?? l.reference ?? '—',
      reference: l.designation ? l.reference : null,
      qte: `${qteFr(l.quantite)}${l.unite ? ` ${l.unite}` : ''}`,
      puBl: puLigne(l),
      puFacture: null,
      ecart: '—',
      enEcart: false
    };
    const e = factureLiee ? ecartDeLigne(l, ecarts) : null;
    if (!e) return r;
    const g = genre(e);
    const pf = e.prix_facture ?? null;
    if (g === 'prix') {
      // Le serveur compare un prix moyen sur tous les BL : seul l'écart de cette ligne compte.
      if (pf !== null && r.puBl !== null && Math.abs(r.puBl - pf) > 0.005) {
        r.puFacture = pf;
        r.ecart = signe(Math.round((pf - r.puBl) * 10000) / 10000);
        r.enEcart = true;
      }
    } else if (g === 'en_trop') {
      r.ecart = 'livré, pas facturé';
      r.enEcart = true;
    } else {
      r.puFacture = pf;
      r.ecart = `facturé : ${qteFr(e.qte_facture)}`;
      r.enEcart = true;
    }
    return r;
  }

  let rangees = $derived.by(() => {
    const r = lignes.map(rangee);
    return [...r.filter((x) => x.enEcart), ...r.filter((x) => !x.enEcart)];
  });
  let visibles = $derived(tout ? rangees : rangees.slice(0, MAX));
  /** Facturés, absents de tous les BL de la facture. */
  let absents = $derived(factureLiee ? ecarts.filter((e) => genre(e) === 'manquant' && e.qte_bl <= 0.0005) : []);
</script>

<div class="carte-fiche">
  <h3 class="titre-carte">
    Articles du BL
    <span class="etiquette-carte">{factureLiee ? 'prix BL → facturé' : `${lignes.length} ligne${lignes.length > 1 ? 's' : ''}`}</span>
  </h3>
  <div class="defile-x">
    <table class="articles-bl">
      <thead>
        <tr>
          <th>Article</th>
          <th class="n">Qté</th>
          <th class="n">PU BL</th>
          {#if factureLiee}<th class="n">PU facturé</th><th class="n">Écart</th>{/if}
        </tr>
      </thead>
      <tbody>
        {#each visibles as r (r.cle)}
          <tr class:en-ecart={r.enEcart}>
            <td>{r.article}{#if r.reference}<span class="ref t2 num">{r.reference}</span>{/if}</td>
            <td class="n num">{r.qte}</td>
            <td class="n num">{formatMontant(r.puBl, '')}</td>
            {#if factureLiee}
              <td class="n num">{r.puFacture === null ? '—' : formatMontant(r.puFacture, '')}</td>
              <td class="n ecart">{r.ecart}</td>
            {/if}
          </tr>
        {/each}
        {#if absents.length}
          <tr class="sous-titre"><td colspan="5">Facturé, absent des BL</td></tr>
          {#each absents as e, i (i)}
            <tr class="en-ecart">
              <td>{e.designation ?? e.reference ?? 'Article'}{#if e.designation && e.reference}<span class="ref t2 num">{e.reference}</span>{/if}</td>
              <td class="n num">0</td>
              <td class="n num">—</td>
              <td class="n num">{e.prix_facture === null || e.prix_facture === undefined ? '—' : formatMontant(e.prix_facture, '')}</td>
              <td class="n ecart">facturé : {qteFr(e.qte_facture)}</td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </div>
  {#if rangees.length > MAX}
    <button type="button" class="btn-lien plus" aria-expanded={tout} onclick={() => (tout = !tout)}>
      {tout ? 'Montrer les 8 premiers' : `${rangees.length - MAX} autre${rangees.length - MAX > 1 ? 's' : ''} article${rangees.length - MAX > 1 ? 's' : ''}`}
    </button>
  {/if}
  {#if factureLiee && plusieursBl}
    <p class="t2 petit note">Quantités comparées sur l’ensemble des BL de la facture.</p>
  {/if}
</div>

<style>
  .articles-bl {
    font-size: 15px;
  }
  .articles-bl th {
    white-space: normal;
  }
  .articles-bl td {
    padding-top: 6px;
    padding-bottom: 6px;
  }
  .ref {
    display: block;
    font-size: 13px;
  }
  .ecart {
    white-space: nowrap;
    color: var(--muted);
  }
  .en-ecart .ecart {
    color: var(--warn);
    font-weight: 600;
  }
  .sous-titre td {
    color: var(--text2);
    font-size: 14px;
    font-weight: 600;
    padding-top: var(--e3);
  }
  .plus {
    margin-top: var(--e2);
  }
  .note {
    margin: var(--e2) 0 0;
  }
</style>
