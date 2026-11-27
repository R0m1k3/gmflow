<script lang="ts">
  // Maintenance, en trois vues (U10, `partie`) : Avancé (modèles et effort par tâche, limites
  // par jour, appels à Claude, expéditeurs publicitaires), Journal (qui a changé quoi) et Zone
  // dangereuse (réinitialisation : données tirées des mails effacées, configuration gardée).
  import { ErreurApi, api, message } from '#lib/api.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES, libelleTache } from '#lib/libelles.ts';
  import type { Appels, BilanReinitialisation, ExpediteurMarque, LigneJournal, Page, Reglages, ReglagesVue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Repli from '#lib/ui/Repli.svelte';
  import { OBJETS_HISTORIQUE, duree, libelleAction, libelleObjet, n } from './outils.ts';

  let { onchange, partie = 'avance' }: { onchange?: () => void; partie?: 'avance' | 'journal' | 'danger' } = $props();

  let erreur = $state('');
  let info = $state('');

  // ---------- Modèles, effort et limites ----------
  let vue = $state<ReglagesVue | null>(null);
  let reglages = $state<Reglages | null>(null);
  let enCours = $state(false);
  const LECTURE = ['extraction_texte', 'ocr_scan', 'nouvel_essai', 'identification_fournisseur'] as const;
  const MAILS = ['tri_mail', 'brouillon_reponse'] as const;

  async function chargerReglages() {
    const v = await api.get<ReglagesVue>('/api/admin/claude/reglages');
    vue = v;
    // Copie des seuls réglages (sans les compteurs ni les listes de choix).
    const { appels_lecture_du_jour: _l, appels_tri_du_jour: _t, modeles: _m, efforts: _e, ...r } = v;
    reglages = JSON.parse(JSON.stringify(r)) as Reglages;
  }
  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!reglages) return;
    erreur = '';
    info = '';
    enCours = true;
    try {
      await api.put('/api/admin/claude/reglages', {
        ...reglages,
        quota_lecture_jour: Number(reglages.quota_lecture_jour),
        quota_tri_jour: Number(reglages.quota_tri_jour),
        tri_depuis_jours: Number(reglages.tri_depuis_jours)
      });
      await chargerReglages();
      info = 'Réglages avancés enregistrés.';
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  // ---------- Expéditeurs publicitaires ----------
  let marques = $state<ExpediteurMarque[] | null>(null);
  async function chargerMarques() {
    marques = await api.get<ExpediteurMarque[]>('/api/admin/expediteurs-marques');
  }
  let aRetirer = $state<ExpediteurMarque | null>(null);
  let dlgRetirer = $state(false);
  async function retirer() {
    if (!aRetirer) return;
    erreur = '';
    try {
      await api.del(`/api/admin/expediteurs-marques/${aRetirer.id}`);
      await chargerMarques();
    } catch (err) {
      erreur = message(err);
    }
  }
  const nomMarque = (m: ExpediteurMarque) => (m.domaine ? `Tout le domaine ${m.domaine}` : (m.adresse ?? ''));

  $effect(() => {
    if (partie !== 'avance') return;
    chargerReglages().catch((e) => (erreur = message(e)));
    chargerMarques().catch((e) => (erreur = message(e)));
  });

  // ---------- Appels à Claude (chargés à l'ouverture) ----------
  let ouvertAppels = $state(false);
  let appels = $state<Appels | null>(null);
  let pageAppels = $state(1);
  let pagesAppels = $derived(appels ? Math.max(1, Math.ceil((appels.total ?? 0) / 50)) : 1);
  async function chargerAppels() {
    try {
      appels = await api.get<Appels>(`/api/admin/claude/appels${pageAppels > 1 ? `?page=${pageAppels}` : ''}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    if (ouvertAppels && !appels) void chargerAppels();
  });
  function allerAppels(p: number) {
    pageAppels = p;
    void chargerAppels();
  }

  // ---------- Historique (chargé à l'ouverture) ----------
  // Vue Journal : l'historique est toute la page, chargé d'emblée.
  let ouvertHistorique = $derived(partie === 'journal');
  let objet = $state('');
  let pageHist = $state(1);
  let historique = $state<Page<LigneJournal> | null>(null);
  let pagesHist = $derived(historique ? Math.max(1, Math.ceil((historique.total ?? 0) / 50)) : 1);
  async function chargerHistorique() {
    const q = new URLSearchParams({ page: String(pageHist) });
    if (objet) q.set('objet_type', objet);
    try {
      historique = await api.get<Page<LigneJournal>>(`/api/admin/journal?${q}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    if (ouvertHistorique && !historique) void chargerHistorique();
  });
  function allerHist(p: number) {
    pageHist = p;
    void chargerHistorique();
  }
  function details(d: Record<string, unknown> | null | undefined): string {
    if (!d) return '';
    return Object.entries(d)
      .map(([k, v]) => `${k.replace(/_/g, ' ')} : ${v !== null && typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
      .join(' · ');
  }

  // ---------- Zone dangereuse : réinitialisation ----------
  const MOT = 'RÉINITIALISER';
  let dlgReinit = $state(false);
  let tape = $state('');
  let reinitEnCours = $state(false);
  let erreurReinit = $state('');
  // Refus du serveur (400, 409) : la transaction n'a rien effacé. Sinon, on ne sait pas.
  let refusReinit = $state(false);
  let bilanReinit = $state<BilanReinitialisation | null>(null);
  let motJuste = $derived(tape.trim() === MOT);
  function ouvrirReinit() {
    tape = '';
    erreurReinit = '';
    dlgReinit = true;
  }
  async function reinitialiser() {
    erreurReinit = '';
    bilanReinit = null;
    reinitEnCours = true;
    try {
      bilanReinit = await api.post<BilanReinitialisation>('/api/admin/reinitialiser', { confirmation: tape.trim() });
      appels = null;
      historique = null;
      if (ouvertAppels) void chargerAppels();
      if (ouvertHistorique) void chargerHistorique();
      void rafraichirCompteurs();
      onchange?.();
    } catch (e) {
      erreurReinit = message(e);
      refusReinit = e instanceof ErreurApi && e.code < 500;
    } finally {
      reinitEnCours = false;
    }
  }

  let resumeModeles = $derived(
    reglages ? `lecture ${reglages.lecture_auto ? 'automatique' : 'coupée'}, tri ${reglages.tri_auto ? 'automatique' : 'coupé'}` : ''
  );
  let resumeLimites = $derived(
    reglages ? `lecture ${reglages.quota_lecture_jour}, tri ${reglages.quota_tri_jour} appels par jour` : ''
  );
</script>

{#snippet modeleEffort(t: (typeof LECTURE)[number] | (typeof MAILS)[number])}
  {#if vue && reglages}
    <tr>
      <td>{libelleTache(t)}</td>
      <td>
        <select bind:value={reglages[t].modele} aria-label="Modèle : {libelleTache(t)}">
          {#each vue.modeles as m (m)}<option value={m}>{m}</option>{/each}
        </select>
      </td>
      <td>
        <select bind:value={reglages[t].effort} aria-label="Effort : {libelleTache(t)}">
          {#each vue.efforts as ef (ef)}<option value={ef}>{ef}</option>{/each}
        </select>
      </td>
    </tr>
  {/if}
{/snippet}

{#snippet pagination(p: number, total: number, aller: (n: number) => void)}
  {#if total > 1}
    <div class="pages">
      <button type="button" class="btn sm" disabled={p <= 1} onclick={() => aller(p - 1)}>Précédent</button>
      <span class="petit t2">Page <span class="num">{p}</span> sur <span class="num">{total}</span></span>
      <button type="button" class="btn sm" disabled={p >= total} onclick={() => aller(p + 1)}>Suivant</button>
    </div>
  {/if}
{/snippet}

{#if partie === 'avance'}
<section class="sujet" id="avance">
  <div class="sujet-h"><h2>{RUBRIQUES.avance}</h2></div>
  <p class="lead">Rarement utile : le choix des modèles de Claude, les limites de dépense, les appels faits.</p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  <div class="carte-r">
    <Repli icone="curseurs" titre="Modèles et effort par tâche" resume={resumeModeles}>
      {#if vue && reglages}
        <form onsubmit={enregistrer}>
          <table class="modeles">
            <thead><tr><th>Tâche</th><th>Modèle</th><th>Effort</th></tr></thead>
            <tbody>
              <tr class="groupe"><td colspan="3">Lecture des documents</td></tr>
              {#each LECTURE as t (t)}{@render modeleEffort(t)}{/each}
              <tr class="groupe"><td colspan="3">Mails</td></tr>
              {#each MAILS as t (t)}{@render modeleEffort(t)}{/each}
            </tbody>
          </table>
          <label class="case">
            <input type="checkbox" bind:checked={reglages.lecture_auto} />
            <span>Lecture automatique des nouvelles pièces jointes<span class="aide">Coupée, rien n’est lu ; « Relire » sur un document marche toujours.</span></span>
          </label>
          <label class="case">
            <input type="checkbox" bind:checked={reglages.tri_auto} />
            <span>Tri automatique des nouveaux mails<span class="aide">Les réponses ne sont jamais proposées toutes seules : elles se demandent mail par mail.</span></span>
          </label>
          <div class="form-f"><button class="btn" class:en-cours={enCours} disabled={enCours}>Enregistrer</button></div>
        </form>
      {:else}
        <Chargement compact />
      {/if}
    </Repli>

    <Repli icone="graphique" titre={RUBRIQUES.limites} resume={resumeLimites}>
      {#if vue && reglages}
        <form onsubmit={enregistrer}>
          <div class="formulaire">
            <div class="champ">
              <label for="lim-lecture">Lecture des documents (appels par jour)</label>
              <input id="lim-lecture" class="num" type="number" min="0" bind:value={reglages.quota_lecture_jour} />
              <div class="aide"><span class="num">{vue.appels_lecture_du_jour}</span> aujourd’hui. Au-delà, les documents attendent le lendemain.</div>
            </div>
            <div class="champ">
              <label for="lim-tri">Tri des mails (appels par jour)</label>
              <input id="lim-tri" class="num" type="number" min="0" bind:value={reglages.quota_tri_jour} />
              <div class="aide">
                <span class="num">{vue.appels_tri_du_jour}</span> aujourd’hui. Les listes, les « no-reply » et les expéditeurs publicitaires ne coûtent rien.
              </div>
            </div>
            <div class="champ">
              <label for="tri-depuis">Trier les mails reçus depuis (jours)</label>
              <input id="tri-depuis" class="num" type="number" min="1" max="365" bind:value={reglages.tri_depuis_jours} />
              <div class="aide">Les mails plus anciens restent sans tri.</div>
            </div>
          </div>
          <div class="form-f"><button class="btn" class:en-cours={enCours} disabled={enCours}>Enregistrer</button></div>
        </form>
      {:else}
        <Chargement compact />
      {/if}
    </Repli>

    <Repli icone="prise" titre="Appels à Claude" resume="les 30 derniers jours" bind:ouvert={ouvertAppels}>
      {#if !appels}
        <Chargement compact />
      {:else}
        {#if appels.stats.length}
          <table class="stats">
            <thead><tr><th>Sur 30 jours</th><th>Modèle</th><th class="n">Appels</th><th class="n">Réussis</th><th class="n">Durée moyenne</th></tr></thead>
            <tbody>
              {#each appels.stats as s (s.tache + s.modele)}
                <tr>
                  <td>{libelleTache(s.tache)}</td>
                  <td>{s.modele}</td>
                  <td class="n num">{s.n}</td>
                  <td class="n num">{s.reussis} ({Math.round((100 * s.reussis) / Math.max(1, s.n))} %)</td>
                  <td class="n num">{duree(s.duree_moyenne_ms)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
        <table>
          <thead><tr><th>Quand</th><th>Tâche</th><th>Modèle</th><th class="n">Durée</th><th>Résultat</th></tr></thead>
          <tbody>
            {#each appels.elements as a, i (i)}
              <tr>
                <td class="num">{quand(a.quand)}</td>
                <td>{libelleTache(a.tache)}</td>
                <td>{a.modele}{a.effort ? `, ${a.effort}` : ''}</td>
                <td class="n num">{duree(a.duree_ms)}</td>
                <td>
                  <Etiquette ton={a.succes ? 'ok' : 'erreur'}>{a.succes ? 'Réussi' : 'Échec'}</Etiquette>
                  {#if a.message}<span class="petit t2">{a.message}</span>{/if}
                </td>
              </tr>
            {:else}
              <tr><td colspan="5" class="muted">Aucun appel.</td></tr>
            {/each}
          </tbody>
        </table>
        {@render pagination(pageAppels, pagesAppels, allerAppels)}
      {/if}
    </Repli>

    <Repli icone="drapeau" titre={RUBRIQUES.expediteursPub} resume={marques ? n(marques.length, 'expéditeur') : ''}>
      <p class="petit t2">
        Ajoutés depuis la fiche d’un mail (« Toujours publicité »). Leurs mails sont classés sans appel à Claude et effacés après 90 jours.
      </p>
      {#if !marques}
        <Chargement compact />
      {:else}
        <div class="filets">
          {#each marques as m (m.id)}
            <div class="ligne bloc fixe">
              <div class="a">{nomMarque(m)}</div>
              <div class="b">Ajouté le <span class="num">{quand(m.cree_le)}</span>{m.cree_par ? ` par ${m.cree_par}` : ''}</div>
              <div class="r">
                <button type="button" class="btn sm fantome" onclick={() => { aRetirer = m; dlgRetirer = true; }}>Retirer</button>
              </div>
            </div>
          {:else}
            <p class="muted petit vide-l">Aucun expéditeur publicitaire.</p>
          {/each}
        </div>
      {/if}
    </Repli>

  </div>
</section>
{:else if partie === 'journal'}
<section class="sujet" id="journal">
  <div class="sujet-h"><h2>{RUBRIQUES.journal}</h2></div>
  <p class="lead">Qui a changé quoi, et quand : corrections, validations, envois, réglages.</p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
      <div class="filtre">
        <label for="hist-objet" class="petit t2">Afficher</label>
        <select id="hist-objet" bind:value={objet} onchange={() => allerHist(1)}>
          {#each OBJETS_HISTORIQUE as o (o.valeur)}<option value={o.valeur}>{o.libelle}</option>{/each}
        </select>
      </div>
      {#if !historique}
        <Chargement compact />
      {:else}
        <div class="journal-t">
        <table>
          <thead><tr><th>Quand</th><th>Qui</th><th>Quoi</th><th>Détails</th></tr></thead>
          <tbody>
            {#each historique.elements as l, i (i)}
              <tr>
                <td class="num quand">{quand(l.quand)}</td>
                <td>{l.auteur ?? 'GMFlow'}</td>
                <td>
                  {libelleAction(l.action)}
                  <span class="t2">· {libelleObjet(l.objet_type)}{#if l.objet_id}&nbsp;<span class="num">{l.objet_id}</span>{/if}</span>
                </td>
                <td class="details petit t2">{details(l.details)}</td>
              </tr>
            {:else}
              <tr><td colspan="4" class="muted">Rien pour l’instant.</td></tr>
            {/each}
          </tbody>
        </table>
        </div>
        {@render pagination(pageHist, pagesHist, allerHist)}
      {/if}
</section>
{:else}
<section class="sujet" id="zone-dangereuse">
  <div class="sujet-h"><h2>Zone dangereuse</h2></div>
  <p class="lead">Ce qui ne se défait pas. À n’utiliser qu’en connaissance de cause.</p>
  <section class="danger-zone" aria-label="Zone dangereuse">
    <div class="dz-ligne">
      <div class="dz-texte">
        <div class="dz-titre">Tout reprendre de zéro</div>
        <p>
          Efface tout ce qui vient des mails (mails, pièces jointes, factures et BL, fiches fournisseurs avec leurs codes
          BO Phénix, réponses) et relève à nouveau chaque boîte à partir des mails du jour ; garde les sociétés, les boîtes mail, les
          utilisateurs, la connexion à Claude, les réglages, l’historique et la trace des dépôts Dext/Yooz.
        </p>
      </div>
      <button type="button" class="btn principal danger" class:en-cours={reinitEnCours} disabled={reinitEnCours} onclick={ouvrirReinit}>
        {reinitEnCours ? 'Réinitialisation en cours' : 'Réinitialiser…'}
      </button>
    </div>
    {#if reinitEnCours}
      <Bandeau ton="info" role="status">Réinitialisation en cours, ne fermez pas la page.</Bandeau>
    {:else if erreurReinit}
      <Bandeau ton="erreur" titre={refusReinit ? 'Rien n’a été effacé.' : 'Réinitialisation pas confirmée.'}>{erreurReinit}</Bandeau>
    {:else if bilanReinit}
      <Bandeau ton="ok" role="status" titre="Tout est effacé, la relève a repris.">
        <span class="num">{bilanReinit.mails}</span> mails, <span class="num">{bilanReinit.documents}</span> documents et
        <span class="num">{bilanReinit.fournisseurs}</span> fournisseurs effacés ;
        {n(bilanReinit.boites_relancees, 'boîte')} en cours de relève. Les mails du jour reviennent dans quelques minutes.
      </Bandeau>
    {/if}
  </section>
</section>
{/if}

<Confirmation
  bind:ouvert={dlgReinit}
  titre="Tout effacer et reprendre de zéro ?"
  libelle="Réinitialiser"
  danger
  desactive={!motJuste}
  onconfirmer={reinitialiser}
>
  <p>
    Les mails, documents, fournisseurs, liens et réponses seront effacés pour de bon. Les boîtes repartent
    ensuite des mails du jour : les mails plus anciens ne sont pas relus. La configuration, l’historique et la trace des
    dépôts restent.
  </p>
  <label class="dz-mot" for="dz-mot">Tapez <strong class="num">{MOT}</strong> pour confirmer</label>
  <input
    id="dz-mot"
    class="num"
    type="text"
    autocomplete="off"
    spellcheck="false"
    bind:value={tape}
    onkeydown={(e) => {
      if (e.key === 'Enter' && motJuste) {
        e.preventDefault();
        dlgReinit = false;
        void reinitialiser();
      }
    }}
  />
</Confirmation>

<Confirmation bind:ouvert={dlgRetirer} titre="Ne plus classer en publicité ?" libelle="Retirer" onconfirmer={retirer}>
  {aRetirer ? nomMarque(aRetirer) : ''} : ses prochains mails seront triés comme les autres.
</Confirmation>

<style>
  table {
    margin-bottom: var(--e3);
  }
  .modeles select {
    height: 36px;
    max-width: 14rem;
  }
  .modeles tr.groupe td {
    color: var(--muted);
    font-size: 14px;
    font-weight: 500;
    padding-top: var(--e3);
  }
  .case {
    margin: var(--e2) 0;
  }
  .stats {
    margin-bottom: var(--e5);
  }
  .pages {
    display: flex;
    align-items: center;
    gap: var(--e3);
  }
  .filtre {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin-bottom: var(--e3);
  }
  .filtre select {
    width: auto;
    height: 36px;
  }
  .quand {
    white-space: nowrap;
  }
  .details {
    word-break: break-word;
    max-width: 320px;
  }
  .journal-t {
    overflow-x: auto;
  }
  .danger-zone {
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--err-line);
    padding: var(--e4) var(--e5);
    background: var(--white);
  }
  .dz-ligne {
    display: flex;
    align-items: center;
    gap: var(--e5);
  }
  .dz-texte {
    flex: 1;
    min-width: 0;
  }
  .dz-titre {
    font-weight: 500;
    color: var(--text);
  }
  .dz-texte p {
    margin: var(--e1) 0 0;
    font-size: 15px;
    color: var(--text2);
    max-width: 64ch;
  }
  .dz-ligne .btn {
    flex: none;
  }
  .danger-zone :global(.bandeau) {
    margin: var(--e4) 0 0;
  }
  .dz-mot {
    display: block;
    margin: var(--e4) 0 var(--e2);
    color: var(--text);
  }
  #dz-mot {
    width: 100%;
  }
  @media (max-width: 760px) {
    .dz-ligne {
      flex-direction: column;
      gap: var(--e3);
    }
  }
</style>
