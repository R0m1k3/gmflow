<script lang="ts">
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { EtatClaude, Seance } from '#lib/types.ts';

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

  const finals = ['terminee', 'erreur', 'expiree'];
  async function lancer() {
    erreur = '';
    codeEnvoye = false;
    code = '';
    try {
      seanceId = (await api.post<{ id: string }>('/api/admin/claude/seances')).id;
      seance = { etat: 'demarrage', url: null, message: null };
      sonder(async () => {
        seance = await api.get<Seance>(`/api/admin/claude/seances/${seanceId}`);
        if (finals.includes(seance.etat)) {
          await chargerEtat();
          if (seance.etat === 'terminee') suivreEtat();
          return false;
        }
        return true;
      });
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
      <span>Jeton : <span class="etiquette {classe}">{texte}</span></span>
      {#if etat.fin}<span class="discret">se termine par …{etat.fin}</span>{/if}
      <span class="discret">testé {quand(etat.teste_le)}</span>
      <span class="discret">dernière utilisation {quand(etat.utilise_le)}</span>
      <span style="flex:1"></span>
      {#if etat.etat !== 'absent'}
        <button class="second" onclick={tester}>Tester</button>
        <button class="danger" onclick={supprimer}>Supprimer</button>
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
  {:else}
    {#if seance}
      <div class="bandeau {seance.etat === 'terminee' ? 'ok' : 'erreur'}">{etapes[seance.etat]}{seance.message ? ` ${seance.message}` : ''}</div>
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
