<script lang="ts">
  import { api, ErreurApi, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { libelleTache } from '#lib/documents.ts';
  import type { Appels, EtatClaude, ExpediteurMarque, Reglages, ReglagesVue, Seance } from '#lib/types.ts';

  let etat = $state<EtatClaude | null>(null);
  let jeton = $state('');
  let erreur = $state('');
  let seanceId = $state<string | null>(null);
  let seance = $state<Seance | null>(null);
  let code = $state('');
  let codeEnvoye = $state(false);

  async function chargerEtat() {
    etat = await api.get<EtatClaude>('/api/admin/claude');
    return etat.etat === 'en_test';
  }
  $effect(() => {
    chargerEtat().catch((e) => (erreur = message(e)));
  });
  function suivreEtat() {
    sonder(chargerEtat);
  }

  async function coller(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    try {
      await api.put('/api/admin/claude/jeton', { jeton });
      jeton = '';
      await chargerEtat();
      suivreEtat();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function tester() {
    erreur = '';
    try {
      await api.post('/api/admin/claude/test');
      if (etat) etat = { ...etat, etat: 'en_test' };
      suivreEtat();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function supprimer() {
    if (!confirm('Supprimer le jeton Claude ?')) return;
    await api.del('/api/admin/claude/jeton');
    await chargerEtat();
  }

  // Clé API Anthropic : alternative au setup token (facturée à l'usage par Anthropic).
  let cle = $state('');
  let infoAcces = $state('');
  async function enregistrerCle(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    infoAcces = '';
    try {
      await api.put('/api/admin/claude/cle-api', { cle });
      cle = '';
      await chargerEtat();
      infoAcces = etat?.acces?.mode === 'cle_api' ? 'Clé enregistrée, test en cours.' : 'Clé enregistrée. Choisissez « Clé API » pour l\'utiliser.';
      if (etat?.etat === 'en_test') suivreEtat();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function supprimerCle() {
    if (!confirm('Supprimer la clé API ? GMFlow reviendra au setup token.')) return;
    erreur = '';
    try {
      await api.del('/api/admin/claude/cle-api');
      await chargerEtat();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function choisirAcces(mode: string) {
    erreur = '';
    infoAcces = '';
    try {
      await api.put('/api/admin/claude/acces', { mode });
      await chargerEtat();
      suivreEtat();
    } catch (err) {
      erreur = message(err);
      await chargerEtat();
    }
  }

  let marques = $state<ExpediteurMarque[]>([]);
  async function chargerMarques() {
    marques = await api.get<ExpediteurMarque[]>('/api/admin/expediteurs-marques');
  }
  $effect(() => {
    chargerMarques().catch((e) => (erreur = message(e)));
  });
  async function retirerMarque(m: ExpediteurMarque) {
    if (!confirm(`Ne plus classer automatiquement ${m.domaine ?? m.adresse} en publicité ?`)) return;
    erreur = '';
    try {
      await api.del(`/api/admin/expediteurs-marques/${m.id}`);
      await chargerMarques();
    } catch (err) {
      erreur = message(err);
    }
  }

  const finals = ['terminee', 'erreur', 'expiree'];
  let arreterSuivi: (() => void) | null = null;
  function suivreSeance() {
    arreterSuivi?.();
    arreterSuivi = sonder(async () => {
      seance = await api.get<Seance>(`/api/admin/claude/seances/${seanceId}`);
      if (finals.includes(seance.etat)) {
        await chargerEtat();
        if (seance.etat === 'terminee') suivreEtat();
        return false;
      }
      return true;
    });
  }
  // Séance ouverte (page fermée ou rechargée) : on la reprend avec son lien et le champ du code.
  async function reprendre(): Promise<boolean> {
    const s = await api.get<(Seance & { id: string }) | null>('/api/admin/claude/seances/en-cours');
    if (!s) return false;
    seanceId = s.id;
    seance = { etat: s.etat, url: s.url, message: s.message };
    codeEnvoye = s.etat === 'code_recu';
    suivreSeance();
    return true;
  }
  $effect(() => {
    reprendre().catch(() => {});
    return () => arreterSuivi?.();
  });
  async function lancer() {
    erreur = '';
    codeEnvoye = false;
    code = '';
    try {
      seanceId = (await api.post<{ id: string }>('/api/admin/claude/seances')).id;
      seance = { etat: 'demarrage', url: null, message: null };
      suivreSeance();
    } catch (err) {
      // Une séance est déjà ouverte : on la reprend au lieu de bloquer.
      if (err instanceof ErreurApi && err.code === 409 && (await reprendre().catch(() => false))) return;
      erreur = message(err);
    }
  }
  async function annuler() {
    erreur = '';
    try {
      await api.post(`/api/admin/claude/seances/${seanceId}/annuler`);
      seance = await api.get<Seance>(`/api/admin/claude/seances/${seanceId}`);
      arreterSuivi?.();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function envoyerCode(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    try {
      await api.post(`/api/admin/claude/seances/${seanceId}/code`, { code });
      codeEnvoye = true;
    } catch (err) {
      erreur = message(err);
    }
  }

  let vue = $state<ReglagesVue | null>(null);
  let reglages = $state<Reglages | null>(null);
  let infoReglages = $state('');
  let appels = $state<Appels | null>(null);
  let pageAppels = $state(1);
  const taches = ['extraction_texte', 'ocr_scan', 'nouvel_essai', 'identification_fournisseur'] as const;
  const tachesTri = ['tri_mail', 'brouillon_reponse'] as const;

  async function chargerReglages() {
    vue = await api.get<ReglagesVue>('/api/admin/claude/reglages');
    // Copie des seuls réglages (sans les compteurs ni les listes de choix).
    const { appels_lecture_du_jour: _l, appels_tri_du_jour: _t, modeles: _m, efforts: _e, ...r } = vue;
    reglages = JSON.parse(JSON.stringify(r)) as Reglages;
  }
  async function chargerAppels() {
    appels = await api.get<Appels>(`/api/admin/claude/appels${pageAppels > 1 ? `?page=${pageAppels}` : ''}`);
  }
  $effect(() => {
    chargerReglages().catch((e) => (erreur = message(e)));
    chargerAppels().catch((e) => (erreur = message(e)));
  });
  async function enregistrerReglages(e: SubmitEvent) {
    e.preventDefault();
    if (!reglages) return;
    erreur = '';
    infoReglages = '';
    try {
      await api.put('/api/admin/claude/reglages', {
        ...reglages,
        quota_lecture_jour: Number(reglages.quota_lecture_jour),
        quota_tri_jour: Number(reglages.quota_tri_jour),
        tri_depuis_jours: Number(reglages.tri_depuis_jours)
      });
      await chargerReglages();
      infoReglages = 'Réglages enregistrés.';
    } catch (err) {
      erreur = message(err);
    }
  }
  function allerAppels(n: number) {
    pageAppels = n;
    chargerAppels().catch((e) => (erreur = message(e)));
  }
  let pagesAppels = $derived(appels ? Math.max(1, Math.ceil(appels.total / 50)) : 1);
  function duree(ms: number) {
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} s`;
  }

  const libelles: Record<string, [string, string]> = {
    valide: ['valide', 'ok'],
    erreur: ['en erreur', 'ko'],
    absent: ['aucun jeton', ''],
    en_test: ['test en cours…', 'att']
  };
  const etapes: Record<string, string> = {
    demarrage: 'Démarrage de Claude CLI…',
    attente_code: 'Ouvrez le lien, connectez-vous, puis collez le code obtenu.',
    code_recu: 'Code envoyé, en attente du jeton…',
    terminee: 'Jeton enregistré. Test en cours.',
    erreur: 'La séance a échoué.',
    expiree: 'Aucun code reçu à temps : relancez la séance.'
  };
</script>

<h1>Claude</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}

{#if etat}
  {@const [texte, classe] = libelles[etat.etat] ?? [etat.etat, '']}
  <div class="carte">
    <div class="actions">
      <span>
        {etat.acces?.mode === 'cle_api' ? 'Clé API' : 'Setup token'} :
        <span class="etiquette {classe}">{etat.etat === 'absent' && etat.acces?.mode === 'cle_api' ? 'aucune clé' : texte}</span>
      </span>
      {#if etat.acces?.mode === 'cle_api'}
        {#if etat.acces.cle_api_fin}<span class="discret">se termine par …{etat.acces.cle_api_fin}</span>{/if}
      {:else if etat.fin}<span class="discret">se termine par …{etat.fin}</span>{/if}
      <span class="discret">testé {quand(etat.teste_le)}</span>
      <span class="discret">dernière utilisation {quand(etat.utilise_le)}</span>
      <span style="flex:1"></span>
      {#if etat.etat !== 'absent'}
        <button class="second" onclick={tester}>Tester</button>
      {/if}
      {#if etat.acces?.jeton ?? etat.etat !== 'absent'}
        <button class="danger" onclick={supprimer}>Supprimer le jeton</button>
      {/if}
    </div>
    {#if etat.etat === 'erreur' && etat.message}<p class="discret">{etat.message}</p>{/if}
  </div>
{/if}

<h2>Générer un jeton depuis GMFlow</h2>
<div class="carte">
  <p class="discret">Lance <code>claude setup-token</code> sur le serveur. Il faut un abonnement Claude.</p>
  {#if seance && !finals.includes(seance.etat)}
    <p>{etapes[seance.etat]}</p>
    {#if seance.url}
      <p><a href={seance.url} target="_blank" rel="noopener noreferrer">Ouvrir la page de connexion Claude</a></p>
      {#if seance.etat === 'attente_code' && !codeEnvoye}
        <form class="actions" onsubmit={envoyerCode}>
          <input bind:value={code} placeholder="Code de connexion" required autocomplete="off" style="max-width:360px" />
          <button>Envoyer le code</button>
        </form>
      {/if}
    {/if}
    <p><button class="second" onclick={annuler}>Annuler la séance</button></p>
  {:else}
    {#if seance}
      {#if seance.message === 'annulée'}
        <p class="discret">Séance annulée.</p>
      {:else}
        <div class="bandeau {seance.etat === 'terminee' ? 'ok' : 'erreur'}">{etapes[seance.etat]}{seance.message ? ` ${seance.message}` : ''}</div>
      {/if}
    {/if}
    <button onclick={lancer}>Lancer la séance</button>
  {/if}
</div>

<h2>Ou coller un jeton</h2>
<form class="carte actions" onsubmit={coller}>
  <input type="password" bind:value={jeton} placeholder="sk-ant-oat01-…" required autocomplete="off" style="max-width:420px" />
  <button>Enregistrer et tester</button>
  <span class="discret">Généré ailleurs avec <code>claude setup-token</code>. Il n'est plus jamais affiché ensuite.</span>
</form>

<h2>Ou utiliser une clé API Anthropic</h2>
<div class="carte">
  {#if infoAcces}<div class="bandeau ok">{infoAcces}</div>{/if}
  <p class="discret">
    Alternative au setup token : une clé de la console Anthropic, facturée à l'usage. GMFlow utilise l'une ou l'autre, jamais les deux.
  </p>
  {#if etat?.acces}
    <div class="actions">
      <span>Accès utilisé :</span>
      <label class="choix"><input type="radio" name="acces" checked={etat.acces.mode === 'jeton'} onchange={() => choisirAcces('jeton')} /> setup token</label>
      <label class="choix">
        <input type="radio" name="acces" checked={etat.acces.mode === 'cle_api'} disabled={!etat.acces.cle_api} onchange={() => choisirAcces('cle_api')} />
        clé API{etat.acces.cle_api ? ` (…${etat.acces.cle_api_fin ?? ''})` : ' (aucune)'}
      </label>
      <span style="flex:1"></span>
      {#if etat.acces.cle_api}<button class="danger" onclick={supprimerCle}>Supprimer la clé</button>{/if}
    </div>
  {/if}
  <form class="actions" style="margin-top:0.8rem" onsubmit={enregistrerCle}>
    <input type="password" bind:value={cle} placeholder="sk-ant-api03-…" required autocomplete="off" style="max-width:420px" />
    <button>{etat?.acces?.cle_api ? 'Remplacer la clé' : 'Enregistrer la clé'}</button>
    <span class="discret">Elle n'est plus jamais affichée ensuite.</span>
  </form>
</div>

<h2>Lecture des documents</h2>
{#if vue && reglages}
  <form class="carte" onsubmit={enregistrerReglages}>
    {#if infoReglages}<div class="bandeau ok">{infoReglages}</div>{/if}
    <table class="reglages">
      <thead><tr><th>Tâche</th><th>Modèle</th><th>Effort</th></tr></thead>
      <tbody>
        {#each taches as t (t)}
          <tr>
            <td>{libelleTache[t]}</td>
            <td>
              <select bind:value={reglages[t].modele} aria-label="Modèle : {libelleTache[t]}">
                {#each vue.modeles as m (m)}<option value={m}>{m}</option>{/each}
              </select>
            </td>
            <td>
              <select bind:value={reglages[t].effort} aria-label="Effort : {libelleTache[t]}">
                {#each vue.efforts as ef (ef)}<option value={ef}>{ef}</option>{/each}
              </select>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="champs">
      <div>
        <label for="quota">Appels de lecture par jour (au plus)</label>
        <input id="quota" type="number" min="0" bind:value={reglages.quota_lecture_jour} />
        <div class="discret">{vue.appels_lecture_du_jour} aujourd'hui. Au-delà, les documents attendent le lendemain.</div>
      </div>
      <div class="case">
        <label><input type="checkbox" bind:checked={reglages.lecture_auto} /> Lecture automatique des nouvelles pièces jointes</label>
        <div class="discret">Coupée, rien n'est lu, même sans Claude. « Relire » sur un document marche toujours.</div>
      </div>
    </div>
    <button>Enregistrer</button>
  </form>
{/if}

<h2>Tri des mails et réponses</h2>
{#if vue && reglages}
  <form class="carte" onsubmit={enregistrerReglages}>
    <table class="reglages">
      <thead><tr><th>Tâche</th><th>Modèle</th><th>Effort</th></tr></thead>
      <tbody>
        {#each tachesTri as t (t)}
          <tr>
            <td>{libelleTache[t]}</td>
            <td>
              <select bind:value={reglages[t].modele} aria-label="Modèle : {libelleTache[t]}">
                {#each vue.modeles as m (m)}<option value={m}>{m}</option>{/each}
              </select>
            </td>
            <td>
              <select bind:value={reglages[t].effort} aria-label="Effort : {libelleTache[t]}">
                {#each vue.efforts as ef (ef)}<option value={ef}>{ef}</option>{/each}
              </select>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="champs">
      <div>
        <label for="qtri">Appels de tri par jour (au plus)</label>
        <input id="qtri" type="number" min="0" bind:value={reglages.quota_tri_jour} />
        <div class="discret">
          {vue.appels_tri_du_jour} aujourd'hui. Les mails reconnus par les en-têtes (listes, no-reply, expéditeurs marqués) ne coûtent rien.
        </div>
      </div>
      <div>
        <label for="dtri">Trier les mails reçus depuis (jours)</label>
        <input id="dtri" type="number" min="1" max="365" bind:value={reglages.tri_depuis_jours} />
        <div class="discret">Les mails plus anciens restent non triés.</div>
      </div>
      <div class="case">
        <label><input type="checkbox" bind:checked={reglages.tri_auto} /> Tri automatique des nouveaux mails</label>
        <div class="discret">Les brouillons ne sont jamais automatiques : ils se demandent mail par mail.</div>
      </div>
    </div>
    <button>Enregistrer</button>
  </form>
{/if}

<h3>Expéditeurs toujours classés en publicité</h3>
<p class="discret">Ajoutés depuis la fiche d'un mail (« Toujours publicité »). Leurs mails sont classés sans appel à Claude et supprimés après 90 jours.</p>
<table>
  <thead><tr><th>Adresse ou domaine</th><th>Ajouté</th><th></th></tr></thead>
  <tbody>
    {#each marques as m (m.id)}
      <tr>
        <td>{m.domaine ? `tout le domaine ${m.domaine}` : m.adresse}</td>
        <td class="discret">{quand(m.cree_le)}{m.cree_par ? ` par ${m.cree_par}` : ''}</td>
        <td><button class="second" onclick={() => retirerMarque(m)}>Retirer</button></td>
      </tr>
    {:else}
      <tr><td colspan="3" class="discret">Aucun expéditeur marqué.</td></tr>
    {/each}
  </tbody>
</table>

<h2>Appels à Claude</h2>
{#if appels}
  {#if appels.stats.length}
    <table class="stats">
      <thead><tr><th>Sur 30 jours</th><th>Modèle</th><th>Appels</th><th>Réussis</th><th>Durée moyenne</th></tr></thead>
      <tbody>
        {#each appels.stats as s (s.tache + s.modele)}
          <tr>
            <td>{libelleTache[s.tache] ?? s.tache}</td>
            <td>{s.modele}</td>
            <td class="nombre">{s.n}</td>
            <td class="nombre">{s.reussis} ({Math.round((100 * s.reussis) / Math.max(1, s.n))} %)</td>
            <td class="nombre">{duree(s.duree_moyenne_ms)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <table>
    <thead><tr><th>Quand</th><th>Tâche</th><th>Modèle</th><th>Durée</th><th>Résultat</th></tr></thead>
    <tbody>
      {#each appels.elements as a, i (i)}
        <tr>
          <td class="date">{quand(a.quand)}</td>
          <td>{libelleTache[a.tache] ?? a.tache}</td>
          <td>{a.modele}{a.effort ? `, ${a.effort}` : ''}</td>
          <td class="nombre">{duree(a.duree_ms)}</td>
          <td>
            <span class="etiquette {a.succes ? 'ok' : 'ko'}">{a.succes ? 'réussi' : 'échec'}</span>
            {#if a.message}<span class="discret">{a.message}</span>{/if}
          </td>
        </tr>
      {:else}
        <tr><td colspan="5" class="discret">Aucun appel.</td></tr>
      {/each}
    </tbody>
  </table>
  {#if pagesAppels > 1}
    <div class="pagination">
      <button class="second" disabled={pageAppels <= 1} onclick={() => allerAppels(pageAppels - 1)}>Précédent</button>
      <span class="discret">Page {pageAppels} sur {pagesAppels}</span>
      <button class="second" disabled={pageAppels >= pagesAppels} onclick={() => allerAppels(pageAppels + 1)}>Suivant</button>
    </div>
  {/if}
{/if}

<style>
  .reglages { margin-bottom: 1rem; }
  .reglages select { max-width: 12rem; }
  .stats { margin-bottom: 0.8rem; }
  .choix { display: flex; gap: 0.3rem; align-items: center; color: var(--encre); font-size: 1rem; margin: 0; }
  .choix input { width: auto; }
  h3 { font-size: 0.98rem; margin: 1.3rem 0 0.4rem; font-weight: 650; }
  .case label { display: flex; gap: 0.4rem; align-items: center; color: var(--encre); font-size: 1rem; margin-top: 1.4rem; }
</style>
