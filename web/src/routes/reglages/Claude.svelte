<script lang="ts">
  // Connexion à Claude : état, puis « Se connecter avec le compte Claude » en étapes
  // (séance `claude setup-token` sur le serveur), autres moyens repliés (jeton collé, clé API).
  import { api, ErreurApi, message, sonder } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES, type Ton } from '#lib/libelles.ts';
  import type { EtatClaude, Seance } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Repli from '#lib/ui/Repli.svelte';

  let { onchange }: { onchange: () => Promise<void> } = $props();

  let etat = $state<EtatClaude | null>(null);
  let erreur = $state('');
  let info = $state('');

  // ---------- État et essai ----------
  let arretEtat: (() => void) | null = null;
  async function chargerEtat(): Promise<boolean> {
    const avant = etat?.etat;
    etat = await api.get<EtatClaude>('/api/admin/claude');
    if (avant === 'en_test' && etat.etat !== 'en_test') void onchange();
    return etat.etat === 'en_test';
  }
  function suivreEtat() {
    arretEtat?.();
    arretEtat = sonder(chargerEtat);
  }
  $effect(() => {
    chargerEtat()
      .then((enTest) => {
        if (enTest) suivreEtat();
      })
      .catch((e) => (erreur = message(e)));
    return () => arretEtat?.();
  });

  async function agir(f: () => Promise<unknown>, ok = '') {
    erreur = '';
    info = '';
    try {
      await f();
      if (ok) info = ok;
    } catch (e) {
      erreur = message(e);
    }
  }
  const essayer = () =>
    agir(async () => {
      await api.post('/api/admin/claude/test');
      if (etat) etat = { ...etat, etat: 'en_test' };
      suivreEtat();
    });

  let resume = $derived.by((): { titre: string; ton: Ton; texte: string } => {
    const mode = etat?.acces?.mode;
    switch (etat?.etat) {
      case 'valide':
        return {
          titre: 'Connecté',
          ton: 'ok',
          texte: mode === 'cle_api' ? `avec une clé API${etat.acces?.cle_api_fin ? ` (…${etat.acces.cle_api_fin})` : ''}` : 'avec le compte Claude'
        };
      case 'en_test':
        return { titre: 'Essai de la connexion en cours', ton: 'info', texte: '' };
      case 'erreur':
        return { titre: 'Connexion refusée', ton: 'erreur', texte: etat.message ?? '' };
      default:
        return {
          titre: 'Pas connecté',
          ton: 'alerte',
          texte: 'sans connexion, les factures ne sont pas lues et aucune réponse n’est proposée'
        };
    }
  });

  // ---------- Se connecter avec le compte Claude (séance) ----------
  type SeanceSuivie = Seance & { id: string };
  const FINALES: Seance['etat'][] = ['terminee', 'erreur', 'expiree'];
  let seance = $state<SeanceSuivie | null>(null);
  let reprise = $state(false);
  let code = $state('');
  let codeEnvoye = $state(false);
  let lancement = $state(false);
  let parcours = $state(false);
  let enCoursSeance = $derived(!!seance && !FINALES.includes(seance.etat));
  let arretSeance: (() => void) | null = null;

  function suivreSeance() {
    arretSeance?.();
    arretSeance = sonder(async () => {
      if (!seance) return false;
      const s = await api.get<Seance>(`/api/admin/claude/seances/${seance.id}`);
      seance = { ...s, id: seance.id };
      if (FINALES.includes(s.etat)) {
        await chargerEtat();
        if (s.etat === 'terminee') suivreEtat();
        return false;
      }
      return true;
    });
  }
  // Connexion ouverte ailleurs (page fermée ou rechargée) : on la reprend, lien et code compris.
  async function reprendre(): Promise<boolean> {
    const s = await api.get<SeanceSuivie | null>('/api/admin/claude/seances/en-cours');
    if (!s) return false;
    seance = s;
    reprise = true;
    parcours = true;
    codeEnvoye = s.etat === 'code_recu';
    suivreSeance();
    return true;
  }
  $effect(() => {
    reprendre().catch(() => {});
    return () => arretSeance?.();
  });

  async function lancer() {
    erreur = '';
    info = '';
    codeEnvoye = false;
    code = '';
    reprise = false;
    lancement = true;
    try {
      const { id } = await api.post<{ id: string }>('/api/admin/claude/seances');
      seance = { id, etat: 'demarrage', url: null, message: null };
      suivreSeance();
    } catch (e) {
      // Une connexion est déjà ouverte : on la reprend au lieu de bloquer.
      if (!(e instanceof ErreurApi && e.code === 409 && (await reprendre().catch(() => false)))) erreur = message(e);
    } finally {
      lancement = false;
    }
  }
  async function envoyerCode(e: SubmitEvent) {
    e.preventDefault();
    if (!seance) return;
    const id = seance.id;
    await agir(async () => {
      await api.post(`/api/admin/claude/seances/${id}/code`, { code: code.trim() });
      codeEnvoye = true;
    });
  }
  async function annuler() {
    if (!seance) return;
    const id = seance.id;
    await agir(async () => {
      await api.post(`/api/admin/claude/seances/${id}/annuler`);
      arretSeance?.();
      const s = await api.get<Seance>(`/api/admin/claude/seances/${id}`);
      seance = { ...s, id };
    });
  }

  /** Ouvre le parcours de connexion (bouton de la mise en route). */
  export function connecter() {
    parcours = true;
  }
  let montrerParcours = $derived(parcours || enCoursSeance || (etat !== null && etat.etat !== 'valide' && etat.etat !== 'en_test'));
  // Étape courante du parcours : 1 lien, 2 connexion chez Claude, 3 code.
  let etape = $derived(!enCoursSeance || !seance?.url ? 1 : codeEnvoye || seance.etat === 'code_recu' ? 4 : 3);

  // ---------- Autres moyens : jeton collé, clé API ----------
  let jeton = $state('');
  let cle = $state('');
  const collerJeton = (e: SubmitEvent) => {
    e.preventDefault();
    return agir(async () => {
      await api.put('/api/admin/claude/jeton', { jeton: jeton.trim() });
      jeton = '';
      await chargerEtat();
      suivreEtat();
    }, 'Jeton enregistré : essai de la connexion en cours.');
  };
  const enregistrerCle = (e: SubmitEvent) => {
    e.preventDefault();
    return agir(async () => {
      await api.put('/api/admin/claude/cle-api', { cle: cle.trim() });
      cle = '';
      await chargerEtat();
      if (etat?.etat === 'en_test') suivreEtat();
      info = etat?.acces?.mode === 'cle_api' ? 'Clé enregistrée : essai en cours.' : 'Clé enregistrée. Choisissez « Clé API » pour l’utiliser.';
    });
  };
  const choisirAcces = (mode: 'jeton' | 'cle_api') =>
    agir(async () => {
      try {
        await api.put('/api/admin/claude/acces', { mode });
      } finally {
        await chargerEtat();
      }
      suivreEtat();
    });

  // ---------- Confirmations ----------
  let dlg = $state<{ titre: string; libelle: string; texte: string; faire: () => Promise<void> } | null>(null);
  let dlgOuvert = $state(false);
  function demander(titre: string, libelle: string, texte: string, faire: () => Promise<void>) {
    dlg = { titre, libelle, texte, faire };
    dlgOuvert = true;
  }
  const deconnecter = () =>
    demander('Se déconnecter de Claude ?', 'Se déconnecter', 'Les factures ne seront plus lues et aucune réponse ne sera proposée jusqu’à la prochaine connexion.', () =>
      agir(async () => {
        await api.del('/api/admin/claude/jeton');
        await chargerEtat();
        await onchange();
      })
    );
  const supprimerCle = () =>
    demander('Supprimer la clé API ?', 'Supprimer la clé', 'GMFlow reviendra à la connexion par le compte Claude.', () =>
      agir(async () => {
        await api.del('/api/admin/claude/cle-api');
        await chargerEtat();
        await onchange();
      })
    );
</script>

<section class="sujet" id="claude">
  <div class="sujet-h"><h2>{RUBRIQUES.claude}</h2></div>
  <p class="lead">Claude lit les factures, trie les mails et propose les réponses. Il faut un abonnement Claude, ou une clé API.</p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  {#if !etat && !erreur}
    <Chargement compact />
  {:else if etat}
    <div class="filets">
      <div class="ligne bloc fixe">
        <div class="a">
          <Etiquette ton={resume.ton} point>{resume.titre}</Etiquette>
          {#if resume.texte}<span class="t2 texte-etat">{resume.texte}</span>{/if}
        </div>
        <div class="b">
          {#if etat.teste_le}Essayée le <span class="num">{quand(etat.teste_le)}</span>{/if}
          {#if etat.utilise_le}{' '}· dernière utilisation le <span class="num">{quand(etat.utilise_le)}</span>{/if}
        </div>
        <div class="r actions-etat">
          {#if etat.etat !== 'absent'}
            <button type="button" class="btn sm" class:en-cours={etat.etat === 'en_test'} disabled={etat.etat === 'en_test'} onclick={essayer}>
              Essayer la connexion
            </button>
          {/if}
          {#if etat.acces?.mode !== 'cle_api' && (etat.acces?.jeton ?? etat.etat !== 'absent')}
            <button type="button" class="btn sm fantome" onclick={deconnecter}>Se déconnecter</button>
          {/if}
        </div>
      </div>
    </div>

    {#if montrerParcours}
      <div class="parcours">
        <h3 class="sous-titre">Se connecter avec le compte Claude</h3>
        {#if reprise && enCoursSeance}
          <Bandeau ton="info">Une connexion était déjà commencée : elle reprend ici.</Bandeau>
        {/if}
        {#if seance && !enCoursSeance}
          {#if seance.message === 'annulée'}
            <Bandeau ton="neutre">Connexion annulée.</Bandeau>
          {:else if seance.etat === 'terminee'}
            <Bandeau ton="ok" titre="Compte Claude connecté.">Essai de la connexion en cours.</Bandeau>
          {:else if seance.etat === 'expiree'}
            <Bandeau titre="Aucun code reçu à temps.">Recommencez depuis l’étape 1.</Bandeau>
          {:else}
            <Bandeau ton="erreur" titre="La connexion a échoué.">{seance.message ?? ''}</Bandeau>
          {/if}
        {/if}
        <ol class="etapes">
          <li class:cur={etape === 1} class:fait={etape > 1}>
            <span class="n">{#if etape > 1}<Icone nom="coche" />{:else}1{/if}</span>
            <div>
              <div class="t">Obtenir le lien de connexion</div>
              {#if enCoursSeance && seance?.url}
                <a class="btn" href={seance.url} target="_blank" rel="noopener noreferrer">Ouvrir la page de connexion Claude<Icone nom="ouvrir" /></a>
              {:else if enCoursSeance}
                <Chargement compact texte="Préparation du lien" />
              {:else}
                <button type="button" class="btn principal" class:en-cours={lancement} disabled={lancement} onclick={lancer}>
                  {seance ? 'Recommencer' : 'Obtenir le lien'}
                </button>
              {/if}
            </div>
          </li>
          <li class:cur={etape === 3} class:fait={etape > 3}>
            <span class="n">{#if etape > 3}<Icone nom="coche" />{:else}2{/if}</span>
            <div>
              <div class="t">Se connecter chez Claude</div>
              <div class="petit t2">Avec le compte Claude de la société. Claude affiche ensuite un code : copiez-le.</div>
            </div>
          </li>
          <li class:cur={etape === 3} class:fait={etape > 3}>
            <span class="n">{#if etape > 3}<Icone nom="coche" />{:else}3{/if}</span>
            <div>
              <div class="t">Coller le code ici</div>
              {#if etape === 4}
                <Chargement compact texte="Code reçu, connexion en cours" />
              {:else}
                <form class="code" onsubmit={envoyerCode}>
                  <label class="sr" for="code-claude">Code de connexion</label>
                  <input id="code-claude" bind:value={code} placeholder="Code donné par Claude" autocomplete="off" required disabled={etape !== 3} />
                  <button class="btn" disabled={etape !== 3 || !code.trim()}>Valider le code</button>
                </form>
              {/if}
            </div>
          </li>
        </ol>
        {#if enCoursSeance}
          <div class="form-f"><button type="button" class="btn sm fantome" onclick={annuler}>Annuler la connexion</button></div>
        {/if}
      </div>
    {:else}
      <div class="form-f">
        <button type="button" class="btn sm fantome" onclick={() => (parcours = true)}>Se reconnecter avec un autre compte Claude</button>
      </div>
    {/if}

    <div class="replis">
      <Repli titre="Autres moyens de connexion" resume="jeton collé, clé API">
        <h4>Coller un jeton de connexion</h4>
        <p class="petit t2">Obtenu sur un autre ordinateur avec la commande <code>claude setup-token</code>. Il n’est plus jamais affiché ensuite.</p>
        <form class="code" onsubmit={collerJeton}>
          <label class="sr" for="jeton-claude">Jeton de connexion</label>
          <input id="jeton-claude" type="password" bind:value={jeton} placeholder="sk-ant-oat01-…" autocomplete="off" required />
          <button class="btn" disabled={!jeton.trim()}>Enregistrer et essayer</button>
        </form>
        {#if etat.acces?.mode === 'cle_api' && etat.acces.jeton}
          <p class="petit"><button type="button" class="btn-lien" onclick={deconnecter}>Supprimer le jeton enregistré</button></p>
        {/if}

        <h4>Clé API Anthropic</h4>
        <p class="petit t2">
          Une clé de la console Anthropic, facturée à l’usage. GMFlow utilise l’une ou l’autre, jamais les deux.
        </p>
        {#if etat.acces}
          <div class="choix" role="radiogroup" aria-label="Moyen de connexion utilisé">
            <label class="case">
              <input type="radio" name="acces" checked={etat.acces.mode === 'jeton'} onchange={() => choisirAcces('jeton')} />
              Compte Claude
            </label>
            <label class="case">
              <input type="radio" name="acces" checked={etat.acces.mode === 'cle_api'} disabled={!etat.acces.cle_api} onchange={() => choisirAcces('cle_api')} />
              Clé API{#if etat.acces.cle_api}&nbsp;<span class="num">…{etat.acces.cle_api_fin ?? ''}</span>{:else}<span class="muted">&nbsp;(aucune)</span>{/if}
            </label>
            {#if etat.acces.cle_api}<button type="button" class="btn sm fantome" onclick={supprimerCle}>Supprimer la clé</button>{/if}
          </div>
        {/if}
        <form class="code" onsubmit={enregistrerCle}>
          <label class="sr" for="cle-claude">Clé API</label>
          <input id="cle-claude" type="password" bind:value={cle} placeholder="sk-ant-api03-…" autocomplete="off" required />
          <button class="btn" disabled={!cle.trim()}>{etat.acces?.cle_api ? 'Remplacer la clé' : 'Enregistrer la clé'}</button>
        </form>
      </Repli>
    </div>
  {/if}
</section>

{#if dlg}
  <Confirmation bind:ouvert={dlgOuvert} titre={dlg.titre} libelle={dlg.libelle} danger onconfirmer={dlg.faire}>{dlg.texte}</Confirmation>
{/if}

<style>
  .texte-etat {
    font-weight: 400;
    margin-left: var(--e2);
  }
  .ligne .a {
    white-space: normal;
  }
  .actions-etat {
    display: flex;
    gap: var(--e2);
    align-items: center;
  }
  .parcours {
    margin-top: var(--e2);
  }
  .parcours > :global(.bandeau) {
    margin-bottom: var(--e3);
  }
  .etapes {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .etapes li {
    display: grid;
    grid-template-columns: 32px 1fr;
    gap: var(--e3);
    padding: var(--e2) 0 var(--e3);
  }
  .etapes .n {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font: 500 11px/1 var(--mono);
    border: 1px solid var(--line2);
    color: var(--text2);
  }
  .etapes .n :global(svg.i) {
    width: 12px;
    height: 12px;
    stroke-width: 2.5;
  }
  .etapes li.fait .n {
    background: var(--black);
    border-color: var(--black);
    color: #fff;
  }
  .etapes li.cur .n {
    border-color: var(--accent);
    color: var(--accent);
    border-width: 1.5px;
  }
  .etapes .t {
    font-weight: 500;
    line-height: 24px;
    margin-bottom: var(--e1);
  }
  .etapes li:not(.cur):not(.fait) .t {
    color: var(--text2);
  }
  .code {
    display: flex;
    gap: var(--e2);
    max-width: 520px;
    margin-bottom: var(--e3);
  }
  .code input {
    flex: 1;
    min-width: 0;
  }
  h4 {
    margin: var(--e2) 0 var(--e1);
    font-size: 13px;
    font-weight: 500;
  }
  h4 + p {
    margin-bottom: var(--e2);
  }
  .choix {
    display: flex;
    align-items: center;
    gap: var(--e5);
    margin-bottom: var(--e3);
    flex-wrap: wrap;
  }
  .choix .case {
    align-items: center;
  }
  .choix .case input {
    margin-top: 0;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
