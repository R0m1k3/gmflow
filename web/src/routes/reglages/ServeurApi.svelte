<script lang="ts">
  // Serveur du magasin (Apiflow) : adresse et clé d'API (chiffrée, jamais réaffichée), essai de
  // connexion et copie des fournisseurs du magasin faits par le worker (toutes les 12 h, ou à la
  // demande). Les fiches fournisseurs sans code BO Phénix proposent ensuite les codes du magasin.
  import { api, message, sonder } from '#lib/api.ts';
    import { RUBRIQUES } from '#lib/libelles.ts';
  import type { ServeurApiVue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import { leA } from './outils.ts';

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
    <div class="carte-r">
      <div class="rang">
        <div class="g">
          <span class="t num">{s.adresse}</span>
          <span class="s">
            {#if vue.jeton_enregistre}clé enregistrée{#if s.fin}{' '}(…<span class="num">{s.fin}</span>){/if}{:else}<span class="flag w">pas de clé</span>{/if}
          </span>
          {#if essai}
            <span class="resultat">Essai en cours…</span>
          {:else if vue.dernier_test}
            <span class="resultat">
              <Etiquette ton={vue.dernier_test.ok ? 'ok' : 'erreur'}>
                <Icone nom={vue.dernier_test.ok ? 'coche' : 'alerte'} />{vue.dernier_test.ok ? 'Connexion réussie' : 'Connexion échouée'}
              </Etiquette>
              <span class="num">le {leA(vue.dernier_test.quand)}</span>{vue.dernier_test.message ? ` : ${vue.dernier_test.message}` : ''}
            </span>
          {/if}
        </div>
        <div class="d">
          <button type="button" class="btn" class:en-cours={essai} disabled={essai} onclick={essayer}>Tester la connexion</button>
          <button type="button" class="lien-a" onclick={() => { if (vue) remplir(vue); edition = true; }}>Modifier</button>
          <button type="button" class="lien-a" onclick={supprimer}>Supprimer</button>
        </div>
      </div>
      <div class="rang">
        <div class="g">
          <span class="s15">Fournisseurs du magasin copiés : <span class="num">{vue.nombre}</span></span>
          {#if copie}
            <span class="resultat">Copie en cours…</span>
          {:else if vue.synchro && !vue.synchro.ok}
            <span class="resultat"><Etiquette ton="erreur"><Icone nom="alerte" />Copie échouée</Etiquette>{vue.synchro.message ?? ''}</span>
          {:else if vue.synchro}
            <span class="resultat num">dernière copie le {leA(vue.synchro.quand)}</span>
          {/if}
        </div>
        <div class="d">
          <button type="button" class="btn" class:en-cours={copie} disabled={copie} onclick={copier}>Mettre à jour maintenant</button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .s15 {
    font-size: 15px;
  }
  .resultat {
    display: flex;
    align-items: center;
    gap: var(--e2);
    flex-wrap: wrap;
    font-size: 14px;
    color: var(--text2);
  }
  .t {
    word-break: break-all;
  }
</style>
