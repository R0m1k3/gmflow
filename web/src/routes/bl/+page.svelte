<script lang="ts">
  // Onglet BL : bons de livraison reçus (GET /api/documents?type=bl).
  import ListeDocuments, { TRI_DOCUMENTS, type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'tous', l: 'Tous', params: { type: 'bl' } },
    { v: 'a-valider', l: 'À contrôler', params: { type: 'bl', verifie: 'non' } },
    { v: 'valides', l: 'Validés', params: { type: 'bl', verifie: 'oui' } },
    // Accueil : « N BL sans facture depuis 7 jours ».
    { v: 'sans-facture', l: 'Sans facture depuis 7 jours', params: { type: 'bl', sans_facture: '7' } },
    { v: 'ecartes', l: 'Écartés', params: { type: 'bl', etat: 'ignore' } }
  ];
</script>

<ListeDocuments
  titre="Bons de livraison"
  chemin="/bl"
  {filtres}
  rechercheType="bl"
  videTitre="Aucun BL"
  singulier="un BL"
  section="BL"
  sansSuite="Aucun autre BL à contrôler"
  pied="valider"
  colonnesTri={TRI_DOCUMENTS.filter((c) => c.cle !== 'echeance')}
/>
