<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, ErreurApi, message } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Moi } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';

  let identifiant = $state('');
  let motDePasse = $state('');
  let erreur = $state<{ titre: string; texte: string } | null>(null);
  let envoi = $state(false);

  async function connecter(e: SubmitEvent) {
    e.preventDefault();
    envoi = true;
    erreur = null;
    try {
      session.moi = await api.post<Moi>('/api/connexion', { identifiant: identifiant.trim(), mot_de_passe: motDePasse });
      session.charge = true;
      // Retour à la page quittée par une session expirée (`?retour=`), jamais vers un autre site.
      const retour = page.url.searchParams.get('retour') ?? '';
      await goto(/^\/(?![/\\])/.test(retour) && !retour.startsWith('/connexion') ? retour : '/');
    } catch (err) {
      const code = err instanceof ErreurApi ? err.code : 0;
      if (code === 401) erreur = { titre: 'Identifiant ou mot de passe incorrect.', texte: 'Vérifiez la saisie, majuscules comprises.' };
      else if (code === 429) erreur = { titre: 'Trop d’essais.', texte: 'Attendez quelques minutes avant de réessayer.' };
      else if (code === 0 || code >= 500) erreur = { titre: 'GMFlow ne répond pas.', texte: 'Réessayez dans un instant ; si cela dure, prévenez l’administrateur.' };
      else erreur = { titre: 'Connexion impossible.', texte: message(err) };
      motDePasse = '';
    } finally {
      envoi = false;
    }
  }
</script>

<svelte:head><title>Connexion · GMFlow</title></svelte:head>

<div class="accueil">
  <div class="logo"><span class="marque" aria-hidden="true"></span>GMFlow</div>
  <div class="carte-seule">
    <h1>Se connecter</h1>
    <p>Factures, BL et mails des fournisseurs, au même endroit.</p>

    {#if erreur}<Bandeau ton="erreur" titre={erreur.titre}>{erreur.texte}</Bandeau>{/if}

    <form onsubmit={connecter}>
      <Champ libelle="Identifiant" bind:valeur={identifiant} requis autocomplete="username" />
      <Champ libelle="Mot de passe" type="password" bind:valeur={motDePasse} requis autocomplete="current-password" />
      <button class="btn principal plein" class:en-cours={envoi} disabled={envoi}>Se connecter</button>
    </form>
  </div>
  <p class="pied">Mot de passe oublié : demandez à un administrateur de GMFlow.</p>
</div>

<style>
  /* Écran seul (Sauge) : logo, puis une carte blanche (rayon 16, anneau #DDE2DA) sur le fond sauge. */
  .carte-seule {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    padding: var(--e8) var(--e6);
    background: var(--white);
    border-radius: var(--r-carte);
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.04),
      0 0 0 1px var(--line);
  }
  .carte-seule h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 500;
    line-height: 1.2;
  }
  .carte-seule > p {
    margin: calc(-1 * var(--e2)) 0 var(--e2);
    font-size: 16px;
    color: var(--text2);
    text-wrap: pretty;
  }
  .carte-seule :global(.bandeau) {
    margin: 0;
  }
  .carte-seule :global(.btn.plein) {
    width: 100%;
    height: 48px;
    font-size: 16px;
  }
  .carte-seule form :global(.btn.plein) {
    margin-top: var(--e2);
  }
  .pied {
    margin: var(--e5) var(--e2) 0;
    font-size: 15px;
    color: var(--muted);
  }
  @media (max-width: 420px) {
    .carte-seule {
      padding: var(--e6) var(--e5);
    }
  }
</style>
