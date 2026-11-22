<script lang="ts">
  import { goto } from '$app/navigation';
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
      await goto('/');
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
  <h1>Se connecter</h1>
  <p class="t2">Factures, BL et mails des fournisseurs, au même endroit.</p>

  {#if erreur}<Bandeau ton="erreur" titre={erreur.titre}>{erreur.texte}</Bandeau>{/if}

  <form onsubmit={connecter}>
    <Champ libelle="Identifiant" bind:valeur={identifiant} requis autocomplete="username" />
    <Champ libelle="Mot de passe" type="password" bind:valeur={motDePasse} requis autocomplete="current-password" />
    <button class="btn principal plein" class:en-cours={envoi} disabled={envoi}>Se connecter</button>
  </form>
  <p class="petit muted pied">Mot de passe oublié : demandez à un administrateur de GMFlow.</p>
</div>

<style>
  /* Écran seul, centré (le cadre <main class="seul"> limite déjà la largeur). */
  .accueil {
    padding: var(--e8) 0;
  }
  .logo {
    display: flex;
    align-items: center;
    gap: var(--e2);
    font-weight: 600;
    font-size: 15px;
    color: var(--black);
    margin-bottom: var(--e10);
  }
  .marque {
    width: 16px;
    height: 16px;
    background: var(--black);
    display: block;
    clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%, 0 60%, 60% 60%, 60% 40%, 0 40%);
  }
  h1 {
    margin-bottom: var(--e1);
  }
  p.t2 {
    margin-bottom: var(--e6);
  }
  .accueil > :global(.bandeau) {
    margin-bottom: var(--e4);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
  }
  .plein {
    width: 100%;
    margin-top: var(--e2);
  }
  .pied {
    margin-top: var(--e5);
  }
</style>
