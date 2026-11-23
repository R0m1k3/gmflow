<script lang="ts">
  // Ajouter une boîte mail (parcours C) : 1 connexion, 2 essai et dossiers, 3 historique.
  // L'essai passe par POST /api/admin/boites/tester (paramètres pas encore enregistrés) ;
  // « Démarrer la surveillance » crée la boîte avec cet essai réussi (essai_id) et l'active d'un coup.
  import { goto } from '$app/navigation';
  import { tick } from 'svelte';
  import { api, attendreEssai, creerBoite, ErreurApi, essayerBoite, message } from '#lib/api.ts';
  import { nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { EssaiBoite, Societe } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import { aideMotDePasse, domaine, portPourSecurite, serveursPour, type Securite } from '../../outils.ts';

  let admin = $derived(session.moi?.role === 'admin');
  $effect(() => {
    if (session.moi && !admin) void goto('/', { replaceState: true });
  });

  let societes = $state<Societe[]>([]);
  let erreurChargement = $state('');
  $effect(() => {
    if (!admin) return;
    api
      .get<Societe[]>('/api/admin/societes')
      .then((l) => {
        societes = l.filter((s) => !s.archivee);
        if (societes.length === 1 && !f.societe_id) f.societe_id = String(societes[0].id);
      })
      .catch((e) => (erreurChargement = message(e)));
  });

  let ecran = $state<1 | 2 | 3>(1);
  let f = $state({
    adresse: '',
    mot_de_passe: '',
    societe_id: '',
    imap_serveur: '',
    imap_port: '993',
    smtp_serveur: '',
    smtp_port: '465',
    smtp_securite: 'tls',
    dossiers: ['INBOX'] as string[],
    dossier_envoyes: '',
    repondre: true,
    reprise_mois: '3'
  });
  let voirMdp = $state(false);
  let changerServeurs = $state(false);

  // ---------- Écran 1 : serveurs d'après le domaine ----------
  let propose = $derived(serveursPour(f.adresse));
  let adresseValide = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(f.adresse.trim()));
  let reconnu = $derived(adresseValide && propose.hebergeur !== null);
  let champsServeurs = $derived(adresseValide && (!reconnu || changerServeurs));
  let nomHebergeur = $derived(propose.hebergeur === 'gmail' ? 'Gmail' : 'Infomaniak');
  // Le préremplissage suit le domaine tant que l'admin n'a pas touché aux serveurs.
  let domaineRempli = '';
  $effect(() => {
    const d = domaine(f.adresse);
    if (!adresseValide || d === domaineRempli) return;
    domaineRempli = d;
    if (changerServeurs) return;
    f.imap_serveur = propose.imap.serveur;
    f.imap_port = String(propose.imap.port);
    f.smtp_serveur = propose.smtp.serveur;
    f.smtp_port = String(propose.smtp.port);
    f.smtp_securite = propose.smtp.securite;
  });
  let securitePrecedente = 'tls';
  $effect(() => {
    const s = f.smtp_securite;
    if (s === securitePrecedente) return;
    securitePrecedente = s;
    if (f.smtp_port === '465' || f.smtp_port === '587') f.smtp_port = String(portPourSecurite(s as Securite));
  });

  function suivant1(e: SubmitEvent) {
    e.preventDefault();
    if (!f.societe_id) return;
    ecran = 2;
    void essayer();
  }

  // ---------- Écran 2 : essai ----------
  let essai = $state<EssaiBoite | null>(null);
  let essaiId = $state<string | null>(null);
  let essaiEnCours = $state(false);
  let erreurEssai = $state('');
  async function essayer() {
    essai = null;
    essaiId = null;
    erreurEssai = '';
    essaiEnCours = true;
    try {
      const { id } = await essayerBoite({
        serveur: f.imap_serveur.trim(),
        port: Number(f.imap_port) || 993,
        identifiant: f.adresse.trim(),
        mot_de_passe: f.mot_de_passe,
        smtp_serveur: f.smtp_serveur.trim() || undefined,
        smtp_port: f.smtp_serveur.trim() ? Number(f.smtp_port) || undefined : undefined,
        smtp_securite: f.smtp_serveur.trim() ? (f.smtp_securite as Securite) : undefined
      });
      const r = await attendreEssai(id);
      essai = r;
      essaiId = id;
      const imap = r.resultat_imap;
      if (imap?.ok) {
        // Réception cochée, Envoyés repéré ; on garde les choix d'un essai précédent s'ils existent encore.
        const noms = new Set(imap.dossiers.map((d) => d.nom));
        const gardes = f.dossiers.filter((d) => noms.has(d));
        f.dossiers = gardes.length ? gardes : [imap.reception ?? 'INBOX'];
        if (!f.dossier_envoyes || !noms.has(f.dossier_envoyes)) f.dossier_envoyes = imap.envoyes ?? '';
      }
      f.repondre = !r.smtp || !!r.resultat_smtp?.ok;
    } catch (e) {
      erreurEssai = message(e);
    } finally {
      essaiEnCours = false;
    }
  }
  let imap = $derived(essai?.resultat_imap ?? null);
  let smtp = $derived(essai?.resultat_smtp ?? null);
  let dossiersLisibles = $derived(imap ? imap.dossiers.filter((d) => !d.non_selectionnable) : []);
  let optionsEnvoyes = $derived([
    { valeur: '', libelle: 'Aucun (pas de copie des réponses)' },
    ...dossiersLisibles.map((d) => ({ valeur: d.nom, libelle: d.affiche }))
  ]);

  async function retour1() {
    ecran = 1;
    await tick();
    document.getElementById('mdp')?.focus();
  }

  // ---------- Écran 3 : historique et démarrage ----------
  const HISTORIQUES = [
    { mois: '0', titre: 'Rien', texte: 'seulement les mails qui arrivent à partir de maintenant' },
    { mois: '1', titre: '1 mois', texte: '' },
    { mois: '3', titre: '3 mois', texte: 'conseillé' },
    { mois: '6', titre: '6 mois', texte: '' },
    { mois: '12', titre: '12 mois', texte: 'la première relève peut prendre du temps' }
  ];
  let demarrage = $state('');
  let erreurDemarrage = $state('');

  async function demarrer() {
    erreurDemarrage = '';
    if (!essaiId) return;
    const avecSmtp = f.repondre && !!f.smtp_serveur.trim();
    try {
      // L'essai réussi de l'écran 2 sert de test de la boîte : création et surveillance en un appel.
      demarrage = 'Démarrage de la surveillance';
      const b = await creerBoite({
        societe_id: Number(f.societe_id),
        libelle: f.adresse.trim(),
        serveur: f.imap_serveur.trim(),
        port: Number(f.imap_port) || 993,
        identifiant: f.adresse.trim(),
        mot_de_passe: f.mot_de_passe,
        dossiers: f.dossiers,
        reprise_mois: Number(f.reprise_mois),
        smtp_serveur: avecSmtp ? f.smtp_serveur.trim() : null,
        smtp_port: avecSmtp ? Number(f.smtp_port) || null : null,
        smtp_securite: avecSmtp ? (f.smtp_securite as Securite) : null,
        dossier_envoyes: f.dossier_envoyes || null,
        essai_id: essaiId,
        activer: true
      });
      await goto(`/reglages/boites/${b.id}?nouvelle=1`);
    } catch (e) {
      demarrage = '';
      // Essai trop ancien ou paramètres changés depuis : il est refait, puis on recommence.
      if (e instanceof ErreurApi && e.raison === 'essai') {
        erreurDemarrage = 'L’essai de connexion a plus d’une heure : il est refait. Cliquez à nouveau quand il a réussi.';
        await essayer();
        if (!essai?.resultat_imap?.ok) ecran = 2;
      } else erreurDemarrage = message(e);
    }
  }

  let nomSociete = $derived(nomCourtSociete(societes.find((s) => String(s.id) === f.societe_id)?.raison_sociale));
  const SECURITES = [
    { valeur: 'tls', libelle: 'TLS (port 465)' },
    { valeur: 'starttls', libelle: 'STARTTLS (port 587)' }
  ];
</script>

<svelte:head><title>Ajouter une boîte mail · GMFlow</title></svelte:head>

{#if admin}
  <div class="page etroite">
    <a class="retour" href="/reglages#boites"><Icone nom="gauche" />Réglages</a>
    <div class="page-h">
      <h1>Ajouter une boîte mail</h1>
    </div>

    <div class="sub" aria-label="Étapes">
      <span class:cur={ecran === 1} class:fait={ecran > 1}><i>1</i> Connexion</span>
      <span class:cur={ecran === 2} class:fait={ecran > 2}><i>2</i> Dossiers</span>
      <span class:cur={ecran === 3}><i>3</i> Historique</span>
    </div>

    {#if erreurChargement}<Bandeau ton="erreur">{erreurChargement}</Bandeau>{/if}

    {#if ecran === 1}
      <form class="form" onsubmit={suivant1}>
        <Champ
          libelle="Adresse"
          type="email"
          bind:valeur={f.adresse}
          requis
          autocomplete="off"
          placeholder="compta@houdemont-distribution.fr"
          aide="L’adresse où arrivent les factures et les mails des fournisseurs."
        />
        <div class="champ">
          <label for="mdp">Mot de passe d’application</label>
          <div class="mdp">
            <input id="mdp" type={voirMdp ? 'text' : 'password'} bind:value={f.mot_de_passe} required autocomplete="new-password" class="num-saisie" />
            <button type="button" class="btn-lien voir" onclick={() => (voirMdp = !voirMdp)}>{voirMdp ? 'Masquer' : 'Afficher'}</button>
          </div>
          <div class="aide">{aideMotDePasse(propose.hebergeur)}</div>
        </div>

        {#if reconnu && !changerServeurs}
          <div class="detect full">
            <Icone nom="coche" />
            <div>
              <b>Boîte {nomHebergeur} reconnue.</b> Serveurs préremplis : réception
              <span class="num">{f.imap_serveur}:{f.imap_port}</span>, envoi <span class="num">{f.smtp_serveur}:{f.smtp_port}</span>.
              <button type="button" class="btn-lien discret" onclick={() => (changerServeurs = true)}>Changer les serveurs</button>
            </div>
          </div>
        {:else if champsServeurs}
          <div class="detect full">
            <Icone nom="info" />
            <div>
              {#if reconnu}
                Serveurs de {nomHebergeur}, à modifier si besoin.
              {:else}
                <b>Serveurs préremplis pour Infomaniak</b> (réception <span class="num">mail.infomaniak.com:993</span>, envoi
                <span class="num">mail.infomaniak.com:465</span>). Boîte ailleurs : indiquez les serveurs de l’hébergeur.
              {/if}
            </div>
          </div>
          <div class="serveurs full">
            <Champ libelle="Serveur de réception" bind:valeur={f.imap_serveur} requis mono placeholder="mail.infomaniak.com" aide="Serveur IMAP, port 993 en général." />
            <Champ libelle="Port" bind:valeur={f.imap_port} requis mono inputmode="numeric" />
            <Champ
              libelle="Serveur d’envoi"
              bind:valeur={f.smtp_serveur}
              mono
              placeholder="mail.infomaniak.com"
              aide="Serveur SMTP, pour envoyer les réponses. Vide : réponses proposées, mais rien ne part."
            />
            <Champ libelle="Port" bind:valeur={f.smtp_port} mono inputmode="numeric" />
            <div class="full-s"><Champ libelle="Sécurité de l’envoi" type="select" options={SECURITES} bind:valeur={f.smtp_securite} /></div>
          </div>
        {/if}

        <Champ
          libelle="Société"
          type="select"
          options={[{ valeur: '', libelle: 'Choisir…' }, ...societes.map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))]}
          bind:valeur={f.societe_id}
          requis
          aide="Les documents reçus ici sont rangés dans cette société, sauf pour un fournisseur déjà rangé ailleurs."
        />
        {#if societes.length === 0 && !erreurChargement}
          <p class="petit t2 full">Aucune société : <a href="/reglages#societes">ajoutez-en une</a> d’abord.</p>
        {/if}

        <div class="form-f full">
          <button class="btn principal" disabled={!adresseValide || !f.mot_de_passe || !f.societe_id || !f.imap_serveur.trim()}>
            Suivant : essayer la connexion
          </button>
          <a class="btn fantome" href="/reglages#boites">Annuler</a>
        </div>
      </form>
    {:else if ecran === 2}
      <div class="ecran">
        <p class="t2">
          Connexion à <span class="num">{f.imap_serveur}</span> avec <b>{f.adresse}</b>.
        </p>
        {#if essaiEnCours}
          <Chargement texte="Essai de la connexion, quelques secondes" />
        {:else if erreurEssai}
          <Bandeau ton="erreur" titre="L’essai n’a pas abouti.">{erreurEssai}</Bandeau>
        {:else if imap && !imap.ok}
          <Bandeau ton="erreur" titre="Connexion impossible.">
            {imap.message}
            {#if imap.detail}<div class="detail-brut num">{imap.detail}</div>{/if}
          </Bandeau>
        {:else if imap?.ok}
          <Bandeau ton="ok" role="status">{imap.message}</Bandeau>

          <h2 class="titre-bloc">Dossiers à lire</h2>
          <p class="petit t2">Les mails de ces dossiers sont relevés toutes les 5 minutes. Réception suffit le plus souvent.</p>
          <div class="dossiers">
            {#each dossiersLisibles as d (d.nom)}
              <label class="case">
                <input type="checkbox" value={d.nom} bind:group={f.dossiers} />
                <span>{d.nom === imap.reception ? 'Réception' : d.affiche}{#if d.nom === imap.envoyes}{' '}<span class="muted">(envoyés)</span>{/if}</span>
              </label>
            {/each}
          </div>

          <h2 class="titre-bloc">Réponses</h2>
          <Champ
            libelle="Dossier des envoyés"
            type="select"
            options={optionsEnvoyes}
            bind:valeur={f.dossier_envoyes}
            aide={imap.envoyes ? 'Repéré sur le serveur. Une copie de chaque réponse y est rangée ; ses mails servent d’exemples de ton.' : 'Pas repéré tout seul : choisissez-le pour garder une copie des réponses.'}
          />
          {#if smtp}
            {#if smtp.ok}
              <p class="ok-l"><Icone nom="coche" />Envoi des réponses : {smtp.message}</p>
            {:else}
              <Bandeau titre="Les réponses ne pourront pas partir.">
                {smtp.message}
                {#if smtp.detail}<div class="detail-brut num">{smtp.detail}</div>{/if}
              </Bandeau>
            {/if}
            <label class="case">
              <input type="checkbox" bind:checked={f.repondre} />
              <span>Envoyer les réponses depuis cette boîte<span class="aide">Sinon, les réponses sont proposées mais ne partent pas. Modifiable ensuite dans la fiche de la boîte.</span></span>
            </label>
          {:else}
            <p class="petit t2">Pas de serveur d’envoi : les réponses seront proposées mais ne partiront pas.</p>
          {/if}
        {/if}

        <div class="form-f">
          {#if imap?.ok && !essaiEnCours}
            <button type="button" class="btn principal" disabled={f.dossiers.length === 0} onclick={() => (ecran = 3)}>Suivant : historique</button>
            <button type="button" class="btn fantome" onclick={retour1}>Retour</button>
          {:else if !essaiEnCours}
            <button type="button" class="btn principal" onclick={retour1}>Corriger</button>
            <button type="button" class="btn" onclick={essayer}>Réessayer</button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="ecran">
        <h2 class="titre-bloc premier">Historique à relever</h2>
        <p class="petit t2">Les factures reçues sur cette période sont lues et proposées à la validation.</p>
        <div class="historiques" role="radiogroup" aria-label="Historique à relever">
          {#each HISTORIQUES as h (h.mois)}
            <label class="case">
              <input type="radio" name="historique" value={h.mois} bind:group={f.reprise_mois} />
              <span>{h.titre}{#if h.texte}{' '}<span class="muted">· {h.texte}</span>{/if}</span>
            </label>
          {/each}
        </div>

        <div class="filets recap">
          <div class="ligne bloc fixe"><div class="a">{f.adresse}</div><div class="b">{nomSociete} · réception <span class="num">{f.imap_serveur}</span></div></div>
          <div class="ligne bloc fixe">
            <div class="a">{f.dossiers.length > 1 ? `${f.dossiers.length} dossiers lus` : '1 dossier lu'}</div>
            <div class="b">
              {f.dossiers.map((d) => (d === imap?.reception ? 'Réception' : (imap?.dossiers.find((x) => x.nom === d)?.affiche ?? d))).join(', ')}
              · {f.repondre && f.smtp_serveur ? 'réponses envoyées depuis cette boîte' : 'réponses pas envoyées'}
            </div>
          </div>
        </div>

        {#if erreurDemarrage}<Bandeau ton="erreur" titre="La surveillance n’a pas démarré.">{erreurDemarrage}</Bandeau>{/if}

        <div class="form-f">
          <button type="button" class="btn principal" class:en-cours={!!demarrage || essaiEnCours} disabled={!!demarrage || essaiEnCours || !essaiId} onclick={demarrer}>
            {demarrage || 'Démarrer la surveillance'}
          </button>
          {#if !demarrage}<button type="button" class="btn fantome" onclick={() => (ecran = 2)}>Retour</button>{/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Assistant : sous-étapes, encadré « boîte reconnue » (docs/maquette-b.html, écran 4). */
  .retour {
    margin-bottom: var(--e2);
  }
  .sub {
    display: flex;
    gap: var(--e5);
    margin-bottom: var(--e5);
    font-size: 12px;
    color: var(--muted);
  }
  .sub span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sub span.cur {
    color: var(--text);
    font-weight: 500;
  }
  .sub span.fait {
    color: var(--text2);
  }
  .sub i {
    font-style: normal;
    font-family: var(--mono);
    font-size: 12px;
  }
  .form {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--e4) var(--e6);
    max-width: 760px;
  }
  .form .full {
    grid-column: 1 / -1;
  }
  .detect {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px var(--e3);
    border-radius: var(--r2);
    background: var(--pane);
    font-size: 12px;
    line-height: 18px;
    color: var(--text2);
  }
  .detect :global(svg.i) {
    margin-top: 1px;
  }
  .detect b {
    color: var(--text);
    font-weight: 500;
  }
  .detect .num {
    font-size: 12px;
  }
  .discret {
    color: var(--text2);
  }
  .serveurs {
    display: grid;
    grid-template-columns: 1fr 96px;
    gap: var(--e4) var(--e3);
  }
  .full-s {
    grid-column: 1 / -1;
    max-width: 50%;
  }
  .mdp {
    position: relative;
  }
  .mdp input {
    padding-right: 72px;
  }
  .voir {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted);
    text-decoration: none;
    font-size: 12px;
  }
  .form-f {
    display: flex;
    gap: var(--e2);
    align-items: center;
    margin-top: var(--e4);
  }
  .ecran {
    max-width: 640px;
  }
  .ecran > :global(.bandeau) {
    margin-bottom: var(--e4);
  }
  .titre-bloc {
    font-size: 14px;
    font-weight: 500;
    margin: var(--e6) 0 var(--e1);
  }
  .titre-bloc.premier {
    margin-top: 0;
  }
  .dossiers {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 6px var(--e4);
    margin-top: var(--e2);
  }
  .historiques {
    display: flex;
    flex-direction: column;
    gap: var(--e2);
    margin: var(--e3) 0 var(--e5);
  }
  .ecran > .case {
    margin-top: var(--e3);
  }
  .ok-l {
    display: flex;
    align-items: center;
    gap: var(--e2);
    color: var(--ok);
    margin: var(--e3) 0;
  }
  .recap {
    margin-bottom: var(--e4);
  }
  .ligne.fixe {
    cursor: default;
  }
  .ligne.fixe:hover {
    background: none;
  }
  @media (max-width: 760px) {
    .form {
      grid-template-columns: 1fr;
    }
    .full-s {
      max-width: none;
    }
  }
</style>
