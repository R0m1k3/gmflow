<script lang="ts">
  // Création d'une fiche fournisseur (admins). D'habitude inutile : chaque nouveau fournisseur
  // arrive tout seul « à confirmer » avec ses factures.
  import { api, message } from '#lib/api.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Fournisseur } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Champ from './Champ.svelte';
  import { corpsFournisseur, type EditionFournisseur } from './FournFiche.svelte';

  let { oncree, onannule }: { oncree: (id: number) => void; onannule: () => void } = $props();

  let ed = $state<EditionFournisseur>({
    nom: '',
    code_gestion: '',
    siret: '',
    tva: '',
    domaines: '',
    statut: 'valide',
    societe_defaut_id: '',
    format_bl: '',
    notes: ''
  });
  let erreur = $state('');
  let enCours = $state(false);

  async function creer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    enCours = true;
    try {
      const v = await api.post<Fournisseur>('/api/admin/fournisseurs', corpsFournisseur(ed));
      oncree(v.id);
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }
</script>

<div class="titre-fiche"><h1>Nouveau fournisseur</h1></div>
<p class="meta-fiche">Rarement utile : un fournisseur inconnu est créé tout seul, à confirmer, dès sa première facture.</p>
{#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
<form class="formulaire" onsubmit={creer}>
  <div class="plein"><Champ libelle="Nom" bind:valeur={ed.nom} requis /></div>
  <div class="plein"><Champ libelle="Code fournisseur" bind:valeur={ed.code_gestion} mono aide="Le code de ce fournisseur dans le logiciel de gestion du magasin" /></div>
  <Champ libelle="SIRET" bind:valeur={ed.siret} mono inputmode="numeric" />
  <Champ libelle="N° TVA intracommunautaire" bind:valeur={ed.tva} mono />
  <div class="plein">
    <Champ libelle="Domaines mail" bind:valeur={ed.domaines} placeholder="durand.fr" aide="Séparés par des virgules" />
  </div>
  <Champ
    libelle="Société par défaut"
    type="select"
    bind:valeur={ed.societe_defaut_id}
    options={[{ valeur: '', libelle: 'Aucune société' }, ...societe.liste.map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))]}
  />
  <Champ libelle="Format des BL" bind:valeur={ed.format_bl} placeholder="ex. Excel, colonnes réf. / qté" />
  <div class="plein"><Champ libelle="Notes" type="textarea" bind:valeur={ed.notes} /></div>
  <label class="case plein"><input type="checkbox" checked={ed.statut === 'valide'} onchange={(e) => (ed.statut = e.currentTarget.checked ? 'valide' : 'a_confirmer')} /> Fiche confirmée</label>
  <div class="plein boutons">
    <button class="btn principal" class:en-cours={enCours} disabled={enCours}>Créer la fiche</button>
    <button type="button" class="btn fantome" onclick={onannule}>Annuler</button>
  </div>
</form>

<style>
  .formulaire {
    margin-top: var(--e4);
  }
  .boutons {
    display: flex;
    gap: var(--e2);
  }
</style>
