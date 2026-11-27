<script lang="ts">
  // « Mon compte » : ouvert à tout utilisateur connecté depuis le menu du compte (barre du haut).
  import { session } from '#lib/session.svelte.ts';
  import { libelleRole } from '#lib/libelles.ts';
  import MonMotDePasse from '#lib/ui/MonMotDePasse.svelte';
</script>

<svelte:head><title>Mon compte · GMFlow</title></svelte:head>

<div class="page compte">
  <div class="page-h">
    <h1>Mon compte</h1>
  </div>
  {#if session.moi}
    <p class="muted">{session.moi.identifiant} · {libelleRole(session.moi.role)}</p>
  {/if}
  <section class="compte-mdp">
    <h2>Changer mon mot de passe</h2>
    <MonMotDePasse />
  </section>
</div>

<style>
  .compte-mdp {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    padding: var(--e6);
    max-width: 560px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-carte);
  }
  .compte-mdp h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 500;
  }
</style>
