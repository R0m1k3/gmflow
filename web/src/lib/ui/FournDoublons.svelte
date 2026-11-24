<script lang="ts">
  // Doublons probables du référentiel (admin) : paires de fiches qui désignent sans doute le même
  // fournisseur. « Garder » fusionne l'autre fiche dedans ; « Regrouper » range les deux dans un
  // même groupe (U10 pour L3C et Homea), sans rien fusionner ; « Pas un doublon » ne la propose plus.
  // Les paires « même groupe probable » (même SIREN, noms sans rapport) n'offrent pas la fusion.
  import { api, message, modifierFournisseur } from '#lib/api.ts';
  import type { FicheCourte, PaireDoublon } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Chargement from './Chargement.svelte';
  import Confirmation from './Confirmation.svelte';
  import Vide from './Vide.svelte';

  let {
    ouvert,
    onouvrir,
    onchange
  }: {
    /** Fiche ouverte dans le volet de droite. */
    ouvert: number | null;
    onouvrir: (id: number) => void;
    /** Après une fusion ou un « pas un doublon » (compteurs, liste). */
    onchange: (restants: number) => void;
  } = $props();

  let paires = $state<PaireDoublon[] | null>(null);
  let erreur = $state('');
  let info = $state('');

  async function charger() {
    try {
      paires = await api.get<PaireDoublon[]>('/api/admin/fournisseurs/doublons');
      erreur = '';
      onchange(paires.length);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    void charger();
  });

  /** La fiche à garder par défaut : celle qui a un code BO Phénix, confirmée, la plus utilisée. */
  function aGarder(p: PaireDoublon): FicheCourte {
    const poids = (f: FicheCourte) => (f.code_gestion ? 4 : 0) + (f.statut === 'valide' ? 2 : 0) + (f.siret || f.tva ? 1 : 0);
    const [a, b] = [p.a, p.b];
    if (poids(a) !== poids(b)) return poids(a) > poids(b) ? a : b;
    if (a.nb_documents !== b.nb_documents) return a.nb_documents > b.nb_documents ? a : b;
    return a;
  }

  // ---------- Fusion confirmée ----------
  let fusion = $state<{ garder: FicheCourte; retirer: FicheCourte } | null>(null);
  let confirmationOuverte = $state(false);
  function demanderFusion(garder: FicheCourte, retirer: FicheCourte) {
    fusion = { garder, retirer };
    confirmationOuverte = true;
  }
  async function fusionner() {
    if (!fusion) return;
    const { garder, retirer } = fusion;
    try {
      const r = await api.post<{ documents: number }>(`/api/admin/fournisseurs/${retirer.id}/fusionner`, { cible_id: garder.id });
      info = `« ${retirer.nom} » fusionnée dans « ${garder.nom} » : ${r.documents} document${r.documents > 1 ? 's' : ''} déplacé${r.documents > 1 ? 's' : ''}`;
      if (ouvert === retirer.id) onouvrir(garder.id);
      await charger();
    } catch (e) {
      erreur = message(e);
      throw e;
    }
  }

  async function distinct(p: PaireDoublon) {
    try {
      await api.post('/api/admin/fournisseurs/doublons/distinct', { a: p.a.id, b: p.b.id });
      info = `« ${p.a.nom} » et « ${p.b.nom} » ne seront plus proposées`;
      await charger();
    } catch (e) {
      erreur = message(e);
    }
  }

  // ---------- Regrouper ----------
  type ChoixGroupe =
    | { genre: 'dans'; cle: string; groupe: number; nomGroupe: string; membres: FicheCourte[] }
    | { genre: 'nouveau'; cle: string };
  let regroupement = $state<{ p: PaireDoublon; choix: ChoixGroupe[]; cle: string; nom: string } | null>(null);
  let regroupementOuvert = $state(false);
  let erreurGroupe = $state('');

  /** Façons possibles de regrouper la paire (un seul niveau : un groupe n'entre pas dans un autre). */
  function choixPossibles(p: PaireDoublon): ChoixGroupe[] {
    const v: ChoixGroupe[] = [];
    for (const [x, y] of [
      [p.a, p.b],
      [p.b, p.a]
    ]) {
      if (x.groupe_id !== null && y.groupe_id === null && y.nb_marques === 0) {
        v.push({ genre: 'dans', cle: `g${x.groupe_id}`, groupe: x.groupe_id, nomGroupe: x.groupe ?? 'son groupe', membres: [y] });
      } else if (x.groupe_id === null && y.groupe_id === null && y.nb_marques === 0) {
        v.push({ genre: 'dans', cle: `f${x.id}`, groupe: x.id, nomGroupe: x.nom, membres: [y] });
      }
    }
    // Une fiche qui a déjà des marques est sans doute le groupe : proposée d'abord.
    const marques = (c: ChoixGroupe) => (c.genre === 'dans' ? ([p.a, p.b].find((f) => f.id === c.groupe)?.nb_marques ?? 0) : 0);
    v.sort((c1, c2) => marques(c2) - marques(c1));
    if ([p.a, p.b].every((f) => f.groupe_id === null && f.nb_marques === 0)) v.push({ genre: 'nouveau', cle: 'nouveau' });
    return v;
  }
  function demanderRegroupement(p: PaireDoublon) {
    const choix = choixPossibles(p);
    erreurGroupe = choix.length ? '' : 'Ces deux fiches sont déjà dans des groupes différents : retirez-en une de son groupe d’abord.';
    regroupement = { p, choix, cle: choix[0]?.cle ?? '', nom: '' };
    regroupementOuvert = true;
  }
  async function regrouper() {
    if (!regroupement) return;
    const { p, choix, cle, nom } = regroupement;
    const c = choix.find((x) => x.cle === cle);
    erreurGroupe = '';
    try {
      if (!c) throw new Error('Choisissez comment regrouper');
      let groupe: number;
      let nomGroupe: string;
      if (c.genre === 'nouveau') {
        if (!nom.trim()) throw new Error('Donnez le nom du groupe (par exemple U10)');
        const g = await api.post<{ id: number; nom: string }>('/api/admin/fournisseurs', { nom: nom.trim(), statut: 'valide' });
        groupe = g.id;
        nomGroupe = g.nom;
        for (const f of [p.a, p.b]) await modifierFournisseur(f.id, { groupe_id: groupe });
      } else {
        groupe = c.groupe;
        nomGroupe = c.nomGroupe;
        for (const f of c.membres) await modifierFournisseur(f.id, { groupe_id: groupe });
      }
      let ranges = 0;
      try {
        ranges = (await api.post<{ deplaces: number }>(`/api/admin/fournisseurs/${groupe}/repartir`, {})).deplaces;
      } catch {
        /* rien à répartir */
      }
      info =
        `Regroupées dans « ${nomGroupe} »` + (ranges ? ` : ${ranges} document${ranges > 1 ? 's' : ''} rangé${ranges > 1 ? 's' : ''} chez leur marque` : '');
      await charger();
    } catch (e) {
      erreurGroupe = message(e);
      throw e;
    }
  }
  const libelleChoix = (c: ChoixGroupe) =>
    c.genre === 'nouveau'
      ? 'Créer le groupe, les deux fiches en sont des marques'
      : `« ${c.nomGroupe} » est le groupe, « ${c.membres.map((m) => m.nom).join(' », « ')} » une de ses marques`;

  const details = (f: FicheCourte) =>
    [
      f.groupe && `groupe ${f.groupe}`,
      f.nb_marques && `${f.nb_marques} marque${f.nb_marques > 1 ? 's' : ''}`,
      f.code_gestion && `BO ${f.code_gestion}`,
      f.siret ? `SIRET ${f.siret}` : f.tva && `TVA ${f.tva}`,
      f.domaines[0],
      `${f.nb_documents} doc.`
    ]
      .filter(Boolean)
      .join(' · ');
</script>

{#snippet fiche(f: FicheCourte)}
  <button type="button" class="fiche" aria-current={f.id === ouvert ? 'true' : undefined} onclick={() => onouvrir(f.id)}>
    <span class="nom">{f.nom}</span>
    {#if f.statut === 'a_confirmer'}<span class="etiquette att">À confirmer</span>{/if}
    <span class="det">{details(f)}</span>
  </button>
{/snippet}

<div class="doublons">
  {#if erreur}<div class="marge"><Bandeau ton="erreur">{erreur}</Bandeau></div>{/if}
  {#if info}<div class="marge"><Bandeau ton="ok" role="status">{info}</Bandeau></div>{/if}
  {#if paires === null}
    {#if !erreur}<Chargement />{/if}
  {:else if paires.length === 0}
    <Vide compact titre="Aucun doublon probable" texte="Chaque fournisseur a une seule fiche." />
  {:else}
    <p class="marge intro">
      Ces fiches désignent sans doute le même fournisseur : choisissez celle à garder, l'autre y est fusionnée avec ses documents. « Même
      groupe probable » : deux marques d'une même société (L3C et Homea chez U10), à regrouper sans les fusionner.
    </p>
    {#each paires as p (`${p.a.id}-${p.b.id}`)}
      {@const g = aGarder(p)}
      {@const autre = g.id === p.a.id ? p.b : p.a}
      <div class="paire">
        <div class="raison" class:groupe={p.meme_groupe}>{p.raison.charAt(0).toUpperCase() + p.raison.slice(1)}</div>
        {@render fiche(p.a)}
        {@render fiche(p.b)}
        <div class="actions">
          {#if p.meme_groupe}
            <button type="button" class="btn sm principal" onclick={() => demanderRegroupement(p)}>Regrouper</button>
          {:else}
            <button type="button" class="btn sm principal" onclick={() => demanderFusion(g, autre)}>Garder « {g.nom} »</button>
            <button type="button" class="btn sm" onclick={() => demanderFusion(autre, g)}>Garder « {autre.nom} »</button>
            <button type="button" class="btn sm" onclick={() => demanderRegroupement(p)}>Regrouper</button>
          {/if}
          <button type="button" class="btn sm fantome" onclick={() => distinct(p)}>{p.meme_groupe ? 'Sans rapport' : 'Pas un doublon'}</button>
        </div>
      </div>
    {/each}
  {/if}
</div>

<Confirmation bind:ouvert={confirmationOuverte} titre="Fusionner ces deux fiches ?" libelle="Fusionner" onconfirmer={fusionner}>
  {#if fusion}
    « {fusion.retirer.nom} » disparaît : ses {fusion.retirer.nb_documents} document{fusion.retirer.nb_documents > 1 ? 's' : ''}, domaines et
    identifiants passent sur « {fusion.garder.nom} ».
  {/if}
</Confirmation>

<Confirmation bind:ouvert={regroupementOuvert} titre="Regrouper ces deux fiches ?" libelle="Regrouper" onconfirmer={regrouper}>
  {#if regroupement}
    <p>Chaque fiche garde ses documents et son code BO Phénix. Les documents restés sur le groupe vont ensuite à leur marque.</p>
    <fieldset class="choix-groupe">
      <legend class="petit t2">Qui est le groupe ?</legend>
      {#each regroupement.choix as c (c.cle)}
        <label class="case"><input type="radio" name="choix-groupe" value={c.cle} bind:group={regroupement.cle} /> {libelleChoix(c)}</label>
      {/each}
      {#if regroupement.cle === 'nouveau'}
        <input class="nom-groupe" bind:value={regroupement.nom} placeholder="Nom du groupe, ex. U10" aria-label="Nom du groupe" />
      {/if}
    </fieldset>
    {#if erreurGroupe}<p class="erreur-groupe petit" role="alert">{erreurGroupe}</p>{/if}
  {/if}
</Confirmation>

<style>
  .choix-groupe {
    display: grid;
    gap: var(--e2);
    margin: var(--e3) 0 0;
    padding: 0;
    border: 0;
  }
  .nom-groupe {
    margin-left: 26px;
  }
  .erreur-groupe {
    color: var(--err-ink);
    margin-top: var(--e2);
  }
  .raison {
    color: var(--text2);
    text-transform: none;
  }
  .raison.groupe {
    color: var(--text);
    font-weight: 500;
  }
  .marge {
    padding: 0 4px 12px;
  }
  .intro {
    font-size: 15px;
    color: var(--text2);
    text-wrap: pretty;
  }
  .raison {
    font-size: 15px;
  }
  .paire {
    display: grid;
    gap: 6px;
    padding: 12px 4px 16px;
    border-top: 1px solid var(--line-faint);
  }
  .fiche {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 2px 8px;
    padding: 10px 14px;
    border: 0;
    border-radius: var(--r2);
    box-shadow: 0 0 0 1px var(--line);
    background: var(--white);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .fiche:hover {
    background: var(--pane);
  }
  .fiche[aria-current='true'] {
    background: var(--sunk);
    box-shadow: 0 0 0 1px var(--line);
  }
  .nom {
    font-size: 16px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .det {
    grid-column: 1 / -1;
    font-size: 14px;
    color: var(--text2);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-top: 4px;
  }
</style>
