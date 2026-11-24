<script lang="ts">
  // Mise en route : bandeau d'une phrase (étapes faites, ce qui manque, « Continuer »), puis le
  // détail des 5 étapes de GET /api/admin/mise-en-route, replié, la première à faire ouverte.
  import { RUBRIQUES, nomCourtSociete } from '#lib/libelles.ts';
  import type { Boite, CleEtape, MiseEnRoute, Societe } from '#lib/types.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import { n } from './outils.ts';

  let {
    etat,
    boites,
    societes,
    agir,
    aller,
    ouvert = $bindable(false)
  }: {
    etat: MiseEnRoute | null;
    boites: Boite[];
    societes: Societe[];
    agir: (cle: CleEtape) => void;
    aller: (id: string) => void;
    ouvert?: boolean;
  } = $props();

  const ETAPES: Record<CleEtape, { titre: string; but: string; action: string }> = {
    societes: { titre: RUBRIQUES.societes, but: 'Les sociétés qui reçoivent les factures', action: 'Ajouter une société' },
    boites: {
      titre: RUBRIQUES.boites,
      but: 'Les adresses où arrivent les factures et les mails des fournisseurs',
      action: 'Ajouter une boîte mail'
    },
    claude: { titre: RUBRIQUES.claude, but: 'Pour lire les factures et proposer les réponses', action: 'Se connecter avec le compte Claude' },
    envoi: { titre: RUBRIQUES.envoi, but: 'L’adresse de collecte de chaque société', action: 'Régler l’envoi' },
    utilisateurs: {
      titre: RUBRIQUES.utilisateurs,
      but: 'Les responsables de magasin et ce qu’ils voient',
      action: 'Ajouter un utilisateur'
    }
  };

  let premiere = $derived(etat?.etapes.find((e) => !e.fait)?.cle ?? null);
  let ouverte = $state<CleEtape | null>(null);
  let choisie = $state(false);
  // Tant que l'utilisateur n'a rien ouvert lui-même, la première étape à faire reste ouverte.
  $effect(() => {
    if (!choisie) ouverte = premiere;
  });
  function basculer(cle: CleEtape) {
    choisie = true;
    ouverte = ouverte === cle ? null : cle;
  }

  let faites = $derived(etat ? etat.etapes.filter((e) => e.fait).length : 0);
  let suivante = $derived(etat?.etapes.find((e) => !e.fait) ?? null);
  // « Il manque … » : le premier manque de la première étape à faire, sinon son but.
  let manque = $derived.by(() => {
    if (!suivante) return '';
    const m = suivante.manques[0] ?? suivante.resume ?? ETAPES[suivante.cle].but;
    return m ? `${m.replace(/[.\s]+$/, '')}.` : '';
  });

  let restantes = $derived(etat ? etat.etapes.filter((e) => !e.fait).length : 0);
  let lead = $derived.by(() => {
    if (!etat) return '';
    if (etat.terminee) return 'Tout est en place : les factures arrivent, sont lues et partent vers Dext / Yooz.';
    const etapes = restantes > 1 ? `${n(restantes, 'étape')} manquent` : 'Une étape manque';
    return `${etapes} encore pour que les factures arrivent et partent seules vers Dext / Yooz.`;
  });

  const nomSociete = (id: number) => nomCourtSociete(societes.find((s) => s.id === id)?.raison_sociale);
  function etatBoite(b: Boite): { ok: boolean; texte: string } {
    if (b.pause_auto) return { ok: false, texte: 'arrêtée : connexion refusée' };
    if (!b.active) return { ok: false, texte: b.dernier_test?.ok ? 'en pause' : 'pas encore démarrée' };
    if (b.erreur_depuis) return { ok: false, texte: 'en erreur' };
    return { ok: true, texte: 'relève toutes les 5 min' };
  }
</script>

<section class="mer" id="mise-en-route" aria-label="Mise en route">
  {#if etat}
    <div class="bandeau-mer" class:fini={etat.terminee}>
      <Icone nom="drapeau" />
      <span class="phrase">
        {#if etat.terminee}
          <b>Mise en route terminée.</b> Les factures arrivent, sont lues et partent vers Dext / Yooz.
        {:else}
          <b>Mise en route : {faites} {faites > 1 ? 'étapes' : 'étape'} sur {etat.etapes.length}.</b> {manque}
        {/if}
      </span>
      <span class="actions-mer">
        <button type="button" class="lien-mer" aria-expanded={ouvert} aria-controls="etapes-mer" onclick={() => (ouvert = !ouvert)}>
          {ouvert ? 'Masquer les étapes' : 'Voir les étapes'}
        </button>
        {#if suivante}
          <button type="button" class="lien-mer fort" onclick={() => suivante && agir(suivante.cle)}>Continuer</button>
        {/if}
      </span>
    </div>
    {#if ouvert}
      <div class="detail-mer" id="etapes-mer">
        <p class="lead">{lead}</p>
    <div class="prog" aria-hidden="true">
      {#each etat.etapes as e (e.cle)}<i class:d={e.fait} class:c={!e.fait && e.cle === premiere}></i>{/each}
    </div>
    <ol class="steps">
      {#each etat.etapes as e, i (e.cle)}
        {@const t = ETAPES[e.cle]}
        <li class="step" class:done={e.fait} class:open={ouverte === e.cle && !e.fait}>
          {#if e.fait}
            <div class="step-h">
              <span class="rond-etape fait" aria-label="Fait"><Icone nom="coche" /></span>
              <div><h3>{t.titre}</h3><p>{e.resume}</p></div>
              <button type="button" class="btn sm fantome" onclick={() => aller(e.cle)}>Modifier</button>
            </div>
          {:else}
            <button type="button" class="step-h" aria-expanded={ouverte === e.cle} onclick={() => basculer(e.cle)}>
              <span class="rond-etape" class:cur={ouverte === e.cle}>{i + 1}</span>
              <div><h3>{t.titre}</h3><p>{e.resume || t.but}</p></div>
              <span class="st">{ouverte === e.cle ? 'En cours' : 'À faire'}</span>
            </button>
            {#if ouverte === e.cle}
              <div class="step-b">
                {#if e.cle === 'boites' && boites.length > 0}
                  <div class="boxes">
                    {#each boites as b (b.id)}
                      {@const eb = etatBoite(b)}
                      <a href="/reglages/boites/{b.id}">
                        <span class={eb.ok ? 'ok' : 'att'}><Icone nom={eb.ok ? 'coche' : 'alerte'} /></span>
                        {b.identifiant}
                        <span class="muted">{nomSociete(b.societe_id)}, {eb.texte}</span>
                      </a>
                    {/each}
                  </div>
                {/if}
                {#if e.manques.length}
                  <ul class="manques">
                    {#each e.manques as m (m)}<li><Icone nom="alerte" />{m}</li>{/each}
                  </ul>
                {/if}
                <div class="form-f">
                  <button type="button" class="btn principal" onclick={() => agir(e.cle)}>{t.action}</button>
                  {#if e.cle !== 'boites'}
                    <button type="button" class="btn fantome" onclick={() => aller(e.cle)}>Voir la section</button>
                  {/if}
                </div>
              </div>
            {/if}
          {/if}
        </li>
      {/each}
    </ol>
        </div>
    {/if}
  {:else}
    <p class="lead">État de la mise en route indisponible : les sections ci-dessous restent utilisables.</p>
  {/if}
</section>

<style>
  .mer {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    scroll-margin-top: var(--e6);
  }
  .mer > .lead {
    margin: 0;
  }
  /* Bandeau ambre de la maquette : drapeau, une phrase, « Continuer ». */
  .bandeau-mer {
    display: flex;
    align-items: center;
    gap: var(--e4);
    padding: var(--e4) var(--e5);
    border-radius: var(--r3);
    background: var(--warn-bg);
    color: var(--warn-ink);
    flex-wrap: wrap;
  }
  .bandeau-mer.fini {
    background: var(--ok-bg);
    color: var(--ok-ink);
  }
  .bandeau-mer > :global(svg.i) {
    flex: none;
  }
  .bandeau-mer .phrase {
    flex: 1 1 320px;
    min-width: 0;
  }
  .bandeau-mer b {
    font-weight: 500;
  }
  .actions-mer {
    display: flex;
    align-items: center;
    gap: var(--e5);
    margin-left: auto;
  }
  .lien-mer {
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    font-size: 15px;
    color: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  .lien-mer.fort {
    font-size: 16px;
    font-weight: 500;
  }
  .lien-mer:hover {
    text-decoration: underline;
  }
  .detail-mer {
    padding: var(--e5);
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
  }
  .detail-mer .lead {
    margin: 0 0 var(--e4);
    color: var(--text2);
  }
  .prog {
    display: flex;
    gap: var(--e1);
    margin-bottom: var(--e4);
  }
  .prog i {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--line);
  }
  .prog i.d {
    background: var(--accent);
  }
  .prog i.c {
    background: var(--warn-line);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .step + .step {
    border-top: 1px solid var(--line-faint);
  }
  .step-h {
    display: grid;
    grid-template-columns: 32px 1fr auto;
    align-items: center;
    gap: var(--e3);
    min-height: 56px;
    padding: var(--e2) 0;
    width: 100%;
  }
  button.step-h {
    border: 0;
    background: none;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  button.step-h:hover h3 {
    color: var(--accent-ink);
  }
  .step-h h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 500;
  }
  .step-h p {
    margin: 0;
    color: var(--text2);
    font-size: 14px;
    line-height: 1.45;
  }
  .step-h .st {
    font-size: 14px;
    color: var(--muted);
  }
  .step.open .step-h .st {
    color: var(--accent-ink);
  }
  .step-b {
    padding: 0 0 var(--e5) 44px;
  }
  .boxes {
    margin: 0 0 var(--e4);
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
    overflow: hidden;
  }
  .boxes a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--e3) var(--e4);
    color: inherit;
    text-decoration: none;
    flex-wrap: wrap;
  }
  .boxes a + a {
    border-top: 1px solid var(--line-faint);
  }
  .boxes a:hover {
    background: var(--pane);
  }
  .boxes .ok {
    color: var(--ok);
    display: flex;
  }
  .boxes .att {
    color: var(--warn);
    display: flex;
  }
  .manques {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .manques li {
    display: flex;
    align-items: flex-start;
    gap: var(--e2);
    padding: 3px 0;
    font-size: 15px;
    color: var(--warn);
  }
  .manques li :global(svg.i) {
    margin-top: 3px;
    flex: none;
  }
  @media (max-width: 760px) {
    .step-b {
      padding-left: 0;
    }
    .detail-mer {
      padding: var(--e4);
    }
    .bandeau-mer {
      padding: var(--e4);
    }
  }
</style>
