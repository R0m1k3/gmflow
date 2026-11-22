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
    }, 'Paramètres enregistrés.');
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
  const activer = () => action(async () => remplir(await api.post<Boite>(`/api/admin/boites/${id}/activer`)), 'Boîte activée : GMFlow relève ses mails toutes les 5 minutes.');
  const pause = () => confirm('Mettre cette boîte en pause ? GMFlow ne relèvera plus ses mails tant que vous ne l\'aurez pas réactivée.') && action(async () => remplir(await api.post<Boite>(`/api/admin/boites/${id}/pause`)), 'Boîte mise en pause : plus aucune relève tant que vous ne la réactivez pas.');
  const relever = () => action(() => api.post(`/api/admin/boites/${id}/relever`), 'Relève demandée : les chiffres se mettent à jour dans quelques instants.');
</script>

<p class="retour"><a href="/admin/boites">← Boîtes mail</a></p>
{#if b}
  <div class="entete">
    <div>
      <h1>{b.libelle}</h1>
      <p class="sous">Connexion, dossiers lus et envoi des réponses pour cette boîte mail.</p>
    </div>
  </div>

  {#if b.pause_auto}
    <div class="bandeau erreur">GMFlow a mis cette boîte en pause après {b.echecs_auth} refus de connexion. Vérifiez le mot de passe (section Connexion), cliquez sur « Tester la connexion », puis sur « Activer ».</div>
  {:else if b.derniere_erreur}
    <div class="bandeau attention">La dernière relève a échoué{b.erreur_depuis ? ` (depuis ${quand(b.erreur_depuis)})` : ''} : {b.derniere_erreur}. GMFlow réessaie tout seul ; si cela dure, vérifiez le mot de passe puis testez la connexion.</div>
  {/if}
  {#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
  {#if info}<div class="bandeau ok">{info}</div>{/if}

  <section class="carte">
    <div class="actions etat">
      <span class="etiquette {b.pause_auto ? 'ko' : b.active ? (b.erreur_depuis ? 'att' : 'ok') : ''}">
        {b.pause_auto ? 'En pause (refus de connexion)' : b.active ? (b.erreur_depuis ? 'En erreur' : 'Connectée') : 'Inactive'}
      </span>
      <span class="discret">Dernière relève : {quand(b.derniere_releve)}</span>
    </div>
    {#if b.dernier_test}
      <p class="discret">
        Dernier test {quand(b.dernier_test.quand)} :
        <span class="etiquette {b.dernier_test.ok ? 'ok' : 'ko'}">{b.dernier_test.ok ? 'Réussi' : 'Échec'}</span>
        {b.dernier_test.message}
      </p>
    {:else}
      <p class="discret">Cette boîte n'a pas encore été testée. Le test est nécessaire avant de l'activer.</p>
    {/if}
    <div class="actions">
      {#if b.active && !b.pause_auto}
        <button onclick={relever}>Relever maintenant</button>
        <button class="second" onclick={tester} disabled={testEnCours}>{testEnCours ? 'Test en cours…' : 'Tester la connexion'}</button>
        <button class="danger" onclick={pause}>Mettre en pause</button>
      {:else}
        <button onclick={activer} disabled={!b.dernier_test?.ok}>Activer</button>
        <button class="second" onclick={tester} disabled={testEnCours}>{testEnCours ? 'Test en cours…' : 'Tester la connexion'}</button>
      {/if}
    </div>
    {#if !(b.active && !b.pause_auto) && !b.dernier_test?.ok}
      <p class="aide">Le bouton « Activer » est disponible dès qu'un test de connexion a réussi.</p>
    {/if}
  </section>

  <form class="pile formulaire" onsubmit={enregistrer}>
    <section>
      <div class="section"><div class="titre"><h2>Connexion</h2></div></div>
      <p class="aide intro">Les informations pour lire cette boîte, comme dans un logiciel de messagerie (IMAPS, port 993).</p>
      <div class="champs">
        <div>
          <label for="s">Société</label>
          <select id="s" bind:value={f.societe_id}>{#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}</select>
          <p class="aide">La société à laquelle appartiennent les mails.</p>
        </div>
        <div>
          <label for="l">Nom de la boîte</label>
          <input id="l" bind:value={f.libelle} required />
          <p class="aide">Pour la reconnaître dans GMFlow.</p>
        </div>
        <div>
          <label for="srv">Serveur de réception (IMAP)</label>
          <input id="srv" bind:value={f.serveur} required />
          <p class="aide">Gmail : imap.gmail.com. Infomaniak : mail.infomaniak.com.</p>
        </div>
        <div>
          <label for="p">Port</label>
          <input id="p" type="number" bind:value={f.port} />
          <p class="aide">993 dans presque tous les cas.</p>
        </div>
        <div>
          <label for="i">Identifiant</label>
          <input id="i" bind:value={f.identifiant} required autocomplete="off" />
          <p class="aide">Presque toujours l'adresse mail complète.</p>
        </div>
        <div>
          <label for="m">Nouveau mot de passe d'application</label>
          <input id="m" type="password" bind:value={f.mot_de_passe} placeholder="inchangé" autocomplete="new-password" />
          <p class="aide">Laissez vide pour garder l'actuel. Pour Gmail et Infomaniak, utilisez un mot de passe d'application créé dans la sécurité du compte.</p>
        </div>
        <div>
          <label for="r">Reprise de l'historique (mois)</label>
          <input id="r" type="number" min="0" max="24" bind:value={f.reprise_mois} />
          <p class="aide">Combien de mois de mails déjà reçus GMFlow lit.</p>
        </div>
      </div>
    </section>

    <section>
      <div class="section"><div class="titre"><h2>Dossiers lus</h2></div></div>
      <p class="aide intro">Cochez les dossiers de la boîte que GMFlow doit lire. Le dossier de réception (INBOX) suffit le plus souvent.</p>
      {#if !b.dernier_test?.ok}<p class="bandeau info">Testez la connexion pour voir la liste des dossiers de la boîte.</p>{/if}
      <div class="dossiers">
        {#each proposes as d (d)}
          <label class="case"><input type="checkbox" value={d} bind:group={f.dossiers} /> {d}</label>
        {/each}
      </div>
    </section>

    <section>
      <div class="section"><div class="titre"><h2>Réponses</h2></div></div>
      <p class="aide intro">
        Le serveur d'envoi sert à répondre aux mails depuis GMFlow, avec le même identifiant et le même mot de passe. Sans lui, vous pouvez
        préparer des brouillons mais rien ne peut partir. Le dossier des envoyés reçoit une copie de chaque réponse et donne à Claude un exemple de votre ton.
      </p>
      <div class="champs">
        <div>
          <label for="smtp">Serveur d'envoi (SMTP)</label>
          <input id="smtp" bind:value={f.smtp_serveur} placeholder="smtp.gmail.com" />
          <p class="aide">Gmail : smtp.gmail.com. Infomaniak : mail.infomaniak.com. Vide : pas d'envoi.</p>
        </div>
        <div>
          <label for="sp">Port</label>
          <input id="sp" type="number" min="1" max="65535" bind:value={f.smtp_port} placeholder="465" />
          <p class="aide">465 (sécurité TLS) ou 587 (STARTTLS).</p>
        </div>
        <div>
          <label for="ss">Sécurité</label>
          <select id="ss" bind:value={f.smtp_securite}>
            <option value="tls">TLS (port 465)</option>
            <option value="starttls">STARTTLS (port 587)</option>
          </select>
          <p class="aide">En cas de doute, gardez TLS.</p>
        </div>
        <div>
          <label for="env">Dossier des envoyés</label>
          <select id="env" bind:value={f.dossier_envoyes}>
            <option value="">Aucun (pas de copie)</option>
            {#each envoyesProposes as d (d)}<option value={d}>{d}</option>{/each}
          </select>
          <p class="aide">Souvent « Sent » ou « Éléments envoyés ». La liste apparaît après un test de connexion.</p>
        </div>
      </div>
      <div class="actions"><button type="button" class="second" onclick={proposerSmtp}>Remplir d'après le serveur de réception</button></div>
    </section>

    <div class="actions barre"><button>Enregistrer les paramètres</button></div>
  </form>

  <div class="section">
    <div class="titre"><h2>Mails lus sur les {b.reprise_mois} derniers mois</h2></div>
    <p class="aide intro">Pour vérifier que GMFlow voit tout : comparez ces nombres à ceux affichés par le webmail pour chaque dossier.</p>
    {#if compteurs.length}
      <table>
        <thead><tr><th>Dossier</th><th class="nombre">Messages vus</th><th class="nombre">Mails distincts</th><th class="nombre">Total vus</th><th class="nombre">Dernier UID</th></tr></thead>
        <tbody>
          {#each compteurs as c (c.dossier)}
            <tr><td>{c.dossier}</td><td class="nombre">{c.messages_vus}</td><td class="nombre">{c.mails_uniques}</td><td class="nombre">{c.messages_vus_total}</td><td class="nombre">{c.dernier_uid}</td></tr>
          {/each}
        </tbody>
      </table>
      <p class="aide">« Dernier UID » est le numéro technique du dernier message lu dans le dossier.</p>
    {:else}
      <div class="vide"><strong>Aucun mail lu pour l'instant</strong><p>Les chiffres apparaissent après la première relève d'une boîte activée.</p></div>
    {/if}
  </div>
{:else if erreur}
  <div class="bandeau erreur">{erreur}</div>
{:else}
  <div class="charge" aria-busy="true"><span></span></div>
{/if}

<style>
  .retour { margin-bottom: var(--e3); }
  .etat { margin-bottom: var(--e2); }
  .formulaire { margin-top: var(--e6); }
  .formulaire section > .section { margin-top: 0; }
  .intro { margin: 0 0 var(--e4); max-width: 70ch; }
  .dossiers { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: var(--e1) var(--e4); }
  .barre { position: sticky; bottom: 0; background: var(--fond); padding: var(--e3) 0; border-top: 1px solid var(--trait); }
</style>
