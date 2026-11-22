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

<div class="marque">GMFlow</div>
<p class="accroche">Courrier et factures des sociétés</p>
<div class="carte feuille">
  <h1>Créer le compte administrateur</h1>
  {#if fait}
    <div class="bandeau ok" role="status">Compte administrateur créé.</div>
    <a class="bouton" href="/connexion">Se connecter</a>
  {:else if valide === false}
    <div class="bandeau erreur">Ce lien n’est plus valable : il a déjà servi, il a expiré, ou un compte existe déjà.</div>
    <a href="/connexion">Aller à la connexion</a>
  {:else if valide}
    <p class="discret">Ce lien ne sert qu’une fois. Ce compte pourra ensuite créer les autres utilisateurs.</p>
    {#if erreur}<div class="bandeau erreur" role="alert">{erreur}</div>{/if}
    <form onsubmit={creer}>
      <div><label for="id">Identifiant</label><input id="id" bind:value={identifiant} autocomplete="username" autocapitalize="none" required /></div>
      <div><label for="mdp">Mot de passe</label><input id="mdp" type="password" minlength="12" bind:value={motDePasse} autocomplete="new-password" required aria-describedby="aide-mdp" /><p class="aide" id="aide-mdp">12 caractères au moins</p></div>
      <div><label for="mdp2">Confirmer le mot de passe</label><input id="mdp2" type="password" bind:value={confirmation} autocomplete="new-password" required /></div>
      <button>Créer le compte</button>
    </form>
  {:else}
    <div class="charge" aria-label="Vérification du lien"><span></span></div>
  {/if}
</div>

<style>
  .marque { font-family: var(--serif); font-weight: 600; font-size: 2rem; line-height: 1.1; margin: 0 0 var(--e1); }
  .accroche { color: var(--gris); margin: 0 0 var(--e6); }
  .feuille { border-top: 3px double var(--trait-fort); padding-top: var(--e6); }
  form { display: grid; gap: var(--e4); }
  form > div { margin: 0; }
  form button { width: 100%; margin-top: var(--e2); }
</style>
