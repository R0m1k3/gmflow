<script lang="ts">
  // Onglet BL : bons de livraison reçus (GET /api/documents?type=bl).
  import ListeDocuments, { type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'tous', l: 'Tous', params: { type: 'bl' } },
    { v: 'a-valider', l: 'À valider', params: { type: 'bl', verifie: 'non' } },
    { v: 'valides', l: 'Validés', params: { type: 'bl', verifie: 'oui' } }
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
  sansSuite="Aucun autre BL à valider"
  pied="valider"
/>
