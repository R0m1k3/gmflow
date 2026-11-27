<script lang="ts">
  /**
   * « Changer mon mot de passe » pour l'utilisateur connecté (`POST /api/moi/mot-de-passe`).
   * L'ancien mot de passe est exigé ; les autres appareils connectés à ce compte sont
   * déconnectés, celui-ci reste connecté. Composant autonome (page « Mon compte »).
   */
  import { ErreurApi, api, message } from '#lib/api.ts';
  import type { ChangementMdp } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Champ from './Champ.svelte';
  import { LONGUEUR_MIN, phrase, verifierChangement, type ErreursMdp } from './motDePasse.ts';

  let ancien = $state('');
  let nouveau = $state('');
  let confirmation = $state('');
  let erreurs = $state<ErreursMdp>({});
  let erreur = $state('');
  let envoi = $state(false);
  let fait = $state(false);

  let reste = $derived(LONGUEUR_MIN - [...nouveau].length);

  async function changer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    fait = false;
    erreurs = verifierChangement(ancien, nouveau, confirmation);
    if (Object.keys(erreurs).length > 0) return;
    envoi = true;
    try {
      await api.post('/api/moi/mot-de-passe', { ancien, nouveau } satisfies ChangementMdp);
      fait = true;
      ancien = nouveau = confirmation = '';
    } catch (err) {
      if (err instanceof ErreurApi && (err.champ === 'ancien' || err.champ === 'nouveau')) {
        erreurs = { [err.champ]: phrase(err.message) };
      } else if (err instanceof ErreurApi && err.code === 429) {
        erreur = 'Trop d’essais avec un mauvais mot de passe actuel. Patientez quelques minutes, puis réessayez.';
      } else {
        erreur = phrase(message(err));
      }
    } finally {
      envoi = false;
    }
  }
</script>

<form class="mon-mdp" onsubmit={changer} novalidate>
  {#if fait}
    <Bandeau ton="ok" titre="Mot de passe changé." role="status">
      Les autres appareils connectés à votre compte ont été déconnectés.
    </Bandeau>
  {/if}
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  <Champ
    libelle="Mot de passe actuel"
    type="password"
    bind:valeur={ancien}
    requis
    autocomplete="current-password"
    erreur={erreurs.ancien}
  />
  <Champ
    libelle="Nouveau mot de passe"
    type="password"
    bind:valeur={nouveau}
    requis
    autocomplete="new-password"
    aide={nouveau.length > 0 && reste > 0
      ? `Encore ${reste} caractère${reste > 1 ? 's' : ''} au moins.`
      : `${LONGUEUR_MIN} caractères au moins. Une phrase facile à retenir convient bien.`}
    erreur={erreurs.nouveau}
  />
  <Champ
    libelle="Confirmation du nouveau mot de passe"
    type="password"
    bind:valeur={confirmation}
    requis
    autocomplete="new-password"
    erreur={erreurs.confirmation}
  />
  <div>
    <button class="btn principal" class:en-cours={envoi} disabled={envoi}>Changer le mot de passe</button>
  </div>
</form>

<style>
  .mon-mdp {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    max-width: 480px;
  }
</style>
