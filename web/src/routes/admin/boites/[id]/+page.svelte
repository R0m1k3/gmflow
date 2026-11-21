<script lang="ts">
  import { page } from '$app/state';
  import { api, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { Boite, Compteur, Societe } from '#lib/types.ts';

  const id = page.params.id;
  let b = $state<Boite | null>(null);
  let societes = $state<Societe[]>([]);
  let compteurs = $state<Compteur[]>([]);
  let erreur = $state('');
  let info = $state('');
  let f = $state({
    societe_id: '', libelle: '', serveur: '', port: 993, identifiant: '', mot_de_passe: '', reprise_mois: 3, dossiers: [] as string[],
    smtp_serveur: '', smtp_port: null as number | null, smtp_securite: 'tls', dossier_envoyes: ''
  });
  let testEnCours = $state(false);

  function remplir(v: Boite) {
    b = v;
    f = {
      societe_id: String(v.societe_id), libelle: v.libelle, serveur: v.serveur, port: v.port, identifiant: v.identifiant, mot_de_passe: '',
      reprise_mois: v.reprise_mois, dossiers: [...v.dossiers],
      smtp_serveur: v.smtp_serveur ?? '', smtp_port: v.smtp_port, smtp_securite: v.smtp_securite ?? 'tls', dossier_envoyes: v.dossier_envoyes ?? ''
    };
  }
  async function lire(): Promise<Boite> {
    const v = (await api.get<Boite[]>('/api/admin/boites')).find((x) => String(x.id) === id);
    if (!v) throw new Error('Boîte introuvable.');
    return v;
  }
  async function charger() {
    remplir(await lire());
    compteurs = await api.get<Compteur[]>(`/api/admin/boites/${id}/compteurs`);
  }
  $effect(() => {
    api.get<Societe[]>('/api/admin/societes').then((v) => (societes = v));
    charger().catch((e) => (erreur = message(e)));
  });

  // Dossiers proposés : ceux du serveur (dernier test) et ceux déjà cochés.
  let proposes = $derived([...new Set([...(b?.dernier_test?.dossiers ?? []), ...f.dossiers])].sort());
  // Dossier des envoyés : ceux du serveur dont le nom y ressemble d'abord.
  let envoyesProposes = $derived(
    [...new Set([...(b?.dernier_test?.dossiers ?? []), ...(f.dossier_envoyes ? [f.dossier_envoyes] : [])])].sort(
      (x, y) => Number(/sent|envoy/i.test(y)) - Number(/sent|envoy/i.test(x)) || x.localeCompare(y)
    )
  );

  // Réglages SMTP déduits du serveur IMAP (Gmail, Infomaniak, sinon « imap. » → « smtp. »).
  function proposerSmtp() {
    const h = f.serveur.trim().toLowerCase();
    f.smtp_serveur = h === 'imap.gmail.com' ? 'smtp.gmail.com' : h === 'mail.infomaniak.com' ? 'mail.infomaniak.com' : h.replace(/^imap\./, 'smtp.');
    f.smtp_port = 465;
    f.smtp_securite = 'tls';
    if (!f.dossier_envoyes) f.dossier_envoyes = envoyesProposes.find((d) => /sent|envoy/i.test(d)) ?? '';
  }

  async function action(fn: () => Promise<unknown>, ok?: string) {
    erreur = '';
    info = '';
    try {
      await fn();
      if (ok) info = ok;
    } catch (e) {
      erreur = message(e);
    }
  }

  const enregistrer = (e: SubmitEvent) => {
    e.preventDefault();
    return action(async () => {
      remplir(
        await api.put<Boite>(`/api/admin/boites/${id}`, {
          ...f,
          societe_id: Number(f.societe_id),
          mot_de_passe: f.mot_de_passe || null,
          smtp_port: f.smtp_port || null
        })
      );
      compteurs = await api.get<Compteur[]>(`/api/admin/boites/${id}/compteurs`);
    }, 'Enregistré.');
  };

  function tester() {
    return action(async () => {
      const avant = b?.dernier_test?.quand;
      await api.post(`/api/admin/boites/${id}/tester`);
      testEnCours = true;
      sonder(async () => {
        const v = await lire();
        if (v.dernier_test?.quand && v.dernier_test.quand !== avant) {
          testEnCours = false;
          const coches = f.dossiers;
          remplir(v);
          f.dossiers = coches;
          return false;
        }
        return true;
      });
    });
  }
  const activer = () => action(async () => remplir(await api.post<Boite>(`/api/admin/boites/${id}/activer`)), 'Boîte active : relève toutes les 5 minutes.');
  const pause = () => action(async () => remplir(await api.post<Boite>(`/api/admin/boites/${id}/pause`)), 'Boîte en pause.');
  const relever = () => action(() => api.post(`/api/admin/boites/${id}/relever`), 'Relève demandée : les compteurs se mettent à jour dans quelques instants.');
</script>

<p><a href="/admin/boites">← Boîtes surveillées</a></p>
{#if b}
  <h1>{b.libelle}</h1>
  {#if b.pause_auto}
    <div class="bandeau erreur">Pause automatique après {b.echecs_auth} refus de connexion : vérifiez le mot de passe, testez, puis activez à nouveau.</div>
  {:else if b.derniere_erreur}
    <div class="bandeau attention">Dernière erreur{b.erreur_depuis ? ` (depuis ${quand(b.erreur_depuis)})` : ''} : {b.derniere_erreur}</div>
  {/if}
  {#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
  {#if info}<div class="bandeau ok">{info}</div>{/if}

  <div class="carte">
    <div class="actions">
      <span class="etiquette {b.active && !b.pause_auto ? 'ok' : b.pause_auto ? 'ko' : ''}">{b.pause_auto ? 'pause automatique' : b.active ? 'active' : 'inactive'}</span>
      <span class="discret">Dernière relève : {quand(b.derniere_releve)}</span>
      <span style="flex:1"></span>
      <button class="second" onclick={tester} disabled={testEnCours}>{testEnCours ? 'Test en cours…' : 'Tester la connexion'}</button>
      {#if b.active && !b.pause_auto}
        <button class="second" onclick={relever}>Relever maintenant</button>
        <button class="danger" onclick={pause}>Mettre en pause</button>
      {:else}
        <button onclick={activer} disabled={!b.dernier_test?.ok}>Activer</button>
      {/if}
    </div>
    {#if b.dernier_test}
      <p class="discret">
        Dernier test {quand(b.dernier_test.quand)} :
        <span class="etiquette {b.dernier_test.ok ? 'ok' : 'ko'}">{b.dernier_test.ok ? 'réussi' : 'échec'}</span>
        {b.dernier_test.message}
      </p>
    {:else}
      <p class="discret">Pas encore testée : le test est nécessaire avant l'activation.</p>
    {/if}
  </div>

  <form class="carte" style="margin-top:1rem" onsubmit={enregistrer}>
    <h2>Paramètres</h2>
    <div class="champs">
      <div>
        <label for="s">Structure</label>
        <select id="s" bind:value={f.societe_id}>{#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}</select>
      </div>
      <div><label for="l">Libellé</label><input id="l" bind:value={f.libelle} required /></div>
      <div><label for="srv">Serveur IMAP</label><input id="srv" bind:value={f.serveur} required /></div>
      <div><label for="p">Port</label><input id="p" type="number" bind:value={f.port} /></div>
      <div><label for="i">Identifiant</label><input id="i" bind:value={f.identifiant} required autocomplete="off" /></div>
      <div><label for="m">Nouveau mot de passe</label><input id="m" type="password" bind:value={f.mot_de_passe} placeholder="inchangé" autocomplete="new-password" /></div>
      <div><label for="r">Reprise (mois)</label><input id="r" type="number" min="0" max="24" bind:value={f.reprise_mois} /></div>
    </div>
    <h2>Dossiers surveillés</h2>
    {#if !b.dernier_test?.ok}<p class="discret">Testez la connexion pour voir la liste des dossiers du serveur.</p>{/if}
    <div class="dossiers">
      {#each proposes as d (d)}
        <label class="dossier"><input type="checkbox" value={d} bind:group={f.dossiers} /> {d}</label>
      {/each}
    </div>
    <h2>Réponses</h2>
    <p class="discret">
      Serveur d'envoi des réponses, avec le même identifiant et le même mot de passe. Sans serveur SMTP, les brouillons restent
      possibles mais rien ne peut partir. Le dossier des envoyés reçoit une copie de chaque réponse et sert d'exemple de ton.
    </p>
    <div class="champs">
      <div><label for="smtp">Serveur SMTP</label><input id="smtp" bind:value={f.smtp_serveur} placeholder="aucun" /></div>
      <div><label for="sp">Port</label><input id="sp" type="number" min="1" max="65535" bind:value={f.smtp_port} placeholder="465" /></div>
      <div>
        <label for="ss">Sécurité</label>
        <select id="ss" bind:value={f.smtp_securite}>
          <option value="tls">TLS (port 465)</option>
          <option value="starttls">STARTTLS (port 587)</option>
        </select>
      </div>
      <div>
        <label for="env">Dossier des envoyés</label>
        <select id="env" bind:value={f.dossier_envoyes}>
          <option value="">aucun (pas de copie)</option>
          {#each envoyesProposes as d (d)}<option value={d}>{d}</option>{/each}
        </select>
      </div>
    </div>
    <div class="actions" style="margin-top:0.8rem">
      <button>Enregistrer</button>
      <button type="button" class="second" onclick={proposerSmtp}>Proposer d'après le serveur IMAP</button>
    </div>
  </form>

  <h2>Compteurs sur la période de reprise ({b.reprise_mois} mois)</h2>
  <p class="discret">À comparer au nombre de messages affiché par le webmail pour chaque dossier.</p>
  <table>
    <thead><tr><th>Dossier</th><th>Messages vus</th><th>Mails uniques</th><th>Total vus</th><th>Dernier UID</th></tr></thead>
    <tbody>
      {#each compteurs as c (c.dossier)}
        <tr><td>{c.dossier}</td><td class="nombre">{c.messages_vus}</td><td class="nombre">{c.mails_uniques}</td><td class="nombre">{c.messages_vus_total}</td><td class="nombre">{c.dernier_uid}</td></tr>
      {/each}
    </tbody>
  </table>
{:else if erreur}
  <div class="bandeau erreur">{erreur}</div>
{/if}

<style>
  .dossiers { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 0.3rem 1rem; }
  .dossier { color: var(--encre); font-size: 0.95rem; display: flex; gap: 0.4rem; align-items: center; margin: 0; }
</style>
