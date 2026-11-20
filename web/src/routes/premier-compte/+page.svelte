<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';

  const jeton = page.url.searchParams.get('jeton') ?? '';
  let valide = $state<boolean | null>(null);
  let identifiant = $state('');
  let motDePasse = $state('');
  let confirmation = $state('');
  let erreur = $state('');
  let fait = $state(false);

  $effect(() => {
    api
      .get(`/api/premier-compte/${encodeURIComponent(jeton)}`)
      .then(() => (valide = true))
      .catch(() => (valide = false));
  });

  async function creer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    if (motDePasse !== confirmation) {
      erreur = 'Les deux mots de passe sont différents.';
      return;
    }
    try {
      await api.post('/api/premier-compte', { jeton, identifiant, mot_de_passe: motDePasse });
      fait = true;
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<div class="carte">
  <h1>Premier compte</h1>
  {#if fait}
    <div class="bandeau ok">Compte administrateur créé.</div>
    <a href="/connexion">Se connecter</a>
  {:else if valide === false}
    <div class="bandeau erreur">Ce lien n'est plus valable (déjà utilisé, expiré, ou un compte existe déjà).</div>
    <a href="/connexion">Aller à la connexion</a>
  {:else if valide}
    <p class="discret">Création du compte administrateur. Le lien ne sert qu'une fois.</p>
    {#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
    <form onsubmit={creer}>
      <p><label for="id">Identifiant</label><input id="id" bind:value={identifiant} autocomplete="username" required /></p>
      <p><label for="mdp">Mot de passe (12 caractères au moins)</label><input id="mdp" type="password" minlength="12" bind:value={motDePasse} autocomplete="new-password" required /></p>
      <p><label for="mdp2">Confirmation</label><input id="mdp2" type="password" bind:value={confirmation} autocomplete="new-password" required /></p>
      <button>Créer le compte</button>
    </form>
  {/if}
</div>
