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
      infoAcces = 'Connexion enregistrée, test en cours.';
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
    if (!confirm('Supprimer la connexion à Claude ? GMFlow ne pourra plus lire les documents ni trier les mails tant que vous ne vous reconnectez pas.')) return;
    erreur = '';
    try {
      await api.del('/api/admin/claude/jeton');
      await chargerEtat();
    } catch (err) {
      erreur = message(err);
    }
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
      infoAcces = etat?.acces?.mode === 'cle_api' ? 'Clé enregistrée, test en cours.' : 'Clé enregistrée. Choisissez « Clé API » dans « Accès utilisé » pour l\'utiliser.';
      if (etat?.etat === 'en_test') suivreEtat();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function supprimerCle() {
    if (!confirm('Supprimer la clé API ? GMFlow reviendra à la connexion par compte Claude.')) return;
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
    valide: ['Connectée', 'ok'],
    erreur: ['En erreur', 'ko'],
    absent: ['Non connectée', 'ko'],
    en_test: ['Test en cours', 'att']
  };
  const etapes: Record<string, string> = {
    demarrage: 'Préparation de la connexion…',
    attente_code: 'Ouvrez la page Claude, connectez-vous, puis collez ici le code affiché.',
    code_recu: 'Code envoyé, en attente de la réponse de Claude…',
    terminee: 'Connexion enregistrée. Test en cours.',
    erreur: 'La connexion a échoué.',
    expiree: 'Aucun code reçu à temps : recommencez la connexion.'
  };
  let enCours = $derived(!!seance && !finals.includes(seance.etat));
  // Étape active du parcours : 1 lancer, 2 ouvrir la page Claude, 3 coller le code.
  let etapeActive = $derived(!enCours ? 1 : seance?.etat === 'attente_code' || seance?.etat === 'code_recu' ? (codeEnvoye || seance?.etat === 'code_recu' ? 4 : 2) : 1);
</script>

<div class="entete">
  <div>
    <h1>Connexion à Claude</h1>
    <p class="sous">Claude est l'assistant qui lit les factures et bons de livraison et trie les mails. Cette page le relie à votre compte.</p>
  </div>
</div>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}

<!-- 1. État de la connexion -->
<div class="section premiere">
  <div class="titre"><h2>1. État de la connexion</h2></div>
</div>
{#if etat}
  {@const [texte, classe] = libelles[etat.etat] ?? [etat.etat, '']}
  {#if etat.etat === 'absent'}
    <div class="bandeau attention">Claude n'est pas connecté : aucun document n'est lu et aucun mail n'est trié. Suivez les étapes de la section 2.</div>
  {:else if etat.etat === 'erreur'}
    <div class="bandeau erreur">Claude refuse la connexion{etat.message ? ` : ${etat.message}` : ''}. Cliquez sur « Tester la connexion » ; si l'erreur revient, reconnectez-vous (section 2).</div>
  {:else if etat.etat === 'en_test'}
    <div class="bandeau info" aria-live="polite">Vérification de la connexion en cours, quelques secondes.</div>
  {/if}
  <div class="carte">
    <dl class="kv">
      <dt>Connexion</dt>
      <dd>
        <span class="etiquette {classe}">{etat.etat === 'absent' && etat.acces?.mode === 'cle_api' ? 'Aucune clé' : texte}</span>
      </dd>
      <dt>Type d'accès</dt>
      <dd>
        {etat.acces?.mode === 'cle_api' ? 'Clé API Anthropic (facturée à l\'usage)' : 'Compte Claude (abonnement)'}
        {#if etat.acces?.mode === 'cle_api'}
          {#if etat.acces.cle_api_fin}<span class="discret"> · clé se terminant par …{etat.acces.cle_api_fin}</span>{/if}
        {:else if etat.fin}<span class="discret"> · se termine par …{etat.fin}</span>{/if}
      </dd>
      <dt>Dernier test</dt>
      <dd>{quand(etat.teste_le)}</dd>
      <dt>Dernière utilisation</dt>
      <dd>{quand(etat.utilise_le)}</dd>
    </dl>
    {#if etat.etat !== 'absent' || (etat.acces?.jeton ?? false)}
      <div class="actions rangee">
        {#if etat.etat !== 'absent'}<button class="second" onclick={tester} disabled={etat.etat === 'en_test'}>Tester la connexion</button>{/if}
        {#if etat.acces?.jeton ?? etat.etat !== 'absent'}<button class="danger" onclick={supprimer}>Supprimer la connexion</button>{/if}
      </div>
    {/if}
  </div>
{:else if !erreur}
  <div class="charge" aria-busy="true"><span></span></div>
{/if}

<!-- 2. Se connecter -->
<div class="section">
  <div class="titre"><h2>2. Se connecter</h2></div>
</div>
<p class="gris">Il faut un abonnement Claude. GMFlow vous guide en trois étapes, sans rien installer. <span class="discret">(Technique : cela lance la commande <code>claude setup-token</code> sur le serveur.)</span></p>

<ol class="etapes">
  <li class:fait={etapeActive > 1} class:actuelle={etapeActive === 1}>
    <div class="num">1</div>
    <div class="contenu">
      <strong>Lancer la connexion</strong>
      {#if !enCours}
        {#if seance}
          {#if seance.message === 'annulée'}
            <p class="discret">Connexion annulée.</p>
          {:else}
            <div class="bandeau {seance.etat === 'terminee' ? 'ok' : 'erreur'}">{etapes[seance.etat]}{seance.message ? ` ${seance.message}` : ''}{seance.etat === 'terminee' ? '' : ' Vous pouvez recommencer.'}</div>
          {/if}
        {/if}
        <p class="aide">GMFlow prépare un lien de connexion personnel.</p>
        <button onclick={lancer}>Lancer la connexion</button>
      {:else if seance?.etat === 'demarrage'}
        <p class="discret" aria-live="polite">{etapes.demarrage}</p>
      {:else}
        <p class="discret">Lien prêt.</p>
      {/if}
    </div>
  </li>
  <li class:fait={etapeActive > 2} class:actuelle={etapeActive === 2} class:attente={etapeActive < 2}>
    <div class="num">2</div>
    <div class="contenu">
      <strong>Se connecter sur le site de Claude</strong>
      {#if enCours && seance?.url}
        <p class="aide">Le lien s'ouvre dans un nouvel onglet. Connectez-vous avec votre compte Claude, autorisez l'accès, puis copiez le code affiché.</p>
        <a class="bouton {etapeActive === 2 ? '' : 'second'}" href={seance.url} target="_blank" rel="noopener noreferrer">Ouvrir la page de connexion Claude</a>
      {:else}
        <p class="aide">Disponible après l'étape 1.</p>
      {/if}
    </div>
  </li>
  <li class:fait={etapeActive > 3} class:actuelle={etapeActive === 3 || (etapeActive === 2 && enCours && !!seance?.url)} class:attente={etapeActive < 2}>
    <div class="num">3</div>
    <div class="contenu">
      <strong>Coller le code ici</strong>
      {#if enCours && seance?.etat === 'attente_code' && !codeEnvoye}
        <form class="rangee-champ" onsubmit={envoyerCode}>
          <div>
            <label for="code">Code de connexion</label>
            <input id="code" bind:value={code} required autocomplete="off" />
          </div>
          <button>Envoyer le code</button>
        </form>
        <p class="aide">Un code refusé se corrige ici : recollez-le en entier, sans espace avant ni après.</p>
      {:else if enCours && (codeEnvoye || seance?.etat === 'code_recu')}
        <p class="discret" aria-live="polite">{etapes.code_recu}</p>
      {:else}
        <p class="aide">Disponible après l'étape 2.</p>
      {/if}
    </div>
  </li>
</ol>
{#if enCours}
  <div class="actions annuler"><button class="second" onclick={annuler}>Annuler la connexion</button></div>
{/if}

<details class="repli">
  <summary>Autres façons de se connecter</summary>

  <h3>Coller un jeton déjà généré</h3>
  <form class="carte" onsubmit={coller}>
    <p class="aide intro">Si vous avez déjà créé un jeton (setup-token) sur un autre ordinateur avec la commande <code>claude setup-token</code>. Il n'est plus jamais affiché ensuite.</p>
    <div class="rangee-champ">
      <div>
        <label for="jeton">Jeton</label>
        <input id="jeton" type="password" bind:value={jeton} placeholder="sk-ant-oat01-…" required autocomplete="off" />
      </div>
      <button>Enregistrer et tester</button>
    </div>
  </form>

  <h3>Utiliser une clé API Anthropic</h3>
  <div class="carte">
    {#if infoAcces}<div class="bandeau ok">{infoAcces}</div>{/if}
    <p class="aide intro">
      Une clé créée sur la console Anthropic (console.anthropic.com), facturée à l'usage au lieu de l'abonnement. GMFlow utilise l'un ou l'autre accès, jamais les deux.
    </p>
    {#if etat?.acces}
      <fieldset>
        <legend>Accès utilisé</legend>
        <label class="case"><input type="radio" name="acces" checked={etat.acces.mode === 'jeton'} onchange={() => choisirAcces('jeton')} /> Compte Claude (abonnement)</label>
        <label class="case">
          <input type="radio" name="acces" checked={etat.acces.mode === 'cle_api'} disabled={!etat.acces.cle_api} onchange={() => choisirAcces('cle_api')} />
          Clé API{etat.acces.cle_api ? ` (…${etat.acces.cle_api_fin ?? ''})` : ' (aucune enregistrée)'}
        </label>
      </fieldset>
    {/if}
    <form onsubmit={enregistrerCle}>
      <div class="rangee-champ">
        <div>
          <label for="cle">Clé API</label>
          <input id="cle" type="password" bind:value={cle} placeholder="sk-ant-api03-…" required autocomplete="off" />
        </div>
        <button>{etat?.acces?.cle_api ? 'Remplacer la clé' : 'Enregistrer la clé'}</button>
        {#if etat?.acces?.cle_api}<button type="button" class="danger" onclick={supprimerCle}>Supprimer la clé</button>{/if}
      </div>
      <p class="aide">Elle n'est plus jamais affichée ensuite.</p>
    </form>
  </div>
</details>

<!-- 3. Réglages avancés -->
<div class="section">
  <div class="titre"><h2>3. Réglages avancés</h2></div>
</div>
<p class="gris">Les réglages par défaut conviennent. À ouvrir seulement pour régler la précision, la vitesse ou la limite de lectures par jour.</p>

<details class="repli">
  <summary>Lecture des documents</summary>
  <p class="aide intro">Comment Claude lit les factures, avoirs et bons de livraison joints aux mails.</p>
  {#if vue && reglages}
    <form class="carte" onsubmit={enregistrerReglages}>
      {#if infoReglages}<div class="bandeau ok">{infoReglages}</div>{/if}
      <table class="reglages">
        <thead><tr><th>Tâche</th><th>Modèle</th><th>Effort de réflexion</th></tr></thead>
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
      <p class="aide">Modèle : un modèle plus puissant lit mieux mais plus lentement. Effort : le temps de réflexion de Claude ; plus il est élevé, plus la lecture est soignée.</p>
      <div class="champs haut">
        <div>
          <label for="quota">Limite de lectures par jour</label>
          <input id="quota" type="number" min="0" bind:value={reglages.quota_lecture_jour} />
          <p class="aide"><span class="nombre">{vue.appels_lecture_du_jour}</span> lectures aujourd'hui. Au-delà, les documents attendent le lendemain.</p>
        </div>
        <div>
          <label class="case"><input type="checkbox" bind:checked={reglages.lecture_auto} /> Lire automatiquement les nouvelles pièces jointes</label>
          <p class="aide">Décoché, rien n'est lu tout seul. Le bouton « Relire » d'un document fonctionne toujours.</p>
        </div>
      </div>
      <div class="actions"><button>Enregistrer</button></div>
    </form>
  {:else}
    <div class="charge" aria-busy="true"><span></span></div>
  {/if}
</details>

<details class="repli">
  <summary>Tri des mails et réponses</summary>
  <p class="aide intro">Comment Claude classe les mails reçus (question, information, publicité) et rédige les brouillons de réponse.</p>
  {#if vue && reglages}
    <form class="carte" onsubmit={enregistrerReglages}>
      {#if infoReglages}<div class="bandeau ok">{infoReglages}</div>{/if}
      <table class="reglages">
        <thead><tr><th>Tâche</th><th>Modèle</th><th>Effort de réflexion</th></tr></thead>
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
      <div class="champs haut">
        <div>
          <label for="qtri">Limite de tris par jour</label>
          <input id="qtri" type="number" min="0" bind:value={reglages.quota_tri_jour} />
          <p class="aide"><span class="nombre">{vue.appels_tri_du_jour}</span> tris aujourd'hui. Les mails reconnus d'avance (listes de diffusion, no-reply, expéditeurs marqués) ne comptent pas.</p>
        </div>
        <div>
          <label for="dtri">Trier les mails reçus depuis (jours)</label>
          <input id="dtri" type="number" min="1" max="365" bind:value={reglages.tri_depuis_jours} />
          <p class="aide">Les mails plus anciens restent non triés.</p>
        </div>
        <div>
          <label class="case"><input type="checkbox" bind:checked={reglages.tri_auto} /> Trier automatiquement les nouveaux mails</label>
          <p class="aide">Les brouillons ne sont jamais automatiques : ils se demandent mail par mail.</p>
        </div>
      </div>
      <div class="actions"><button>Enregistrer</button></div>
    </form>
  {:else}
    <div class="charge" aria-busy="true"><span></span></div>
  {/if}
</details>

<details class="repli">
  <summary>Expéditeurs toujours classés en publicité{marques.length ? ` (${marques.length})` : ''}</summary>
  <p class="aide intro">Ajoutés depuis la fiche d'un mail (« Toujours publicité »). Leurs mails sont classés sans appeler Claude et supprimés après 90 jours.</p>
  {#if marques.length}
    <ul class="liste">
      {#each marques as m (m.id)}
        <li>
          <div class="principal">
            <span>{m.domaine ? `Tout le domaine ${m.domaine}` : m.adresse}</span>
            <span class="discret">Ajouté {quand(m.cree_le)}{m.cree_par ? ` par ${m.cree_par}` : ''}</span>
          </div>
          <div class="droite"><button class="second" onclick={() => retirerMarque(m)}>Retirer</button></div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="vide"><strong>Aucun expéditeur marqué</strong><p>Utilisez « Toujours publicité » sur un mail pour en ajouter un.</p></div>
  {/if}
</details>

<details class="repli">
  <summary>Historique des appels à Claude</summary>
  <p class="aide intro">Chaque lecture, tri ou brouillon demandé à Claude, avec sa durée et son résultat. Utile pour comprendre une lecture ratée.</p>
  {#if appels}
    {#if appels.stats.length}
      <table class="stats">
        <thead><tr><th>Sur 30 jours</th><th>Modèle</th><th class="nombre">Appels</th><th class="nombre">Réussis</th><th class="nombre">Durée moyenne</th></tr></thead>
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
    {#if appels.elements.length}
      <table>
        <thead><tr><th>Quand</th><th>Tâche</th><th>Modèle</th><th class="nombre">Durée</th><th>Résultat</th></tr></thead>
        <tbody>
          {#each appels.elements as a, i (i)}
            <tr>
              <td class="date">{quand(a.quand)}</td>
              <td>{libelleTache[a.tache] ?? a.tache}</td>
              <td>{a.modele}{a.effort ? `, ${a.effort}` : ''}</td>
              <td class="nombre">{duree(a.duree_ms)}</td>
              <td>
                <span class="etiquette {a.succes ? 'ok' : 'ko'}">{a.succes ? 'Réussi' : 'Échec'}</span>
                {#if a.message}<span class="discret"> {a.message}</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="vide"><strong>Aucun appel pour l'instant</strong><p>Les appels apparaissent dès que Claude lit un document ou trie un mail.</p></div>
    {/if}
    {#if pagesAppels > 1}
      <div class="pagination">
        <button class="second" disabled={pageAppels <= 1} onclick={() => allerAppels(pageAppels - 1)}>Précédent</button>
        <span>Page {pageAppels} sur {pagesAppels}</span>
        <button class="second" disabled={pageAppels >= pagesAppels} onclick={() => allerAppels(pageAppels + 1)}>Suivant</button>
      </div>
    {/if}
  {:else}
    <div class="charge" aria-busy="true"><span></span></div>
  {/if}
</details>

<style>
  .premiere { margin-top: 0; }
  .rangee { margin-top: var(--e4); }
  .intro { margin: 0 0 var(--e3); max-width: 70ch; }
  .haut { align-items: start; }
  .reglages { margin-bottom: var(--e3); }
  .reglages select { max-width: 14rem; }
  .stats { margin-bottom: var(--e4); }

  .etapes { list-style: none; margin: var(--e4) 0 var(--e3); padding: 0; display: grid; gap: var(--e3); }
  .etapes > li { display: flex; gap: var(--e4); padding: var(--e4); background: var(--surface); border: 1px solid var(--trait); border-radius: var(--rayon); }
  .etapes > li.actuelle { border-color: var(--accent); }
  .etapes > li.attente { background: transparent; }
  .etapes .num { flex: none; width: 32px; height: 32px; border-radius: 50%; border: 1px solid var(--trait-fort); display: grid; place-items: center; font-family: var(--serif); font-weight: 600; color: var(--gris); }
  .etapes > li.actuelle .num { background: var(--accent); border-color: var(--accent); color: #fff; }
  .etapes > li.fait .num { background: var(--ok-clair); border-color: var(--ok-trait); color: var(--ok); }
  .etapes .contenu { flex: 1; min-width: 0; }
  .etapes .contenu > strong { display: block; margin-bottom: var(--e1); }
  .etapes .contenu > :last-child { margin-bottom: 0; }
  .rangee-champ { display: flex; gap: var(--e3); align-items: flex-end; flex-wrap: wrap; }
  .rangee-champ > div { flex: 1; min-width: 220px; max-width: 420px; }
  .annuler { margin-bottom: var(--e4); }

  .repli { background: var(--surface); border: 1px solid var(--trait); border-radius: var(--rayon); margin-top: var(--e3); }
  .repli > summary { cursor: pointer; display: flex; align-items: center; min-height: var(--cible); padding: var(--e1) var(--e4); font-weight: 500; list-style: none; }
  .repli > summary::-webkit-details-marker { display: none; }
  .repli > summary::after { content: 'Afficher'; margin-left: auto; font-size: var(--t-petit); font-weight: 400; color: var(--gris); }
  .repli[open] > summary::after { content: 'Masquer'; }
  .repli > summary:hover { background: var(--creux); }
  .repli[open] > summary { border-bottom: 1px solid var(--trait); }
  .repli > :not(summary) { margin-left: var(--e4); margin-right: var(--e4); }
  .repli > :last-child { margin-bottom: var(--e4); }
  .repli > .intro { margin-top: var(--e3); }
  .repli > h3:first-of-type { margin-top: var(--e4); }
  @media (max-width: 760px) { .rangee-champ > button { flex: 1 1 100%; } .etapes > li { padding: var(--e3); gap: var(--e3); } }
</style>
