<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';

  const jeton = page.url.searchParams.get('jeton') ?? '';
  let valide = $state<boolean | null>(null);
  let identifiant = $state('');
  let motDePasse = $state('');
  let confirmation = $state('');
  let erreur = $state('');
  let envoi = $state(false);
  let fait = $state(false);

  $effect(() => {
    if (!jeton) {
      valide = false;
      return;
    }
    api
      .get(`/api/premier-compte/${encodeURIComponent(jeton)}`)
      .then(() => (valide = true))
      .catch(() => (valide = false));
  });

  let tropCourt = $derived(motDePasse.length > 0 && motDePasse.length < 12);
  let different = $derived(confirmation.length > 0 && confirmation !== motDePasse);

  async function creer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    if (motDePasse.length < 12) {
      erreur = 'Le mot de passe doit faire 12 caractères au moins.';
      return;
    }
    if (motDePasse !== confirmation) {
      erreur = 'Les deux mots de passe sont différents.';
      return;
    }
    envoi = true;
    try {
      await api.post('/api/premier-compte', { jeton, identifiant: identifiant.trim(), mot_de_passe: motDePasse });
      fait = true;
    } catch (err) {
      erreur = message(err);
    } finally {
      envoi = false;
    }
  }
</script>

<svelte:head><title>Premier compte · GMFlow</title></svelte:head>

<div class="accueil">
  <div class="logo"><span class="marque" aria-hidden="true"></span>GMFlow</div>

  {#if fait}
    <h1>Compte créé</h1>
    <p class="t2">Le compte administrateur <b>{identifiant.trim()}</b> est prêt. Connectez-vous pour faire la mise en route.</p>
    <a class="btn principal plein" href="/connexion">Se connecter</a>
  {:else if valide === false}
    <h1>Lien plus valable</h1>
    <Bandeau ton="erreur" titre="Ce lien ne sert plus.">Il a déjà servi, il a expiré, ou un compte existe déjà.</Bandeau>
    <a class="btn plein" href="/connexion">Aller à la connexion</a>
  {:else if valide}
    <h1>Créer le premier compte</h1>
    <p class="t2">Ce compte administrateur fera la mise en route de GMFlow. Le lien ne sert qu’une fois.</p>
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
    <form onsubmit={creer}>
      <Champ libelle="Identifiant" bind:valeur={identifiant} requis autocomplete="username" aide="Une adresse mail ou un nom court." />
      <Champ
        libelle="Mot de passe"
        type="password"
        bind:valeur={motDePasse}
        requis
        autocomplete="new-password"
        aide={tropCourt ? `Encore ${12 - motDePasse.length} caractères au moins.` : '12 caractères au moins.'}
        aControler={tropCourt}
      />
      <Champ
        libelle="Confirmation"
        type="password"
        bind:valeur={confirmation}
        requis
        autocomplete="new-password"
        aControler={different ? 'Les deux mots de passe sont différents.' : false}
      />
      <button class="btn principal plein" class:en-cours={envoi} disabled={envoi}>Créer le compte</button>
    </form>
  {:else}
    <Chargement texte="Vérification du lien" />
  {/if}
</div>

