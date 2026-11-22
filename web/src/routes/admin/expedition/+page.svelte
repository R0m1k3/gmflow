<script lang="ts">
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { ExpeditionVue } from '#lib/types.ts';

  let vue = $state<ExpeditionVue | null>(null);
  let f = $state({ serveur: '', port: '465', securite: 'tls', identifiant: '', adresse: '', nom: '', mot_de_passe: '', alertes: '' });
  let effacerMdp = $state(false);
  let destinataire = $state('');
  let erreur = $state('');
  let info = $state('');
  let test = $state<'' | 'en_cours'>('');
  let arret: (() => void) | null = null;

  async function charger() {
    const v = await api.get<ExpeditionVue>('/api/admin/expedition');
    vue = v;
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
    if (!destinataire) destinataire = v.alerte_destinataires[0] ?? '';
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    return () => arret?.();
  });

  function choisirSecurite() {
    if (f.securite === 'tls' && f.port === '587') f.port = '465';
    if (f.securite === 'starttls' && f.port === '465') f.port = '587';
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    info = '';
    if (effacerMdp && !confirm('Supprimer le mot de passe enregistré ? Plus aucun mail ne pourra partir tant qu\'un nouveau n\'est pas saisi.')) return;
    const corps: Record<string, unknown> = {
      expedition: {
        serveur: f.serveur,
        port: Number(f.port),
        securite: f.securite,
        identifiant: f.identifiant,
        adresse: f.adresse,
        nom: f.nom.trim() || null
      },
      alerte_destinataires: f.alertes.split(/[\n,;]/).map((a) => a.trim()).filter(Boolean)
    };
    if (effacerMdp) corps.mot_de_passe = '';
    else if (f.mot_de_passe) corps.mot_de_passe = f.mot_de_passe;
    try {
      await api.put('/api/admin/expedition', corps);
      await charger();
      info = 'Réglages d\'envoi enregistrés.';
    } catch (err) {
      erreur = message(err);
    }
  }

  async function tester() {
    erreur = '';
    info = '';
    try {
      await api.post('/api/admin/expedition/test', { destinataire });
      test = 'en_cours';
      if (vue) vue.dernier_test = null;
      arret?.();
      arret = sonder(async () => {
        await charger();
        if (vue?.dernier_test) {
          test = '';
          return false;
        }
      });
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<div class="entete">
  <div>
    <h1>Envoi des mails</h1>
    <p class="sous">L'adresse de GMFlow qui envoie les factures à Dext ou Yooz et vous prévient en cas de problème.</p>
  </div>
</div>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if vue}
  <div class="bandeau {vue.expedition ? (vue.dernier_test?.ok ? 'ok' : vue.dernier_test ? 'erreur' : 'attention') : 'attention'}">
    {#if !vue.expedition}
      L'envoi n'est pas encore réglé : sans lui, aucune facture ne peut être déposée et aucune alerte ne part. Remplissez la section « Adresse d'envoi » ci-dessous.
    {:else if !vue.dernier_test}
      L'envoi est réglé mais jamais testé. Envoyez un mail de test (en bas de page) pour vous en assurer.
    {:else if vue.dernier_test.ok}
      L'envoi fonctionne : dernier test réussi {quand(vue.dernier_test.quand)}.
    {:else}
      Le dernier test a échoué {quand(vue.dernier_test.quand)}. Vérifiez le mot de passe et le serveur, enregistrez, puis refaites un test.
    {/if}
  </div>

  <form class="pile" onsubmit={enregistrer}>
    <fieldset>
      <legend>Adresse d'envoi</legend>
      <div class="champs">
        <div>
          <label for="ad">Adresse d'expédition</label>
          <input id="ad" type="email" bind:value={f.adresse} required placeholder="gmflow@exemple.fr" />
          <p class="aide">L'adresse qui apparaît comme expéditeur. Dext ou Yooz doit l'accepter.</p>
        </div>
        <div>
          <label for="nm">Nom affiché</label>
          <input id="nm" bind:value={f.nom} placeholder="GMFlow" />
          <p class="aide">Facultatif. Le nom vu par le destinataire.</p>
        </div>
      </div>
    </fieldset>

    <fieldset>
      <legend>Serveur d'envoi (SMTP)</legend>
      <div class="champs">
        <div>
          <label for="sv">Serveur</label>
          <input id="sv" bind:value={f.serveur} placeholder="mail.infomaniak.com" required />
          <p class="aide">Infomaniak : mail.infomaniak.com. Gmail : smtp.gmail.com.</p>
        </div>
        <div>
          <label for="se">Sécurité</label>
          <select id="se" bind:value={f.securite} onchange={choisirSecurite}>
            <option value="tls">TLS (port 465)</option>
            <option value="starttls">STARTTLS (port 587)</option>
          </select>
          <p class="aide">En cas de doute, gardez TLS.</p>
        </div>
        <div>
          <label for="po">Port</label>
          <input id="po" type="number" min="1" max="65535" bind:value={f.port} required />
          <p class="aide">Se règle tout seul selon la sécurité choisie.</p>
        </div>
        <div>
          <label for="id">Identifiant</label>
          <input id="id" bind:value={f.identifiant} autocomplete="off" />
          <p class="aide">Souvent l'adresse mail complète.</p>
        </div>
        <div>
          <label for="mdp">Mot de passe d'application</label>
          <input id="mdp" type="password" bind:value={f.mot_de_passe} autocomplete="new-password" disabled={effacerMdp}
            placeholder={vue.mot_de_passe_enregistre ? 'Enregistré (laisser vide pour le garder)' : ''} />
          <p class="aide">Conseillé plutôt que le mot de passe habituel. Il est chiffré et n'est jamais réaffiché.</p>
        </div>
      </div>
      {#if vue.mot_de_passe_enregistre}
        <label class="case"><input type="checkbox" bind:checked={effacerMdp} /> Supprimer le mot de passe enregistré</label>
      {/if}
    </fieldset>

    <fieldset>
      <legend>Alertes</legend>
      <label for="al">Qui prévenir en cas de problème</label>
      <textarea id="al" rows="2" bind:value={f.alertes} placeholder="direction@exemple.fr"></textarea>
      <p class="aide">Une adresse par ligne. Un mail part quand une boîte reste en erreur plus de 30 minutes, ou qu'un dépôt échoue ou s'interrompt.</p>
    </fieldset>
    <div class="actions"><button>Enregistrer</button></div>
  </form>

  <div class="section">
    <div class="titre"><h2>Tester l'envoi</h2></div>
    <div class="carte">
      <p class="aide intro">Envoie un mail de contrôle pour vérifier que le réglage ci-dessus fonctionne.</p>
      <div class="test">
        <div>
          <label for="dt">Envoyer un mail de test à</label>
          <input id="dt" type="email" bind:value={destinataire} placeholder="vous@exemple.fr" />
        </div>
        <button class="second" disabled={!vue.expedition || !destinataire || test === 'en_cours'} onclick={tester}>{test === 'en_cours' ? 'Envoi en cours…' : 'Envoyer le test'}</button>
      </div>
      {#if !vue.expedition}<p class="aide">Enregistrez d'abord les réglages d'envoi.</p>{/if}
      {#if test === 'en_cours'}
        <p class="discret" aria-live="polite">Envoi en cours, quelques secondes…</p>
      {:else if vue.dernier_test}
        <p aria-live="polite">
          <span class="etiquette {vue.dernier_test.ok ? 'ok' : 'ko'}">{vue.dernier_test.ok ? 'Réussi' : 'Échec'}</span>
          <span class="discret">le {quand(vue.dernier_test.quand)}{vue.dernier_test.message ? ` : ${vue.dernier_test.message}` : ''}</span>
        </p>
      {/if}
    </div>
  </div>
{:else if !erreur}
  <div class="charge" aria-busy="true"><span></span></div>
{/if}

<style>
  .intro { margin: 0 0 var(--e3); }
  .test { display: flex; gap: var(--e3); align-items: flex-end; flex-wrap: wrap; }
  .test > div { flex: 1; min-width: 220px; max-width: 420px; }
  .test > div label { margin-bottom: var(--e1); }
  @media (max-width: 760px) { .test > button { flex: 1 1 100%; } }
</style>
