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
    notes: '',
    adresse: '',
    telephone: '',
    email: '',
    interlocuteur: '',
    conditions_paiement: '',
    noms_documents: '',
    prefixes_codes: '',
    groupe_id: null
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

<div class="tete">
  <h2 class="nom">Nouveau fournisseur</h2>
  <p class="meta">Rarement utile : un fournisseur inconnu est créé tout seul, à confirmer, dès sa première facture.</p>
</div>
{#if erreur}<div class="err"><Bandeau ton="erreur">{erreur}</Bandeau></div>{/if}
<form class="formulaire" onsubmit={creer}>
  <div class="plein"><Champ libelle="Nom" bind:valeur={ed.nom} requis /></div>
  <div class="plein"><Champ libelle="Code BO Phénix" bind:valeur={ed.code_gestion} mono aide="Notre code interne de ce fournisseur dans Phénix" /></div>
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
  <div class="plein"><Champ libelle="Adresse" type="textarea" bind:valeur={ed.adresse} placeholder="N°, rue, code postal, ville" /></div>
  <Champ libelle="Téléphone" bind:valeur={ed.telephone} inputmode="tel" />
  <Champ libelle="E-mail de contact" bind:valeur={ed.email} inputmode="email" />
  <Champ libelle="Interlocuteur" bind:valeur={ed.interlocuteur} />
  <Champ libelle="Conditions de paiement" bind:valeur={ed.conditions_paiement} placeholder="ex. 30 jours fin de mois" />
  <Champ libelle="Format des BL" bind:valeur={ed.format_bl} placeholder="ex. Excel, colonnes réf. / qté" />
  <div class="plein"><Champ libelle="Notes" type="textarea" bind:valeur={ed.notes} /></div>
  <label class="case plein"><input type="checkbox" checked={ed.statut === 'valide'} onchange={(e) => (ed.statut = e.currentTarget.checked ? 'valide' : 'a_confirmer')} /> Fiche confirmée</label>
  <div class="plein boutons">
    <button class="btn principal" class:en-cours={enCours} disabled={enCours}>Créer la fiche</button>
    <button type="button" class="btn fantome" onclick={onannule}>Annuler</button>
  </div>
</form>

<style>
  .tete {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 32px;
    max-width: 816px;
  }
  .nom {
    font-size: 28px;
    font-weight: 500;
    line-height: 1.15;
  }
  .meta {
    font-size: 16px;
    color: var(--text2);
    text-wrap: pretty;
  }
  .formulaire {
    max-width: 816px;
    padding: 20px;
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
  }
  @media (max-width: 760px) {
    .nom {
      font-size: 22px;
    }
    .formulaire {
      padding: 16px;
    }
  }
  .err {
    margin-bottom: 16px;
    max-width: 816px;
  }
  .boutons {
    display: flex;
    gap: var(--e2);
  }
</style>
