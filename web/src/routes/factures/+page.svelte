<script lang="ts">
  // Onglet Factures : factures et avoirs reçus (GET /api/documents?type=facture|avoir).
  import ListeDocuments, { type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'toutes', l: 'Toutes', params: { type: 'facture' } },
    { v: 'a-valider', l: 'À valider', params: { type: 'facture', verifie: 'non' } },
    { v: 'echec', l: 'Envoi échoué', params: { type: 'facture', statut_depot: 'erreur' } },
    { v: 'envoyees', l: 'Envoyées', params: { type: 'facture', statut_depot: 'depose' } },
    { v: 'avoirs', l: 'Avoirs', params: { type: 'avoir' } }
  ];
</script>

<ListeDocuments
  titre="Factures" chemin="/factures" {filtres} rechercheType="facture" videTitre="Aucune facture"
  singulier="une facture"
  section="Facture"
  sansSuite="Aucune autre facture dans cette liste"
/>
