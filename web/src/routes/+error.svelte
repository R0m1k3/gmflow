<script lang="ts">
  // Page d'erreur : adresse inconnue (ancien lien, faute de frappe) ou erreur de chargement.
  import { page } from '$app/state';
  import Vide from '#lib/ui/Vide.svelte';

  let introuvable = $derived(page.status === 404);
</script>

<svelte:head><title>{introuvable ? 'Page introuvable' : 'Erreur'} · GMFlow</title></svelte:head>

<div class="page">
  <Vide
    icone={introuvable ? 'chercher' : 'erreur'}
    titre={introuvable ? 'Cette page n’existe pas.' : 'La page n’a pas pu s’ouvrir.'}
    texte={introuvable
      ? 'Le lien est peut-être ancien. Tout ce qui attend une action est dans À traiter.'
      : (page.error?.message ?? 'Réessayez dans un instant.')}
  >
    <a class="btn" href="/">Retour à À traiter</a>
  </Vide>
</div>
