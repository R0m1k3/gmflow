<script lang="ts">
  // Logiflow : adresse et clé de l'API externe (chiffrée, jamais réaffichée), essai de connexion
  // fait par le worker. Toutes les heures, GMFlow retrouve la livraison de chaque BL ; après
  // l'envoi d'une facture à Dext/Yooz, il écrit sa référence, ses montants et son échéance sur
  // la livraison de ses BL.
  import { api, message, sonder } from '#lib/api.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import type { LogiflowVue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import { leA } from './outils.ts';

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

  let suivi = $state(false);
  async function verifier() {
    erreur = '';
    info = '';
    try {
      await api.post('/api/admin/logiflow/suivi', {});
      suivi = true;
      arret?.();
      arret = sonder(async () => {
        await charger();
        if (vue?.dernier_suivi && vue.dernier_suivi.ok !== null) {
          suivi = false;
          return false;
        }
      });
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
    Toutes les heures, GMFlow retrouve dans Logiflow la livraison de chaque BL reçu (même n° de BL, même fournisseur par son code
    BO Phénix) et l’affiche sur la fiche du BL. Quand une facture liée à ces BL part vers Dext ou Yooz, GMFlow écrit sur chaque
    livraison le n° de la facture, sa part du HT et du TTC (au prorata du montant de chaque BL) et son échéance. Une livraison
    pas encore dans Logiflow est reprise à la vérification suivante. La validation du rapprochement se fait toujours dans
    Logiflow.
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
          {#if suivi || vue.dernier_suivi?.ok === null}
            <span class="resultat">Vérification des BL en cours…</span>
          {:else if vue.dernier_suivi}
            <span class="resultat">
              <Etiquette ton={vue.dernier_suivi.ok ? 'ok' : 'erreur'}>
                <Icone nom={vue.dernier_suivi.ok ? 'coche' : 'alerte'} />{vue.dernier_suivi.ok ? 'Dernière vérification' : 'Vérification échouée'}
              </Etiquette>
              <span class="num">le {leA(vue.dernier_suivi.quand)}</span>{vue.dernier_suivi.message ? ` : ${vue.dernier_suivi.message}` : ''}
            </span>
          {/if}
        </div>
        <div class="d">
          <button type="button" class="btn" class:en-cours={essai} disabled={essai} onclick={essayer}>Tester la connexion</button>
          <button type="button" class="btn" class:en-cours={suivi} disabled={suivi} onclick={verifier}>Vérifier maintenant</button>
          <button type="button" class="lien-a" onclick={() => { if (vue) remplir(vue); edition = true; }}>Modifier</button>
          <button type="button" class="lien-a" onclick={supprimer}>Supprimer</button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
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
