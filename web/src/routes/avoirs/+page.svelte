<script lang="ts">
  // Onglet Avoirs : avoirs reçus (GET /api/documents?type=avoir), comme l'onglet Factures.
  import ListeDocuments, { type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'tous', l: 'Tous', params: { type: 'avoir' } },
    { v: 'a-valider', l: 'À contrôler', params: { type: 'avoir', verifie: 'non' } },
    { v: 'echec', l: 'Échec d’envoi', params: { type: 'avoir', statut_depot: 'erreur' } },
    { v: 'envoyes', l: 'Envoyés', params: { type: 'avoir', statut_depot: 'depose' } },
    { v: 'doublons', l: 'Doublons', params: { type: 'avoir', statut_depot: 'doublon' } },
    { v: 'ecartes', l: 'Écartés', params: { type: 'avoir', etat: 'ignore' } }
  ];
</script>

<ListeDocuments titre="Avoirs" chemin="/avoirs" {filtres} rechercheType="avoir" videTitre="Aucun avoir" singulier="un avoir" section="Avoir" sansSuite="Aucun autre avoir dans cette liste" filtreLot="a-valider" />
