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

<div class="marque">GMFlow</div>
<p class="accroche">Courrier et factures des sociétés</p>
<div class="carte feuille">
  <h1>Se connecter</h1>
  {#if erreur}<div class="bandeau erreur" role="alert">{erreur}</div>{/if}
  <form onsubmit={connecter}>
    <div><label for="id">Identifiant</label><input id="id" bind:value={identifiant} autocomplete="username" autocapitalize="none" required /></div>
    <div><label for="mdp">Mot de passe</label><input id="mdp" type="password" bind:value={motDePasse} autocomplete="current-password" required /></div>
    <button disabled={envoi}>{envoi ? 'Connexion…' : 'Se connecter'}</button>
  </form>
</div>

<style>
  .marque { font-family: var(--serif); font-weight: 600; font-size: 2rem; line-height: 1.1; margin: 0 0 var(--e1); }
  .accroche { color: var(--gris); margin: 0 0 var(--e6); }
  .feuille { border-top: 3px double var(--trait-fort); padding-top: var(--e6); }
  form { display: grid; gap: var(--e4); }
  form > div { margin: 0; }
  form button { width: 100%; margin-top: var(--e2); }
</style>
