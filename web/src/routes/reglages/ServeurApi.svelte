<script lang="ts">
  // Serveur du magasin (Apiflow) : adresse et clé d'API (chiffrée, jamais réaffichée), essai de
  // connexion et copie des fournisseurs du magasin faits par le worker (toutes les 12 h, ou à la
  // demande). Les fiches fournisseurs sans code BO Phénix proposent ensuite les codes du magasin.
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
  let copie = $state(false);
  let arretCopie: (() => void) | null = null;

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
    return () => {
      arret?.();
      arretCopie?.();
    };
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
      info = 'Connexion enregistrée : la copie des fournisseurs du magasin démarre.';
      suivreCopie();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  async function supprimer() {
    if (!confirm('Supprimer l’adresse, la clé et la copie des fournisseurs du magasin ?')) return;
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

  function suivreCopie() {
    copie = true;
    if (vue) vue.synchro = null;
    arretCopie?.();
    arretCopie = sonder(async () => {
      await charger();
      if (vue?.synchro) {
        copie = false;
        return false;
      }
    }, 3000);
  }
  async function copier() {
    erreur = '';
    info = '';
    try {
      await api.post('/api/admin/serveur-api/synchro', {});
      suivreCopie();
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
  <p class="lead">
    L’API Apiflow du magasin : GMFlow y copie la liste des fournisseurs toutes les 12 heures, pour proposer leur code BO Phénix sur
    les fiches fournisseurs.
  </p>
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
            placeholder="https://api.ffnancy.fr"
            aide="La racine du serveur, sans /api à la fin."
          />
        </div>
        <div class="plein">
          <Champ
            libelle="Clé d’API"
            type="password"
            bind:valeur={f.jeton}
            autocomplete="new-password"
            desactive={effacerJeton}
            placeholder={vue.jeton_enregistre ? 'Enregistrée : laisser vide pour la garder' : ''}
            aide="Une des clés API_KEYS d’Apiflow. Chiffrée, jamais réaffichée."
          />
        </div>
        {#if vue.jeton_enregistre}
          <label class="case plein"><input type="checkbox" bind:checked={effacerJeton} /> Supprimer la clé enregistrée</label>
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
          {#if vue.jeton_enregistre}clé enregistrée{#if s.fin}{' '}(…<span class="num">{s.fin}</span>){/if}{:else}<span class="flag w">pas de clé</span>{/if}
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
      <div class="essai">
        <div class="ligne-copie">
          <span>
            Fournisseurs du magasin copiés : <span class="num">{vue.nombre}</span>
            {#if vue.synchro && !copie}
              <span class="t2 petit">· dernière copie le <span class="num">{quand(vue.synchro.quand)}</span></span>
            {/if}
          </span>
          <button type="button" class="btn sm" class:en-cours={copie} disabled={copie} onclick={copier}>Mettre à jour maintenant</button>
        </div>
        {#if copie}
          <span class="petit muted">Copie en cours…</span>
        {:else if vue.synchro && !vue.synchro.ok}
          <span class="petit"><Etiquette ton="erreur">Copie échouée</Etiquette> <span class="t2">{vue.synchro.message ?? ''}</span></span>
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
  .ligne-copie {
    display: flex;
    align-items: center;
    gap: var(--e3);
    flex-wrap: wrap;
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
