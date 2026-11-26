<script lang="ts">
  // Onglet Factures : factures reçues (GET /api/documents?type=facture) ; les avoirs ont leur onglet.
  import ListeDocuments, { type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'toutes', l: 'Toutes', params: { type: 'facture' } },
    { v: 'a-valider', l: 'À contrôler', params: { type: 'facture', verifie: 'non' } },
    { v: 'echec', l: 'Échec d’envoi', params: { type: 'facture', statut_depot: 'erreur' } },
    { v: 'envoyees', l: 'Envoyées', params: { type: 'facture', statut_depot: 'depose' } }
  ];
</script>

<ListeDocuments
  titre="Factures" chemin="/factures" {filtres} rechercheType="facture" videTitre="Aucune facture"
  singulier="une facture"
  section="Facture"
  sansSuite="Aucune autre facture dans cette liste"
/>
