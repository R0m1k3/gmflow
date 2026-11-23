<script lang="ts">
  // Doublons probables du référentiel (admin) : paires de fiches qui désignent sans doute le même
  // fournisseur. « Garder » fusionne l'autre fiche dedans ; « Pas un doublon » ne la propose plus.
  import { api, message } from '#lib/api.ts';
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

  const details = (f: FicheCourte) =>
    [f.code_gestion && `BO ${f.code_gestion}`, f.siret ? `SIRET ${f.siret}` : f.tva && `TVA ${f.tva}`, f.domaines[0], `${f.nb_documents} doc.`]
      .filter(Boolean)
      .join(' · ');
</script>

{#snippet fiche(f: FicheCourte)}
  <button type="button" class="fiche" aria-current={f.id === ouvert ? 'true' : undefined} onclick={() => onouvrir(f.id)}>
    <span class="nom">{f.nom}</span>
    {#if f.statut === 'a_confirmer'}<span class="flag w">Nouveau</span>{/if}
    <span class="det petit muted">{details(f)}</span>
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
    <p class="marge petit t2">
      Ces fiches désignent sans doute le même fournisseur. Choisissez celle à garder : l'autre y est fusionnée, avec ses
      documents.
    </p>
    {#each paires as p (`${p.a.id}-${p.b.id}`)}
      {@const g = aGarder(p)}
      {@const autre = g.id === p.a.id ? p.b : p.a}
      <div class="paire">
        <div class="raison petit muted">{p.raison}</div>
        {@render fiche(p.a)}
        {@render fiche(p.b)}
        <div class="actions">
          <button type="button" class="btn sm principal" onclick={() => demanderFusion(g, autre)}>Garder « {g.nom} »</button>
          <button type="button" class="btn sm" onclick={() => demanderFusion(autre, g)}>Garder « {autre.nom} »</button>
          <button type="button" class="btn sm fantome" onclick={() => distinct(p)}>Pas un doublon</button>
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

<style>
  .marge {
    padding: var(--e3) var(--e4) 0;
  }
  .paire {
    display: grid;
    gap: var(--e1);
    padding: var(--e3) var(--e4);
    border-bottom: 1px solid var(--line);
  }
  .raison {
    text-transform: none;
  }
  .fiche {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0 var(--e2);
    padding: var(--e2) var(--e3);
    border: 1px solid var(--line);
    border-left: 3px solid var(--line);
    border-radius: 4px;
    background: var(--bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .fiche:hover {
    background: var(--pane);
  }
  .fiche[aria-current='true'] {
    background: var(--accent-weak);
    border-left-color: var(--accent);
  }
  .nom {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .det {
    grid-column: 1 / -1;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
    padding-top: var(--e1);
  }
</style>
