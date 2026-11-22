<script lang="ts">
  // Envoi vers Dext / Yooz : adresse de collecte de chaque société (modifiée dans Sociétés),
  // adresse d'envoi (serveur SMTP de GMFlow), destinataires des alertes, essai d'envoi.
  import { tick } from 'svelte';
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES, nomOutil } from '#lib/libelles.ts';
  import type { ExpeditionVue, Societe } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import { portPourSecurite, serveursPour, type Securite } from './outils.ts';

  let {
    societes,
    modifierSociete,
    onchange
  }: { societes: Societe[]; modifierSociete: (id: number) => void; onchange: () => Promise<void> } = $props();

  let vue = $state<ExpeditionVue | null>(null);
  let erreur = $state('');
  let info = $state('');
  let edition = $state(false);
  let enCours = $state(false);
  let effacerMdp = $state(false);
  let f = $state({ serveur: '', port: '465', securite: 'tls' as string, identifiant: '', adresse: '', nom: '', mot_de_passe: '', alertes: '' });
  let destinataire = $state('');
  let essai = $state(false);
  let arret: (() => void) | null = null;

  function remplir(v: ExpeditionVue) {
    const e = v.expedition;
    f = {
      serveur: e?.serveur ?? '',
      port: String(e?.port ?? 465),
      securite: e?.securite ?? 'tls',
      identifiant: e?.identifiant ?? '',
      adresse: e?.adresse ?? '',
      nom: e?.nom ?? '',
      mot_de_passe: '',
      alertes: v.alerte_destinataires.join('\n')
    };
    effacerMdp = false;
  }
  async function charger() {
    vue = await api.get<ExpeditionVue>('/api/admin/expedition');
    if (!edition) remplir(vue);
    if (!destinataire) destinataire = vue.alerte_destinataires[0] ?? '';
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    return () => arret?.();
  });

  /** Ouvre le formulaire (bouton de la mise en route). */
  export async function ouvrir() {
    if (vue) remplir(vue);
    edition = true;
    await tick();
    document.querySelector<HTMLInputElement>('#envoi form input')?.focus({ preventScroll: true });
  }

  // Serveur proposé d'après l'adresse tant que le champ est vide (Gmail, Infomaniak).
  function proposer() {
    if (f.serveur.trim() || !f.adresse.includes('@')) return;
    const s = serveursPour(f.adresse).smtp;
    f.serveur = s.serveur;
    f.port = String(s.port);
    f.securite = s.securite;
    if (!f.identifiant.trim()) f.identifiant = f.adresse.trim();
  }
  // Le port suit la sécurité choisie, s'il valait le port habituel de l'autre.
  let securitePrecedente = 'tls';
  $effect(() => {
    const s = f.securite;
    if (s === securitePrecedente) return;
    securitePrecedente = s;
    if (f.port === '465' || f.port === '587') f.port = String(portPourSecurite(s as Securite));
  });

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    info = '';
    enCours = true;
    const corps: Record<string, unknown> = {
      expedition: {
        serveur: f.serveur.trim(),
        port: Number(f.port),
        securite: f.securite,
        identifiant: f.identifiant.trim(),
        adresse: f.adresse.trim(),
        nom: f.nom.trim() || null
      },
      alerte_destinataires: f.alertes
        .split(/[\n,;]/)
        .map((a) => a.trim())
        .filter(Boolean)
    };
    if (effacerMdp) corps.mot_de_passe = '';
    else if (f.mot_de_passe) corps.mot_de_passe = f.mot_de_passe;
    try {
      await api.put('/api/admin/expedition', corps);
      edition = false;
      await charger();
      info = 'Adresse d’envoi enregistrée. Faites un essai pour vérifier.';
      await onchange();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  async function essayer() {
    erreur = '';
    info = '';
    try {
      await api.post('/api/admin/expedition/test', { destinataire: destinataire.trim() });
      essai = true;
      if (vue) vue.dernier_test = null;
      arret?.();
      arret = sonder(async () => {
        await charger();
        if (vue?.dernier_test) {
          essai = false;
          void onchange();
          return false;
        }
      });
    } catch (err) {
      erreur = message(err);
    }
  }

  const SECURITES = [
    { valeur: 'tls', libelle: 'TLS (port 465)' },
    { valeur: 'starttls', libelle: 'STARTTLS (port 587)' }
  ];
  let actives = $derived(societes.filter((s) => !s.archivee));
</script>

<section class="sujet" id="envoi">
  <div class="sujet-h"><h2>{RUBRIQUES.envoi}</h2></div>
  <p class="lead">
    Une facture validée part par mail à l’adresse de collecte de sa société, depuis l’adresse d’envoi de GMFlow.
  </p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  <h3 class="sous-titre">Adresse de collecte de chaque société</h3>
  <div class="filets">
    {#each actives as s (s.id)}
      <div class="ligne bloc fixe">
        <div class="a">{s.raison_sociale}</div>
        <div class="b">
          {#if s.adresse_depot}
            {nomOutil(s.outil_depot)} : {s.adresse_depot}{#if s.depot_auto}{' '}· {RUBRIQUES.envoiAuto.toLowerCase()}{/if}
          {:else}
            <span class="flag w">Pas d’adresse de collecte : ses factures ne peuvent pas partir</span>
          {/if}
        </div>
        <div class="r">
          <button type="button" class="btn sm fantome" onclick={() => modifierSociete(s.id)}>{s.adresse_depot ? 'Modifier' : 'Indiquer l’adresse'}</button>
        </div>
      </div>
    {:else}
      <p class="muted petit vide-l">Aucune société pour l’instant.</p>
    {/each}
  </div>

  <h3 class="sous-titre">{RUBRIQUES.adresseEnvoi}</h3>
  {#if !vue}
    {#if !erreur}<Chargement compact />{/if}
  {:else if edition || !vue.expedition}
    <form class="edition" onsubmit={enregistrer}>
      <div class="formulaire">
        <Champ libelle="Adresse d’envoi" type="email" bind:valeur={f.adresse} requis placeholder="factures@frouard-distribution.fr" aide="Une adresse dédiée à GMFlow : elle envoie les factures et les alertes." />
        <Champ libelle="Nom affiché" bind:valeur={f.nom} placeholder="GMFlow" />
        <Champ
          libelle="Serveur d’envoi"
          bind:valeur={f.serveur}
          requis
          placeholder="mail.infomaniak.com"
          aide="Le serveur SMTP de cette adresse. Infomaniak : mail.infomaniak.com ; Gmail : smtp.gmail.com."
        />
        <div class="deux">
          <Champ libelle="Sécurité" type="select" options={SECURITES} bind:valeur={f.securite} />
          <Champ libelle="Port" bind:valeur={f.port} mono inputmode="numeric" requis />
        </div>
        <Champ libelle="Identifiant" bind:valeur={f.identifiant} autocomplete="off" aide="Souvent l’adresse elle-même." />
        <Champ
          libelle="Mot de passe d’application"
          type="password"
          bind:valeur={f.mot_de_passe}
          autocomplete="new-password"
          desactive={effacerMdp}
          placeholder={vue.mot_de_passe_enregistre ? 'Enregistré : laisser vide pour le garder' : ''}
          aide="Chiffré, jamais réaffiché."
        />
        {#if vue.mot_de_passe_enregistre}
          <label class="case plein"><input type="checkbox" bind:checked={effacerMdp} /> Supprimer le mot de passe enregistré</label>
        {/if}
        <div class="plein">
          <Champ
            libelle="Destinataires des alertes"
            type="textarea"
            bind:valeur={f.alertes}
            aide="Une adresse par ligne. Prévenus quand une boîte ne répond plus depuis 30 minutes ou qu’un envoi échoue."
          />
        </div>
      </div>
      <div class="form-f">
        <button class="btn principal" class:en-cours={enCours} disabled={enCours}>Enregistrer</button>
        {#if vue.expedition}
          <button type="button" class="btn fantome" onclick={() => { edition = false; if (vue) remplir(vue); }}>Annuler</button>
        {:else}
          <button type="button" class="btn fantome" onclick={proposer} disabled={!f.adresse.includes('@') || !!f.serveur.trim()}>Proposer le serveur d’après l’adresse</button>
        {/if}
      </div>
    </form>
  {:else}
    {@const x = vue.expedition}
    <div class="filets">
      <div class="ligne bloc fixe">
        <div class="a">{x.nom ? `${x.nom} <${x.adresse}>` : x.adresse}</div>
        <div class="b">
          par <span class="num">{x.serveur}:{x.port}</span>{#if !vue.mot_de_passe_enregistre}{' '}· <span class="flag w">pas de mot de passe</span>{/if}
          · alertes à {vue.alerte_destinataires.length ? vue.alerte_destinataires.join(', ') : 'personne'}
        </div>
        <div class="r"><button type="button" class="btn sm fantome" onclick={ouvrir}>Modifier</button></div>
      </div>
      <div class="essai">
        <form
          class="ligne-essai"
          onsubmit={(e) => {
            e.preventDefault();
            void essayer();
          }}
        >
          <label for="essai-envoi">Envoyer un mail d’essai à</label>
          <input id="essai-envoi" type="email" bind:value={destinataire} required />
          <button class="btn sm" class:en-cours={essai} disabled={essai || !destinataire.trim()}>Envoyer l’essai</button>
        </form>
        {#if essai}
          <span class="petit muted">Envoi en cours…</span>
        {:else if vue.dernier_test}
          <span class="petit">
            <Etiquette ton={vue.dernier_test.ok ? 'ok' : 'erreur'}>{vue.dernier_test.ok ? 'Essai réussi' : 'Essai échoué'}</Etiquette>
            <span class="t2">le <span class="num">{quand(vue.dernier_test.quand)}</span>{vue.dernier_test.message ? ` : ${vue.dernier_test.message}` : ''}</span>
          </span>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .deux {
    display: grid;
    grid-template-columns: 1fr 96px;
    gap: var(--e3);
  }
  .vide-l {
    padding: var(--e2);
    margin: 0;
  }
  .essai {
    padding: var(--e3) var(--e2);
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .ligne-essai {
    display: flex;
    align-items: center;
    gap: var(--e2);
    flex-wrap: wrap;
  }
  .ligne-essai input {
    width: auto;
    flex: 1;
    max-width: 280px;
    height: 28px;
  }
  .ligne .a {
    white-space: normal;
  }
</style>
