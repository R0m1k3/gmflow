<script lang="ts">
  // Serveur API : adresse et jeton de connexion à l'API du serveur (jeton chiffré, jamais
  // réaffiché), essai de connexion fait par le worker.
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import type { ServeurApiVue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';

  let vue = $state<ServeurApiVue | null>(null);
  let erreur = $state('');
  let info = $state('');
  let edition = $state(false);
  let enCours = $state(false);
  let effacerJeton = $state(false);
  let f = $state({ adresse: '', jeton: '' });
  let essai = $state(false);
  let arret: (() => void) | null = null;

  function remplir(v: ServeurApiVue) {
    f = { adresse: v.serveur?.adresse ?? '', jeton: '' };
    effacerJeton = false;
  }
  async function charger() {
    vue = await api.get<ServeurApiVue>('/api/admin/serveur-api');
    if (!edition) remplir(vue);
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    return () => arret?.();
  });

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    info = '';
    enCours = true;
    const corps: Record<string, unknown> = { adresse: f.adresse.trim() };
    if (effacerJeton) corps.jeton = '';
    else if (f.jeton.trim()) corps.jeton = f.jeton.trim();
    try {
      await api.put('/api/admin/serveur-api', corps);
      edition = false;
      await charger();
      info = 'Connexion enregistrée. Faites un essai pour vérifier.';
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  async function supprimer() {
    if (!confirm('Supprimer l’adresse et le jeton du serveur API ?')) return;
    erreur = '';
    info = '';
    try {
      await api.del('/api/admin/serveur-api');
      edition = false;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }

  async function essayer() {
    erreur = '';
    info = '';
    try {
      await api.post('/api/admin/serveur-api/test', {});
      essai = true;
      if (vue) vue.dernier_test = null;
      arret?.();
      arret = sonder(async () => {
        await charger();
        if (vue?.dernier_test) {
          essai = false;
          return false;
        }
      });
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<section class="sujet" id="serveur-api">
  <div class="sujet-h"><h2>{RUBRIQUES.serveurApi}</h2></div>
  <p class="lead">Adresse et jeton de connexion à l’API de votre serveur.</p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  {#if !vue}
    {#if !erreur}<Chargement compact />{/if}
  {:else if edition || !vue.serveur}
    <form class="edition" onsubmit={enregistrer}>
      <div class="formulaire">
        <div class="plein">
          <Champ
            libelle="Adresse de l’API"
            type="url"
            bind:valeur={f.adresse}
            requis
            mono
            placeholder="https://api.mon-serveur.fr"
            aide="L’essai de connexion appelle cette adresse (GET)."
          />
        </div>
        <div class="plein">
          <Champ
            libelle="Jeton de connexion"
            type="password"
            bind:valeur={f.jeton}
            autocomplete="new-password"
            desactive={effacerJeton}
            placeholder={vue.jeton_enregistre ? 'Enregistré : laisser vide pour le garder' : ''}
            aide="Envoyé dans l’en-tête Authorization: Bearer. Chiffré, jamais réaffiché."
          />
        </div>
        {#if vue.jeton_enregistre}
          <label class="case plein"><input type="checkbox" bind:checked={effacerJeton} /> Supprimer le jeton enregistré</label>
        {/if}
      </div>
      <div class="form-f">
        <button class="btn principal" class:en-cours={enCours} disabled={enCours}>Enregistrer</button>
        {#if vue.serveur}
          <button type="button" class="btn fantome" onclick={() => { edition = false; if (vue) remplir(vue); }}>Annuler</button>
        {/if}
      </div>
    </form>
  {:else}
    {@const s = vue.serveur}
    <div class="filets">
      <div class="ligne bloc fixe">
        <div class="a num">{s.adresse}</div>
        <div class="b">
          {#if vue.jeton_enregistre}jeton enregistré{#if s.fin}{' '}(…<span class="num">{s.fin}</span>){/if}{:else}<span class="flag w">pas de jeton</span>{/if}
        </div>
        <div class="r">
          <button type="button" class="btn sm fantome" onclick={() => { if (vue) remplir(vue); edition = true; }}>Modifier</button>
          <button type="button" class="btn sm fantome" onclick={supprimer}>Supprimer</button>
        </div>
      </div>
      <div class="essai">
        <div>
          <button type="button" class="btn sm" class:en-cours={essai} disabled={essai} onclick={essayer}>Tester la connexion</button>
        </div>
        {#if essai}
          <span class="petit muted">Essai en cours…</span>
        {:else if vue.dernier_test}
          <span class="petit">
            <Etiquette ton={vue.dernier_test.ok ? 'ok' : 'erreur'}>{vue.dernier_test.ok ? 'Connexion réussie' : 'Connexion échouée'}</Etiquette>
            <span class="t2">le <span class="num">{quand(vue.dernier_test.quand)}</span>{vue.dernier_test.message ? ` : ${vue.dernier_test.message}` : ''}</span>
          </span>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .essai {
    padding: var(--e3) var(--e2);
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .ligne .a {
    white-space: normal;
    word-break: break-all;
  }
  .ligne .r {
    display: flex;
    gap: var(--e1);
  }
</style>
