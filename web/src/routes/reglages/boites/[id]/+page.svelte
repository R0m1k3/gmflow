<script lang="ts">
  // Fiche d'une boîte mail : état, pause et reprise, essai de connexion et d'envoi,
  // réglages (connexion, dossiers lus, réponses), compteurs de la relève.
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, attendreEssai, ErreurApi, essayerSmtp, lireBoite, message, sonder } from '#lib/api.ts';
  import { dateSimple, quand } from '#lib/format.ts';
  import { RUBRIQUES, nomCourtSociete, type Ton } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Boite, Compteur, EssaiBoite, Societe } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import { n, nomDossier, portPourSecurite, ressembleEnvoyes, smtpPourImap, type Securite } from '../../outils.ts';

  let admin = $derived(session.moi?.role === 'admin');
  $effect(() => {
    if (session.moi && !admin) void goto('/', { replaceState: true });
  });

  const id = Number(page.params.id);
  let b = $state<Boite | null>(null);
  let societes = $state<Societe[]>([]);
  let compteurs = $state<Compteur[]>([]);
  let erreur = $state('');
  let info = $state('');
  let introuvable = $state(false);

  type Form = {
    societe_id: string;
    libelle: string;
    serveur: string;
    port: string;
    identifiant: string;
    mot_de_passe: string;
    reprise_mois: string;
    dossiers: string[];
    smtp_serveur: string;
    smtp_port: string;
    smtp_securite: string;
    dossier_envoyes: string;
  };
  let f = $state<Form>({
    societe_id: '',
    libelle: '',
    serveur: '',
    port: '993',
    identifiant: '',
    mot_de_passe: '',
    reprise_mois: '3',
    dossiers: [],
    smtp_serveur: '',
    smtp_port: '',
    smtp_securite: 'tls',
    dossier_envoyes: ''
  });
  let initial = $state('');
  let modifie = $derived(JSON.stringify(f) !== initial);

  /** Message du dernier essai sans le « Connexion réussie : » déjà dit par l'étiquette. */
  function detailTest(m: string | null | undefined): string {
    const t = (m ?? '').replace(/^connexion (réussie|refusée)\s*:\s*/i, '');
    return t.charAt(0).toUpperCase() + t.slice(1);
  }

  function remplir(v: Boite) {
    b = v;
    const dossiers = [...v.dossiers];
    f = {
      societe_id: String(v.societe_id),
      libelle: v.libelle,
      serveur: v.serveur,
      port: String(v.port),
      identifiant: v.identifiant,
      mot_de_passe: '',
      reprise_mois: String(v.reprise_mois),
      dossiers,
      smtp_serveur: v.smtp_serveur ?? '',
      smtp_port: v.smtp_port ? String(v.smtp_port) : '',
      smtp_securite: v.smtp_securite ?? 'tls',
      dossier_envoyes: v.dossier_envoyes ?? ''
    };
    initial = JSON.stringify({ ...f, dossiers: [...v.dossiers] });
    securitePrecedente = f.smtp_securite;
  }
  async function lire(): Promise<Boite> {
    try {
      return await lireBoite(id);
    } catch (e) {
      if (e instanceof ErreurApi && e.code === 404) {
        introuvable = true;
        throw new ErreurApi(404, 'Cette boîte n’existe pas, ou plus.');
      }
      throw e;
    }
  }
  async function chargerCompteurs() {
    compteurs = await api.get<Compteur[]>(`/api/admin/boites/${id}/compteurs`);
  }
  async function charger() {
    remplir(await lire());
    await chargerCompteurs();
  }

  let arretReleve: (() => void) | null = null;
  let arretTest: (() => void) | null = null;
  $effect(() => {
    if (!admin) return;
    api
      .get<Societe[]>('/api/admin/societes')
      .then((v) => (societes = v))
      .catch(() => {});
    charger()
      .then(suivreReleve)
      .catch((e) => (erreur = message(e)));
    return () => {
      arretReleve?.();
      arretTest?.();
    };
  });

  // Première relève (boîte qui vient de démarrer) : compteurs relus jusqu'à la fin.
  function suivreReleve() {
    arretReleve?.();
    if (!b?.active || b.derniere_releve) return;
    arretReleve = sonder(async () => {
      const v = await lire();
      if (b) b = { ...b, derniere_releve: v.derniere_releve, derniere_erreur: v.derniere_erreur, erreur_depuis: v.erreur_depuis, active: v.active, pause_auto: v.pause_auto };
      await chargerCompteurs();
      return !!v.active && !v.derniere_releve;
    }, 4000);
  }

  let vus = $derived(compteurs.reduce((s, c) => s + c.mails_uniques, 0));
  let nouvelle = $derived(page.url.searchParams.has('nouvelle'));

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

  // ---------- Enregistrer ----------
  let enregistrement = $state(false);
  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    enregistrement = true;
    await action(async () => {
      const smtp = f.smtp_serveur.trim();
      const v = await api.put<Boite>(`/api/admin/boites/${id}`, {
        societe_id: Number(f.societe_id),
        libelle: f.libelle.trim(),
        serveur: f.serveur.trim(),
        port: Number(f.port) || 993,
        identifiant: f.identifiant.trim(),
        mot_de_passe: f.mot_de_passe || null,
        reprise_mois: Number(f.reprise_mois),
        dossiers: f.dossiers,
        smtp_serveur: smtp || null,
        smtp_port: smtp ? Number(f.smtp_port) || null : null,
        smtp_securite: smtp ? f.smtp_securite : null,
        dossier_envoyes: f.dossier_envoyes || null
      });
      remplir(v);
      await chargerCompteurs();
    }, 'Enregistré.');
    enregistrement = false;
  }

  // ---------- Essai de la connexion (réception) ----------
  let testEnCours = $state(false);
  function tester() {
    return action(async () => {
      const avant = b?.dernier_test?.quand;
      await api.post(`/api/admin/boites/${id}/tester`);
      testEnCours = true;
      arretTest?.();
      arretTest = sonder(async () => {
        const v = await lire();
        if (v.dernier_test?.quand && v.dernier_test.quand !== avant) {
          testEnCours = false;
          remplir(v);
          return false;
        }
        return true;
      }, 1500);
    });
  }

  // ---------- Essai de l'envoi des réponses ----------
  let essaiSmtp = $state<EssaiBoite | null>(null);
  let smtpEnCours = $state(false);
  async function testerSmtp() {
    essaiSmtp = null;
    smtpEnCours = true;
    await action(async () => {
      const { id: essai } = await essayerSmtp(id);
      essaiSmtp = await attendreEssai(essai);
    });
    smtpEnCours = false;
  }

  // ---------- Pause et reprise ----------
  let dlgPause = $state(false);
  let dlgReprise = $state(false);
  const pause = () => action(async () => remplir(await api.post<Boite>(`/api/admin/boites/${id}/pause`)), 'Boîte en pause : plus aucun mail n’est relevé.');
  const reprendre = () =>
    action(async () => {
      remplir(await api.post<Boite>(`/api/admin/boites/${id}/activer`));
      suivreReleve();
    }, 'Surveillance reprise : nouveaux mails cherchés toutes les 5 minutes.');
  const relever = () =>
    action(async () => {
      await api.post(`/api/admin/boites/${id}/relever`);
      setTimeout(() => void chargerCompteurs().catch(() => {}), 5000);
    }, 'Recherche des nouveaux mails demandée : les compteurs se mettent à jour dans quelques instants.');

  // ---------- Listes proposées ----------
  // Dossiers : ceux du serveur (dernier essai) et ceux déjà cochés.
  // Noms affichés et dossier des envoyés : donnés par le serveur depuis la refonte ; devinés pour un essai plus ancien.
  let detail = $derived(new Map((b?.dernier_test?.dossiers_detail ?? []).map((d) => [d.nom, d])));
  const affiche = (nom: string) => (nom.toUpperCase() === 'INBOX' ? 'Réception' : (detail.get(nom)?.affiche ?? nomDossier(nom)));
  const estEnvoyes = (nom: string) => {
    const d = detail.get(nom);
    return d ? d.envoyes || nom === b?.dernier_test?.envoyes : ressembleEnvoyes(nom);
  };
  let envoyesRepere = $derived(
    b?.dernier_test?.envoyes ?? (b?.dernier_test?.dossiers_detail ? null : (b?.dernier_test?.dossiers ?? []).find(ressembleEnvoyes)) ?? ''
  );
  let proposes = $derived([...new Set([...(b?.dernier_test?.dossiers ?? []), ...f.dossiers])].sort((x, y) => (x === 'INBOX' ? -1 : y === 'INBOX' ? 1 : affiche(x).localeCompare(affiche(y)))));
  let optionsEnvoyes = $derived([
    { valeur: '', libelle: 'Aucun (pas de copie des réponses)' },
    ...[...new Set([...(b?.dernier_test?.dossiers ?? []), ...(f.dossier_envoyes ? [f.dossier_envoyes] : [])])]
      .sort((x, y) => Number(estEnvoyes(y)) - Number(estEnvoyes(x)) || affiche(x).localeCompare(affiche(y)))
      .map((d) => ({ valeur: d, libelle: affiche(d) }))
  ]);
  function proposerSmtp() {
    f.smtp_serveur = smtpPourImap(f.serveur);
    f.smtp_securite = 'tls';
    f.smtp_port = '465';
    if (!f.dossier_envoyes) f.dossier_envoyes = envoyesRepere;
  }
  let securitePrecedente: string | null = null;
  $effect(() => {
    const s = f.smtp_securite;
    if (securitePrecedente === null || s === securitePrecedente) {
      securitePrecedente = s;
      return;
    }
    securitePrecedente = s;
    if (f.smtp_port === '465' || f.smtp_port === '587') f.smtp_port = String(portPourSecurite(s as Securite));
  });

  let etat = $derived.by((): { texte: string; ton: Ton } => {
    if (!b) return { texte: '', ton: 'neutre' };
    if (b.pause_auto) return { texte: 'Arrêtée : connexion refusée', ton: 'erreur' };
    if (!b.active) return b.dernier_test?.ok ? { texte: 'En pause', ton: 'pale' } : { texte: 'Pas encore démarrée', ton: 'alerte' };
    if (b.erreur_depuis) return { texte: 'En erreur', ton: 'alerte' };
    if (!b.derniere_releve) return { texte: 'Première relève en cours', ton: 'info' };
    return { texte: 'Surveillée', ton: 'ok' };
  });
  let nomSociete = $derived(b ? nomCourtSociete(societes.find((s) => s.id === b?.societe_id)?.raison_sociale) : '');
  let optionsSocietes = $derived(
    societes.filter((s) => !s.archivee || String(s.id) === f.societe_id).map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))
  );
  const SECURITES = [
    { valeur: 'tls', libelle: 'TLS (port 465)' },
    { valeur: 'starttls', libelle: 'STARTTLS (port 587)' }
  ];
</script>

<svelte:head><title>{b ? `${b.libelle} · ` : ''}Boîte mail · GMFlow</title></svelte:head>

{#if admin}
  <div class="page etroite">
    <a class="retour" href="/reglages#boites"><Icone nom="gauche" />{RUBRIQUES.boites}</a>

    {#if !b}
      {#if erreur}
        <Bandeau ton="erreur" titre={introuvable ? 'Boîte introuvable.' : 'Boîte pas chargée.'}>
          {erreur}
          {#snippet action()}
            {#if introuvable}<a class="btn sm" href="/reglages#boites">Voir les boîtes</a>{:else}<button class="btn sm" onclick={() => { erreur = ''; void charger(); }}>Réessayer</button>{/if}
          {/snippet}
        </Bandeau>
      {:else}
        <Chargement />
      {/if}
    {:else}
      <div class="page-h">
        <h1>{b.libelle}</h1>
        <Etiquette ton={etat.ton} point>{etat.texte}</Etiquette>
      </div>
      <p class="lead meta">
        {nomSociete}{#if b.libelle !== b.identifiant}{' '}· {b.identifiant}{/if}
        · {b.derniere_releve ? `dernière relève le ${quand(b.derniere_releve)}` : 'pas encore relevée'}
      </p>

      {#if nouvelle && b.active && !b.pause_auto}
        <Bandeau ton="ok" role="status" titre={b.derniere_releve ? 'Surveillance démarrée.' : 'Surveillance démarrée, relève en cours :'}>
          <span class="num">{vus}</span> mails vus{b.derniere_releve ? ' sur la période choisie.' : '.'}
        </Bandeau>
      {/if}
      {#if b.pause_auto}
        <Bandeau ton="erreur" titre="Surveillance arrêtée après {n(b.echecs_auth, 'refus')} de connexion.">
          Vérifiez le mot de passe d’application ci-dessous, essayez la connexion, puis reprenez la surveillance.
        </Bandeau>
      {:else if b.derniere_erreur}
        <Bandeau titre="Dernière erreur{b.erreur_depuis ? `, depuis le ${quand(b.erreur_depuis)}` : ''} :">{b.derniere_erreur}</Bandeau>
      {/if}
      {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
      {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

      <div class="filets etat">
        <div class="ligne bloc fixe">
          <div class="a">
            {#if testEnCours}
              Essai de la connexion en cours…
            {:else if b.dernier_test}
              <Etiquette ton={b.dernier_test.ok ? 'ok' : 'erreur'}>{b.dernier_test.ok ? 'Connexion réussie' : 'Connexion refusée'}</Etiquette>
              <span class="t2 msg">{detailTest(b.dernier_test.message)}</span>
            {:else}
              <span class="t2">Pas encore essayée : l’essai est nécessaire avant de démarrer la surveillance.</span>
            {/if}
          </div>
          <div class="b">{#if b.dernier_test?.quand}Dernier essai le <span class="num">{quand(b.dernier_test.quand)}</span>{/if}</div>
          <div class="r actions">
            <button
              type="button"
              class="btn sm"
              class:en-cours={testEnCours}
              disabled={testEnCours || modifie}
              title={modifie ? 'Enregistrez d’abord les changements' : undefined}
              onclick={tester}>Essayer la connexion</button
            >
            {#if b.active && !b.pause_auto}
              <button type="button" class="btn sm" onclick={relever}><Icone nom="rafraichir" />{RUBRIQUES.releve}</button>
              <button type="button" class="btn sm fantome" onclick={() => (dlgPause = true)}>Mettre en pause</button>
            {:else}
              <button type="button" class="btn sm" disabled={!b.dernier_test?.ok || modifie} title={modifie ? 'Enregistrez d’abord les changements' : undefined} onclick={() => (dlgReprise = true)}>
                {b.derniere_releve || b.pause_auto ? 'Reprendre la surveillance' : 'Démarrer la surveillance'}
              </button>
            {/if}
          </div>
        </div>
      </div>

      <form onsubmit={enregistrer}>
        <h2 class="titre-bloc">Connexion</h2>
        <div class="formulaire">
          <Champ libelle="Nom de la boîte" bind:valeur={f.libelle} requis aide="Tel qu’il apparaît dans GMFlow ; l’adresse par défaut." />
          <Champ libelle="Société" type="select" options={optionsSocietes} bind:valeur={f.societe_id} requis />
          <Champ libelle="Adresse (identifiant)" bind:valeur={f.identifiant} requis autocomplete="off" />
          <Champ
            libelle="Nouveau mot de passe d’application"
            type="password"
            bind:valeur={f.mot_de_passe}
            autocomplete="new-password"
            placeholder="Laisser vide pour le garder"
          />
          <div class="deux">
            <Champ libelle="Serveur de réception" bind:valeur={f.serveur} requis mono aide="Serveur IMAP." />
            <Champ libelle="Port" bind:valeur={f.port} requis mono inputmode="numeric" />
          </div>
          <Champ
            libelle="Historique à relever (mois)"
            bind:valeur={f.reprise_mois}
            mono
            inputmode="numeric"
            aide="De 0 à 24 mois, compté depuis aujourd’hui."
          />
          <p class="plein petit t2">Changer l’adresse, le serveur ou le mot de passe demande un nouvel essai de la connexion.</p>
        </div>

        <h2 class="titre-bloc">Dossiers lus</h2>
        {#if !b.dernier_test?.ok}<p class="petit t2">Essayez la connexion pour voir tous les dossiers du serveur.</p>{/if}
        <div class="dossiers">
          {#each proposes as d (d)}
            <label class="case"><input type="checkbox" value={d} bind:group={f.dossiers} /> {affiche(d)}</label>
          {/each}
        </div>

        <h2 class="titre-bloc">Réponses</h2>
        <p class="petit t2">
          Envoyées depuis cette boîte, avec la même adresse et le même mot de passe. Sans serveur d’envoi, les réponses sont proposées mais ne
          partent pas. Le dossier des envoyés reçoit une copie de chaque réponse et sert d’exemple de ton.
        </p>
        <div class="formulaire">
          <div class="deux">
            <Champ libelle="Serveur d’envoi" bind:valeur={f.smtp_serveur} mono placeholder="Aucun" aide="Serveur SMTP." />
            <Champ libelle="Port" bind:valeur={f.smtp_port} mono inputmode="numeric" placeholder="465" />
          </div>
          <Champ libelle="Sécurité" type="select" options={SECURITES} bind:valeur={f.smtp_securite} />
          <Champ libelle="Dossier des envoyés" type="select" options={optionsEnvoyes} bind:valeur={f.dossier_envoyes} />
          <div class="essai-smtp">
            <button type="button" class="btn sm fantome" onclick={proposerSmtp}>Proposer d’après le serveur de réception</button>
            <button
              type="button"
              class="btn sm"
              class:en-cours={smtpEnCours}
              disabled={smtpEnCours || !b.smtp_serveur || modifie}
              title={!b.smtp_serveur ? 'Enregistrez d’abord un serveur d’envoi' : modifie ? 'Enregistrez d’abord les changements' : undefined}
              onclick={testerSmtp}>Essayer l’envoi</button
            >
          </div>
          {#if essaiSmtp?.resultat_smtp}
            {@const r = essaiSmtp.resultat_smtp}
            <div class="plein">
              <Bandeau ton={r.ok ? 'ok' : 'erreur'}>
                {r.message}{#if r.ok}{' '}Rien n’a été envoyé.{/if}
                {#if !r.ok && r.detail}<div class="detail-brut num">{r.detail}</div>{/if}
              </Bandeau>
            </div>
          {/if}
        </div>

        <div class="form-f">
          <button class="btn principal" class:en-cours={enregistrement} disabled={enregistrement || !modifie || f.dossiers.length === 0}>Enregistrer</button>
          {#if modifie}<button type="button" class="btn fantome" onclick={() => b && remplir(b)}>Annuler les changements</button>{/if}
          {#if f.dossiers.length === 0}<span class="petit flag w">Cochez au moins un dossier</span>{/if}
        </div>
      </form>

      <h2 class="titre-bloc">
        {#if b.reprise_depuis}Relève depuis la réinitialisation du <span class="num">{dateSimple(b.reprise_depuis)}</span>{:else}Relève sur la période ({n(b.reprise_mois, 'mois', 'mois')}){/if}
      </h2>
      <p class="petit t2">À comparer au nombre de messages du webmail, dossier par dossier.</p>
      <table>
        <thead>
          <tr><th>Dossier</th><th class="n">Messages vus</th><th class="n">Mails uniques</th><th class="n">Vus en tout</th><th class="n">Dernier n°</th></tr>
        </thead>
        <tbody>
          {#each compteurs as c (c.dossier)}
            <tr>
              <td>{affiche(c.dossier)}</td>
              <td class="n num">{c.messages_vus}</td>
              <td class="n num">{c.mails_uniques}</td>
              <td class="n num">{c.messages_vus_total}</td>
              <td class="n num">{c.dernier_uid}</td>
            </tr>
          {:else}
            <tr><td colspan="5" class="muted">Rien de relevé pour l’instant.</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <Confirmation bind:ouvert={dlgPause} titre="Mettre la boîte en pause ?" libelle="Mettre en pause" danger onconfirmer={pause}>
    Plus aucun mail n’est relevé jusqu’à la reprise. Rien n’est effacé.
  </Confirmation>
  <Confirmation bind:ouvert={dlgReprise} titre="{b?.derniere_releve ? 'Reprendre' : 'Démarrer'} la surveillance ?" libelle={b?.derniere_releve ? 'Reprendre' : 'Démarrer'} onconfirmer={reprendre}>
    GMFlow cherche les nouveaux mails toutes les 5 minutes{b && !b.derniere_releve
      ? b.reprise_depuis
        ? `, en commençant par les mails reçus depuis le ${dateSimple(b.reprise_depuis)}`
        : `, et relève d’abord l’historique (${n(b.reprise_mois, 'mois', 'mois')})`
      : ''}.
  </Confirmation>
{/if}

<style>
  .retour {
    margin-bottom: var(--e2);
  }
  .page-h {
    margin-bottom: 0;
  }
  .meta {
    margin: var(--e1) 0 var(--e5);
  }
  .page > :global(.bandeau) {
    margin-bottom: var(--e3);
  }
  .etat {
    margin: var(--e2) 0 var(--e2);
  }
  .etat .a {
    white-space: normal;
  }
  .msg {
    font-weight: 400;
    margin-left: var(--e2);
  }
  .actions {
    display: flex;
    gap: var(--e2);
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .ligne.fixe {
    cursor: default;
  }
  .ligne.fixe:hover {
    background: none;
  }
  .titre-bloc {
    font-size: 14px;
    font-weight: 500;
    margin: var(--e8) 0 var(--e2);
  }
  .formulaire p.plein {
    margin: 0;
  }
  .dossiers {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 6px var(--e4);
  }
  .essai-smtp {
    display: flex;
    gap: var(--e2);
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .form-f {
    display: flex;
    gap: var(--e2);
    align-items: center;
    margin: var(--e5) 0 0;
  }
  @media (max-width: 760px) {
    .ligne.bloc {
      grid-template-columns: 1fr;
    }
    .ligne.bloc .r {
      grid-row: auto;
      grid-column: 1;
      justify-content: flex-start;
      margin-top: var(--e2);
    }
  }
</style>
