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
      info = 'Expédition enregistrée.';
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

<h1>Expédition</h1>
<p class="discret">
  L'adresse dédiée qui envoie les factures aux adresses de dépôt Dext ou Yooz des structures, et les alertes (relève en
  échec, dépôt en erreur). Mot de passe d'application conseillé ; il est chiffré et n'est jamais réaffiché.
</p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if vue}
  <form class="carte" onsubmit={enregistrer}>
    <h2>Serveur SMTP</h2>
    <div class="champs">
      <div><label for="sv">Serveur</label><input id="sv" bind:value={f.serveur} placeholder="mail.infomaniak.com" required /></div>
      <div>
        <label for="se">Sécurité</label>
        <select id="se" bind:value={f.securite} onchange={choisirSecurite}>
          <option value="tls">TLS (port 465)</option>
          <option value="starttls">STARTTLS (port 587)</option>
        </select>
      </div>
      <div><label for="po">Port</label><input id="po" type="number" min="1" max="65535" bind:value={f.port} required /></div>
      <div><label for="id">Identifiant</label><input id="id" bind:value={f.identifiant} autocomplete="off" /></div>
      <div>
        <label for="mdp">Mot de passe</label>
        <input id="mdp" type="password" bind:value={f.mot_de_passe} autocomplete="new-password" disabled={effacerMdp}
          placeholder={vue.mot_de_passe_enregistre ? 'enregistré (laisser vide pour le garder)' : ''} />
      </div>
      <div><label for="ad">Adresse d'expédition</label><input id="ad" type="email" bind:value={f.adresse} required /></div>
      <div><label for="nm">Nom affiché</label><input id="nm" bind:value={f.nom} placeholder="GMFlow" /></div>
    </div>
    {#if vue.mot_de_passe_enregistre}
      <p><label><input type="checkbox" bind:checked={effacerMdp} /> Supprimer le mot de passe enregistré</label></p>
    {/if}
    <h2>Alertes</h2>
    <p>
      <label for="al">Destinataires des alertes (une adresse par ligne)</label>
      <textarea id="al" rows="2" bind:value={f.alertes}></textarea>
      <span class="discret">Une boîte en échec depuis plus de 30 minutes, un dépôt en erreur ou interrompu.</span>
    </p>
    <button>Enregistrer</button>
  </form>

  <h2>Tester l'envoi</h2>
  <div class="carte">
    <div class="actions">
      <label for="dt">Envoyer un mail de test à</label>
      <input id="dt" type="email" bind:value={destinataire} />
      <button class="second" disabled={!vue.expedition || !destinataire || test === 'en_cours'} onclick={tester}>Envoyer le test</button>
    </div>
    {#if test === 'en_cours'}
      <p class="discret">Envoi en cours…</p>
    {:else if vue.dernier_test}
      <p>
        <span class="etiquette {vue.dernier_test.ok ? 'ok' : 'ko'}">{vue.dernier_test.ok ? 'réussi' : 'échec'}</span>
        <span class="discret">le {quand(vue.dernier_test.quand)}{vue.dernier_test.message ? ` : ${vue.dernier_test.message}` : ''}</span>
      </p>
    {/if}
    {#if !vue.expedition}<p class="discret">Enregistrez d'abord le serveur SMTP.</p>{/if}
  </div>
{/if}

<style>
  .actions label { margin: 0; }
  .actions input { width: auto; flex: 1; max-width: 24rem; }
  form h2:first-child { margin-top: 0; }
</style>
