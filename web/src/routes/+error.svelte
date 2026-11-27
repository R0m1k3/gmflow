<script lang="ts">
  // Page d'erreur : adresse inconnue (ancien lien, faute de frappe) ou erreur de chargement.
  import { page } from '$app/state';
  import { expliquerStatut } from '#lib/erreurs.ts';
  import { session } from '#lib/session.svelte.ts';
  import Icone from '#lib/ui/Icone.svelte';

  let introuvable = $derived(page.status === 404);
  // Autre erreur : message simple, que faire et bouton d'action (#lib/erreurs.ts).
  let x = $derived(expliquerStatut(page.status, page.error?.message ?? '', session.moi?.role === 'admin'));
</script>

<svelte:head><title>{introuvable ? 'Page introuvable' : 'Erreur'} · GMFlow</title></svelte:head>

<div class="page erreur-page">
  <div class="carte-erreur" role={introuvable ? undefined : 'alert'}>
    <span class="rond" class:ko={!introuvable}><Icone nom={introuvable ? 'chercher' : 'erreur'} /></span>
    <h1>{introuvable ? 'Cette page n’existe pas.' : 'La page n’a pas pu s’ouvrir.'}</h1>
    <p>
      {introuvable
        ? 'Le lien est peut-être ancien. Ce qui attend une action est sur l’accueil, et signalé par un badge dans le menu.'
        : x.message}
    </p>
    {#if !introuvable && x.quoiFaire}<p>{x.quoiFaire}</p>{/if}
    <div class="actions">
      <a class="btn principal" href="/">Retour à l’accueil</a>
      {#if introuvable}
        <a class="btn" href="/factures?filtre=a-valider">Factures à contrôler</a>
      {:else if x.action?.genre === 'lien'}
        <a class="btn" href={x.action.href}>{x.action.libelle}</a>
      {:else if x.action?.genre === 'reconnecter'}
        <a class="btn" href="/connexion">{x.action.libelle}</a>
      {:else}
        <button type="button" class="btn" onclick={() => location.reload()}>Réessayer</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .erreur-page {
    display: flex;
    justify-content: center;
    padding-top: 12vh;
  }
  .carte-erreur {
    width: min(520px, 100%);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--e3);
    padding: var(--e8);
    background: var(--white);
    border-radius: var(--r-carte);
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.04),
      0 0 0 1px var(--line);
  }
  .rond {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 22px;
    background: var(--sunk);
    color: var(--text2);
    margin-bottom: var(--e1);
  }
  .rond.ko {
    background: var(--err-bg);
    color: var(--err);
  }
  h1 {
    font-size: 22px;
    font-weight: 500;
  }
  p {
    margin: 0;
    font-size: 16px;
    color: var(--text2);
    text-wrap: pretty;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
    margin-top: var(--e3);
  }
  @media (max-width: 760px) {
    .erreur-page {
      padding: var(--e6) var(--e4);
    }
    .carte-erreur {
      padding: var(--e6) var(--e5);
    }
  }
</style>
