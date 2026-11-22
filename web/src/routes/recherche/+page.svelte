<script lang="ts">
  // Recherche unique : documents et mails (GET /api/recherche), filtres type et période.
  // Requête vide : derniers mails reçus, publicités comprises (GET /api/mails).
  import { untrack } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, message, rechercher, requete } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import {
    libelleCategorie,
    libelleEtat,
    libelleStatutEnvoi,
    libelleStatutReponse,
    libelleType,
    nomCourtSociete,
    tonEtat,
    tonStatutEnvoi,
    tonStatutReponse,
    type Ton
  } from '#lib/libelles.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { DocumentTrouve, InterpretationRecherche, LigneMail, MailTrouve, Page } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import { dateMail, nomExpediteur } from '#lib/ui/MailFiche.svelte';
  import Montant, { formatMontant } from '#lib/ui/Montant.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  type TypeFiltre = '' | 'document' | 'facture' | 'avoir' | 'bl' | 'mail';
  const TYPES: { v: TypeFiltre; l: string }[] = [
    { v: '', l: 'Tout' },
    { v: 'document', l: 'Documents' },
    { v: 'facture', l: 'Factures' },
    { v: 'avoir', l: 'Avoirs' },
    { v: 'bl', l: 'BL' },
    { v: 'mail', l: 'Mails' }
  ];
  const DOCUMENTS: TypeFiltre[] = ['document', 'facture', 'avoir', 'bl'];

  type Mail = Pick<MailTrouve, 'id' | 'expediteur' | 'objet' | 'date_mail' | 'categorie' | 'statut_reponse' | 'nb_pieces'> & {
    resume?: string | null;
    boites?: string[];
  };
  type Groupe<T> = { total: number; elements: T[] };
  interface Resultats {
    /** `derniers` : requête vide, derniers mails reçus. */
    mode: 'recherche' | 'derniers';
    interpretation: InterpretationRecherche | null;
    documents: Groupe<DocumentTrouve> | null;
    mails: Groupe<Mail> | null;
  }

  // ---------- Paramètres (dans l'URL) ----------
  let params = $derived.by(() => {
    const p = page.url.searchParams;
    const t = (p.get('type') ?? '') as TypeFiltre;
    return {
      q: (p.get('q') ?? '').trim(),
      type: TYPES.some((x) => x.v === t) ? t : ('' as TypeFiltre),
      du: p.get('du') ?? '',
      au: p.get('au') ?? ''
    };
  });

  let saisie = $state('');
  let du = $state('');
  let au = $state('');
  $effect(() => {
    const p = params;
    saisie = p.q;
    du = p.du;
    au = p.au;
  });

  function aller(changes: Partial<typeof params>, remplacer = true) {
    const p = { ...params, ...changes };
    void goto(`/recherche${requete({ q: p.q, type: p.type, du: p.du, au: p.au })}`, {
      replace: remplacer,
      reset: false
    });
  }

  let champ = $state<HTMLInputElement>();
  function chercher(e: SubmitEvent) {
    e.preventDefault();
    champ?.blur();
    aller({ q: saisie.trim() }, saisie.trim() === params.q);
  }

  // ---------- Chargement ----------
  let resultats = $state<Resultats | null>(null);
  let enCours = $state(false);
  let erreur = $state('');
  let pageMails = $state(1);
  let plusEnCours = $state(false);
  let tour = 0;

  const versMail = (l: LigneMail): Mail => ({ ...l });

  async function charger(p: typeof params, s: number | null) {
    const moi = ++tour;
    enCours = true;
    erreur = '';
    try {
      let r: Resultats;
      if (!p.q && (p.type === '' || p.type === 'mail')) {
        const l = await api.get<Page<LigneMail>>(`/api/mails${requete({ societe: s, du: p.du, au: p.au })}`);
        r = { mode: 'derniers', interpretation: null, documents: null, mails: { total: l.total, elements: l.elements.map(versMail) } };
      } else {
        const t = await rechercher({ q: p.q, type: p.type, du: p.du, au: p.au, societe: s });
        r = {
          mode: 'recherche',
          interpretation: t.interpretation,
          documents: p.type === 'mail' ? null : t.documents,
          mails: DOCUMENTS.includes(p.type) ? null : t.mails
        };
      }
      if (moi !== tour) return;
      resultats = r;
      pageMails = 1;
      cur = -1;
    } catch (e) {
      if (moi === tour) erreur = message(e);
    } finally {
      if (moi === tour) enCours = false;
    }
  }

  $effect(() => {
    const p = params;
    const s = societe.id;
    untrack(() => charger(p, s));
  });

  async function plusDeMails() {
    if (!resultats?.mails) return;
    plusEnCours = true;
    try {
      const n = pageMails + 1;
      const l = await api.get<Page<LigneMail>>(
        `/api/mails${requete({ societe: societe.id, du: params.du, au: params.au, page: n })}`
      );
      resultats.mails.elements.push(...l.elements.map(versMail));
      resultats.mails.total = l.total;
      pageMails = n;
    } catch (e) {
      erreur = message(e);
    } finally {
      plusEnCours = false;
    }
  }

  // ---------- Clavier : J/K, Entrée ----------
  let liens = $derived([
    ...(resultats?.documents?.elements ?? []).map((d) => `/documents/${d.id}`),
    ...(resultats?.mails?.elements ?? []).map((m) => `/mails/${m.id}`)
  ]);
  let nbDocs = $derived(resultats?.documents?.elements.length ?? 0);
  let cur = $state(-1);
  let conteneur = $state<HTMLElement>();

  function bouger(sens: 1 | -1) {
    cur = deplacer(cur, sens, liens.length);
    montrer(conteneur, `[data-indice="${cur}"]`);
  }

  $effect(() =>
    raccourcis({
      j: () => bouger(1),
      k: () => bouger(-1),
      Enter: () => {
        if (cur < 0 || !liens[cur]) return false;
        void goto(liens[cur]);
      }
    })
  );

  $effect(() => {
    if (!untrack(() => params.q)) champ?.focus();
  });

  // ---------- Affichage ----------
  const deux = (n: number) => String(n).padStart(2, '0');

  /** 2026-11-18 : « 18/11 » cette année, « 18/11/25 » sinon. */
  function jourCourt(j: string | null | undefined): string {
    if (!j) return '';
    const [a, mo, jo] = j.slice(0, 10).split('-');
    return a === String(new Date().getFullYear()) ? `${jo}/${mo}` : `${jo}/${mo}/${a.slice(2)}`;
  }

  function etatDocument(d: DocumentTrouve): { texte: string; ton: Ton } {
    if (d.etat !== 'lu') return { texte: libelleEtat(d.etat), ton: tonEtat(d.etat) };
    if (d.type === 'facture' || d.type === 'avoir')
      return { texte: libelleStatutEnvoi(d.statut_depot, d.outil) || 'Lue', ton: tonStatutEnvoi(d.statut_depot) };
    if (d.type === 'bl') return d.verifie ? { texte: 'Validé', ton: 'ok' } : { texte: 'À valider', ton: 'alerte' };
    return { texte: 'Lu', ton: 'pale' };
  }

  let compris = $derived.by(() => {
    const i = resultats?.interpretation;
    if (!i) return [] as { k: string; v: string; mono: boolean }[];
    const out: { k: string; v: string; mono: boolean }[] = [];
    if (i.montant !== null) out.push({ k: 'montant (TTC, HT ou TVA)', v: formatMontant(i.montant), mono: true });
    if (i.date)
      out.push({
        k: i.date.annee ? 'date' : 'date, toute année',
        v: `${deux(i.date.jour)}/${deux(i.date.mois)}${i.date.annee ? `/${i.date.annee}` : ''}`,
        mono: true
      });
    for (const n of i.nombres) out.push({ k: 'numéro ou montant', v: String(n), mono: true });
    if (i.mots.length) out.push({ k: i.mots.length > 1 ? 'mots' : 'mot', v: i.mots.map((w) => `« ${w} »`).join(' '), mono: false });
    return out;
  });

  let rien = $derived(
    !!resultats && (resultats.documents?.elements.length ?? 0) === 0 && (resultats.mails?.elements.length ?? 0) === 0
  );
</script>

<svelte:head><title>{params.q ? `${params.q} · Recherche` : 'Recherche'} · GMFlow</title></svelte:head>

<div class="page">
  <form class="cherche" role="search" onsubmit={chercher}>
    <Icone nom="chercher" />
    <input
      bind:this={champ}
      bind:value={saisie}
      id="champ-recherche"
      type="search"
      placeholder="Fournisseur, n° de facture, montant (1 250,40), date (18/11)"
      aria-label="Chercher un document ou un mail"
      onkeydown={(e) => {
        if (e.key === 'Escape') champ?.blur();
      }}
    />
    <button class="btn principal">Chercher</button>
  </form>

  <div class="filtres">
    <div class="segments" role="group" aria-label="Type">
      {#each TYPES as t (t.v)}
        <button type="button" class="btn sm" aria-pressed={params.type === t.v} onclick={() => aller({ type: t.v })}>{t.l}</button>
      {/each}
    </div>
    <span class="vsep large-seul"></span>
    <label class="periode">
      <span>Du</span>
      <input type="date" bind:value={du} max={au || undefined} onchange={() => aller({ du })} />
    </label>
    <label class="periode">
      <span>au</span>
      <input type="date" bind:value={au} min={du || undefined} onchange={() => aller({ au })} />
    </label>
    {#if params.du || params.au}
      <button type="button" class="btn sm fantome" onclick={() => aller({ du: '', au: '' })}>Toute période</button>
    {/if}
  </div>

  <p class="lead compris">
    {#if resultats?.mode === 'derniers'}
      Derniers mails reçus{societe.courante ? ` par ${nomCourtSociete(societe.courante.raison_sociale)}` : ''}, publicités comprises.
    {:else if compris.length}
      Compris :
      {#each compris as c, i (i)}{i ? ', ' : ' '}{c.k} <b class:num={c.mono}>{c.v}</b>{/each}{societe.courante ? `, chez ${nomCourtSociete(societe.courante.raison_sociale)}` : ''}.
    {:else if resultats}
      Les plus récents{societe.courante ? ` de ${nomCourtSociete(societe.courante.raison_sociale)}` : ''}.
    {/if}
  </p>

  {#if erreur}
    <Bandeau ton="erreur" titre="La recherche n’a pas abouti.">
      {erreur}
      {#snippet action()}<button type="button" class="btn sm" onclick={() => charger(params, societe.id)}>Réessayer</button>{/snippet}
    </Bandeau>
  {/if}

  {#if !resultats && enCours}
    <Chargement texte="Recherche" />
  {:else if resultats}
    <div class="resultats" class:recharge={enCours} bind:this={conteneur}>
      {#if rien}
        <Vide
          icone="chercher"
          titre={resultats.mode === 'derniers' ? 'Aucun mail sur cette période' : 'Rien trouvé'}
          texte={resultats.mode === 'derniers'
            ? 'Changez la période, ou cherchez un fournisseur, un numéro ou un montant.'
            : 'Essayez un autre mot, un montant (1 250,40), une date (18/11), ou retirez un filtre.'}
        >
          {#if params.type || params.du || params.au}
            <button type="button" class="btn sm" onclick={() => aller({ type: '', du: '', au: '' })}>Retirer les filtres</button>
          {/if}
        </Vide>
      {/if}

      {#if resultats.documents && resultats.documents.elements.length}
        {@const g = resultats.documents}
        <section class="groupe">
          <div class="groupe-h">
            <h2>Documents</h2>
            <span class="num muted">{g.total}</span>
            {#if g.total > g.elements.length}
              <span class="petit muted">les {g.elements.length} plus récents : précisez la recherche pour voir les autres</span>
            {/if}
          </div>
          <div class="filets">
            {#each g.elements as d, i (d.id)}
              {@const e = etatDocument(d)}
              <a class="ligne" class:cur={cur === i} data-indice={i} href="/documents/{d.id}">
                <div class="a">{d.fournisseur || d.nom_fichier}</div>
                <div class="r">{#if d.montant_ttc !== null}<Montant valeur={d.montant_ttc} avoir={d.type === 'avoir'} />{/if}</div>
                <div class="b">
                  {libelleType(d.type)}{#if d.numero}{' '}<span class="num">{d.numero}</span>{/if}{d.societe ? `, ${nomCourtSociete(d.societe)}` : ''}
                  <span class="etat"><Etiquette ton={e.ton}>{e.texte}</Etiquette></span>
                </div>
                <div class="r2"><span class="num">{jourCourt(d.date_document ?? d.recu_le)}</span></div>
              </a>
            {/each}
          </div>
        </section>
      {/if}

      {#if resultats.mails && resultats.mails.elements.length}
        {@const g = resultats.mails}
        <section class="groupe">
          <div class="groupe-h">
            <h2>Mails</h2>
            <span class="num muted">{g.total}</span>
            {#if resultats.mode === 'recherche' && g.total > g.elements.length}
              <span class="petit muted">les {g.elements.length} plus récents : précisez la recherche pour voir les autres</span>
            {/if}
          </div>
          <div class="filets">
            {#each g.elements as m, i (m.id)}
              {@const k = nbDocs + i}
              <a class="ligne" class:cur={cur === k} data-indice={k} href="/mails/{m.id}" class:pub={m.categorie === 'publicite'}>
                <div class="a">{nomExpediteur(m.expediteur) || '(expéditeur inconnu)'}</div>
                <div class="r">
                  {#if m.statut_reponse === 'a_repondre'}
                    <Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>
                  {:else if m.categorie}
                    <span class="cat">{libelleCategorie(m.categorie)}</span>
                  {/if}
                </div>
                <div class="b">
                  {#if m.nb_pieces}<span class="pj" title="{m.nb_pieces} pièce{m.nb_pieces > 1 ? 's' : ''} jointe{m.nb_pieces > 1 ? 's' : ''}"><Icone nom="trombone" />{#if m.nb_pieces > 1}<span class="num">{m.nb_pieces}</span>{/if}</span>{/if}
                  {m.objet || '(sans objet)'}{#if m.resume}<span class="muted">{` · ${m.resume}`}</span>{/if}
                </div>
                <div class="r2"><span class="num">{dateMail(m.date_mail, false)}</span></div>
              </a>
            {/each}
          </div>
          {#if resultats.mode === 'derniers' && g.elements.length < g.total}
            <button type="button" class="plus" disabled={plusEnCours} onclick={plusDeMails}>
              {plusEnCours ? 'Chargement…' : `Afficher les mails plus anciens (${g.total - g.elements.length})`}
            </button>
          {/if}
        </section>
      {/if}
    </div>

    {#if liens.length}
      <div class="touches-aide large-seul">
        <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
        <span><Kbd t="Enter" /> ouvrir</span>
        <span><Kbd t="/" /> chercher</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .cherche {
    position: relative;
    display: flex;
    gap: var(--e2);
    max-width: 760px;
    margin-bottom: var(--e3);
  }
  .cherche > :global(svg.i) {
    position: absolute;
    left: 12px;
    top: 10px;
    color: var(--muted);
    pointer-events: none;
  }
  .cherche input {
    padding-left: 36px;
  }
  .filtres {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--e2);
    margin-bottom: var(--e3);
  }
  .segments {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e1);
  }
  .segments .btn[aria-pressed='true'] {
    background: var(--accent-weak);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
  .periode {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text2);
    font-size: 12px;
  }
  .periode input {
    width: auto;
    height: 28px;
    font-size: 12px;
  }
  .compris {
    margin-bottom: var(--e5);
    font-size: 12px;
    max-width: none;
  }
  .compris b {
    font-weight: 500;
    color: var(--text);
  }
  .resultats.recharge {
    opacity: 0.55;
    transition: opacity var(--moyen) var(--ease);
  }
  .groupe {
    margin-bottom: var(--e8);
  }
  .groupe-h {
    display: flex;
    align-items: baseline;
    gap: var(--e2);
    margin-bottom: var(--e2);
  }
  .groupe-h h2 {
    font-size: 14px;
  }
  /* sélection J/K : les filets retirent le bord gauche de .ligne.cur, on le remet */
  .filets > .ligne.cur {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .ligne .etat {
    margin-left: var(--e2);
  }
  .ligne .cat {
    color: var(--muted);
    font-size: 12px;
  }
  .ligne.pub .a {
    color: var(--text2);
    font-weight: 400;
  }
  .pj {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-right: var(--e1);
    color: var(--muted);
    vertical-align: -3px;
  }
  .pj :global(svg.i) {
    width: 13px;
    height: 13px;
  }
  .touches-aide {
    display: flex;
    gap: var(--e4);
    color: var(--muted);
    font-size: 12px;
  }
  .touches-aide span {
    display: flex;
    align-items: center;
    gap: var(--e1);
  }
  @media (max-width: 760px) {
    .segments {
      flex-wrap: nowrap;
      overflow-x: auto;
      max-width: 100%;
    }
    .segments .btn {
      flex: none;
    }
    .cherche .btn {
      display: none;
    }
    .periode input {
      height: 40px;
    }
  }
</style>
