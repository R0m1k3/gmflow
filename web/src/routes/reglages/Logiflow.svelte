<script lang="ts">
  // Logiflow : adresse et clé de l'API externe (chiffrée, jamais réaffichée), essai de connexion
  // fait par le worker. Après chaque envoi d'une facture à Dext/Yooz, GMFlow écrit sa référence
  // et ses montants sur la livraison de ses BL dans Logiflow.
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import type { LogiflowVue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';

  let vue = $state<LogiflowVue | null>(null);
  let erreur = $state('');
  let info = $state('');
  let edition = $state(false);
  let enCours = $state(false);
  let effacerJeton = $state(false);
  let f = $state({ adresse: '', jeton: '' });
  let essai = $state(false);
  let arret: (() => void) | null = null;

  function remplir(v: LogiflowVue) {
    f = { adresse: v.serveur?.adresse ?? '', jeton: '' };
    effacerJeton = false;
  }
  async function charger() {
    vue = await api.get<LogiflowVue>('/api/admin/logiflow');
    if (!edition) remplir(vue);
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    return () => arret?.();
  });

  function suivreEssai() {
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
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    info = '';
    enCours = true;
    const corps: Record<string, unknown> = { adresse: f.adresse.trim() };
    if (effacerJeton) corps.jeton = '';
    else if (f.jeton.trim()) corps.jeton = f.jeton.trim();
    try {
      await api.put('/api/admin/logiflow', corps);
      edition = false;
      await charger();
      info = 'Connexion enregistrée : essai en cours.';
      suivreEssai();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  async function supprimer() {
    if (!confirm('Supprimer l’adresse et la clé de Logiflow ? Les factures envoyées ne seront plus reportées dans Logiflow.')) return;
    erreur = '';
    info = '';
    try {
      await api.del('/api/admin/logiflow');
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
      await api.post('/api/admin/logiflow/test', {});
      suivreEssai();
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<section class="sujet" id="logiflow">
  <div class="sujet-h"><h2>{RUBRIQUES.logiflow}</h2></div>
  <p class="lead">
    Quand une facture part vers Dext ou Yooz, GMFlow cherche la livraison de chacun de ses BL dans Logiflow (même n° de BL, même
    fournisseur par son code BO Phénix) et y écrit le n° de la facture, son HT et son TTC. La validation du rapprochement se fait
    toujours dans Logiflow.
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
            libelle="Adresse de Logiflow"
            type="url"
            bind:valeur={f.adresse}
            requis
            mono
            placeholder="https://logiflow.exemple.fr"
            aide="L’adresse du site Logiflow, sans /api à la fin."
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
            aide="À créer dans Logiflow, Paramètres > API externe, avec les droits « lecture + écriture ». Chiffrée, jamais réaffichée."
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
