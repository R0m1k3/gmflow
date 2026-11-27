<script lang="ts">
  // Compta > Envois Dext / Yooz : factures et avoirs validés, selon ce qu'est devenu leur envoi
  // (GET /api/documents?statut_depot=…). Les échecs d'abord : ce sont eux qui demandent une action.
  import ListeDocuments, { type FiltreDocuments } from '#lib/ui/ListeDocuments.svelte';

  const filtres: FiltreDocuments[] = [
    { v: 'echec', l: 'Échec d’envoi', params: { statut_depot: 'erreur' } },
    { v: 'envoyees', l: 'Envoyées', params: { statut_depot: 'depose' } },
    { v: 'doublons', l: 'Doublons', params: { statut_depot: 'doublon' } }
  ];
</script>

<ListeDocuments
  titre="Envois Dext / Yooz"
  chemin="/envois"
  {filtres}
  rechercheType="facture"
  videTitre="Aucun envoi dans cette liste"
  singulier="une pièce"
  section="Envoi"
  sansSuite="Aucun autre envoi dans cette liste"
/>
