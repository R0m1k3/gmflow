<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, message } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Moi } from '#lib/types.ts';

  let identifiant = $state('');
  let motDePasse = $state('');
  let erreur = $state('');
  let envoi = $state(false);

  async function connecter(e: SubmitEvent) {
    e.preventDefault();
    envoi = true;
    erreur = '';
    try {
      session.moi = await api.post<Moi>('/api/connexion', { identifiant, mot_de_passe: motDePasse });
      session.charge = true;
      await goto('/');
    } catch (err) {
      erreur = (err as { code?: number }).code === 401 ? 'Identifiant ou mot de passe incorrect.' : message(err);
    } finally {
      envoi = false;
    }
  }
</script>

<div class="carte">
  <h1>GMFlow</h1>
  {#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
  <form onsubmit={connecter}>
    <p><label for="id">Identifiant</label><input id="id" bind:value={identifiant} autocomplete="username" required /></p>
    <p><label for="mdp">Mot de passe</label><input id="mdp" type="password" bind:value={motDePasse} autocomplete="current-password" required /></p>
    <button disabled={envoi}>Se connecter</button>
  </form>
</div>
