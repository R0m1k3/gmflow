<script lang="ts">
  // Mise en route : les 5 étapes de GET /api/admin/mise-en-route, la première à faire ouverte.
  import { RUBRIQUES, nomCourtSociete } from '#lib/libelles.ts';
  import type { Boite, CleEtape, MiseEnRoute, Societe } from '#lib/types.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import { n } from './outils.ts';

  let {
    etat,
    boites,
    societes,
    agir,
    aller
  }: {
    etat: MiseEnRoute | null;
    boites: Boite[];
    societes: Societe[];
    agir: (cle: CleEtape) => void;
    aller: (id: string) => void;
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

<section class="mer" id="mise-en-route">
  <h2>Mise en route</h2>
  {#if etat}
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
  {:else}
    <p class="lead">État de la mise en route indisponible : les sections ci-dessous restent utilisables.</p>
  {/if}
</section>

<style>
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
    background: var(--black);
  }
  .prog i.c {
    background: var(--accent);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--line);
  }
  .step {
    border-bottom: 1px solid var(--line);
  }
  .step-h {
    display: grid;
    grid-template-columns: 32px 1fr auto;
    align-items: center;
    gap: var(--e3);
    min-height: 52px;
    padding: 6px 0;
    width: 100%;
  }
  button.step-h {
    border: 0;
    background: none;
    text-align: left;
    font: inherit;
    color: inherit;
  }
  button.step-h:hover h3 {
    color: var(--accent-ink);
  }
  .step-h h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 500;
  }
  .step-h p {
    margin: 0;
    color: var(--text2);
    font-size: 12px;
    line-height: 18px;
  }
  .step-h .st {
    font-size: 12px;
    color: var(--muted);
  }
  .step.open .step-h .st {
    color: var(--accent-ink);
  }
  .step-b {
    padding: 0 0 var(--e6) 44px;
  }
  .boxes {
    margin: 0 0 var(--e4);
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
  }
  .boxes a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px var(--e3);
    color: inherit;
    text-decoration: none;
  }
  .boxes a + a {
    border-top: 1px solid var(--line);
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
    color: var(--warn);
  }
  .manques li :global(svg.i) {
    margin-top: 2px;
  }
  @media (max-width: 760px) {
    .step-b {
      padding-left: 0;
    }
  }
</style>
