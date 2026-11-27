<script lang="ts">
  // Accueil des réglages : la mise en route (étapes ouvertes). Une ancienne ancre de la page
  // unique (`/reglages#boites`, favoris, anciens liens) mène à la vue de sa section.
  import { goto } from '$app/navigation';
  import { adresseAncre } from '#lib/reglages.ts';
  import type { CleEtape } from '#lib/types.ts';
  import MiseEnRoute from '../MiseEnRoute.svelte';
  import { donneesReglages } from './donnees.svelte.ts';

  const d = donneesReglages();
  let ouvert = $state(true);

  $effect(() => {
    const cible = adresseAncre(location.hash);
    if (cible && cible !== '/reglages') void goto(cible, { replaceState: true });
  });

  /** « Ajouter une société »… : la vue de la section, son formulaire ouvert. */
  function agir(cle: CleEtape) {
    if (cle === 'boites') return goto('/reglages/boites/nouvelle');
    return goto(`/reglages/${cle}?ouvrir=1`);
  }
  function aller(id: string) {
    return goto(`/reglages/${id}`);
  }
</script>

<MiseEnRoute etat={d.etat} boites={d.boites} societes={d.societes} {agir} {aller} bind:ouvert />
