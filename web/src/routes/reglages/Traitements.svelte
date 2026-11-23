<script lang="ts">
  // Traitements : chaque étape du courrier (relève, lecture, tri, rapprochement, dépôt), ce
  // qui attend et pourquoi, avec un bouton pour la relancer. Se met à jour toute seule.
  import { api, message, sonder } from '#lib/api.ts';
  import { RUBRIQUES } from '#lib/libelles.ts';
  import type { CleEtapeTraitement, EtapeTraitement, Traitements as Vue } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import { quandCourt } from '#lib/ui/DocOutils.ts';
  import Etiquette from '#lib/ui/Etiquette.svelte';

  let vue = $state<Vue | null>(null);
  let erreur = $state('');
  let info = $state('');
  let occupe = $state<CleEtapeTraitement | 'tout' | null>(null);
  let dlgTout = $state(false);

  async function charger() {
    vue = await api.get<Vue>('/api/admin/traitements');
    erreur = '';
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    // Toutes les 10 secondes, tant que la page est ouverte.
    return sonder(async () => {
      await charger();
    }, 10_000);
  });

  const ETATS: Record<EtapeTraitement['etat'], { texte: string; ton: 'ok' | 'alerte' | 'erreur' }> = {
    ok: { texte: 'Normal', ton: 'ok' },
    en_retard: { texte: 'En retard', ton: 'alerte' },
    bloque: { texte: 'Bloqué', ton: 'erreur' },
    arrete: { texte: 'À l’arrêt', ton: 'erreur' }
  };

  const AUTRES: Record<string, string> = { claude: 'la connexion à Claude', redaction: 'la rédaction des réponses' };

  function vu(iso: string | null): string {
    return iso ? quandCourt(iso) : 'jamais';
  }

  async function relancer(e: EtapeTraitement) {
    occupe = e.cle;
    info = '';
    try {
      await api.post(`/api/admin/traitements/${e.cle}/relancer`);
      info = `${e.nom} : relance demandée, elle part dans quelques secondes.`;
      await charger();
    } catch (err) {
      erreur = message(err);
    } finally {
      occupe = null;
    }
  }

  async function toutRelire() {
    occupe = 'tout';
    info = '';
    try {
      const r = await api.post<{ documents: number }>('/api/admin/traitements/tout-relire');
      info =
        r.documents === 0
          ? 'Aucun document à relire : ils sont déjà tous dans la file de lecture.'
          : `${r.documents} document${r.documents > 1 ? 's' : ''} remis en lecture, après les mails récents.`;
      await charger();
    } catch (err) {
      erreur = message(err);
    } finally {
      occupe = null;
    }
  }
</script>

<section class="sujet" id="traitements">
  <div class="sujet-h"><h2>{RUBRIQUES.traitements}</h2></div>
  <p class="lead">
    Ce que GMFlow fait du courrier, étape par étape : ce qui attend, depuis quand et pourquoi. La page se met à jour toute
    seule.
  </p>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  {#if !vue}
    {#if !erreur}<Chargement compact />{/if}
  {:else}
    {#if !vue.service.en_marche}
      <Bandeau ton="erreur" titre="Le service de traitement ne répond plus.">
        Dernier signe de vie : {vu(vue.service.vu_le)}. Il redémarre tout seul ; si cela dure, redémarrez le conteneur
        « worker » de GMFlow.
      </Bandeau>
    {:else if vue.service.autres_arretees.length}
      <Bandeau ton="attention" titre="Une partie du service est à l’arrêt.">
        En panne : {vue.service.autres_arretees.map((f) => AUTRES[f] ?? f).join(', ')}. Elle redémarre toute seule.
      </Bandeau>
    {/if}
    {#if vue.claude}
      <Bandeau ton="attention" titre="Claude ne lit pas en ce moment.">{vue.claude}.</Bandeau>
    {/if}

    <div class="filets">
      {#each vue.etapes as e (e.cle)}
        {@const s = ETATS[e.etat]}
        <div class="ligne bloc fixe etape">
          <div class="a">
            {e.nom}
            <Etiquette ton={s.ton} point>{s.texte}</Etiquette>
          </div>
          <div class="b">
            <span class="resume">{e.resume}.</span>
            {#if e.blocage}
              <span class="detail ko">Ne peut pas avancer : {e.blocage}.</span>
            {/if}
            {#if e.plus_vieille && e.en_attente > 0}
              <span class="detail">
                Le plus ancien attend depuis {quandCourt(e.plus_vieille.depuis)}{e.plus_vieille.raison === e.blocage
                  ? '.'
                  : ` : ${e.plus_vieille.raison}.`}
              </span>
            {/if}
            {#if !e.file.vivante}
              <span class="detail ko">
                {e.file.battement
                  ? `Plus de signe de vie depuis ${quandCourt(e.file.battement)} : redémarrage automatique en cours.`
                  : 'Pas encore démarrée.'}
              </span>
            {/if}
            <span class="detail">Dernier passage : {vu(e.dernier_passage)}.</span>
          </div>
          <div class="r">
            {#if e.relance_demandee}
              <span class="attente">Relance demandée…</span>
            {:else}
              <button
                type="button"
                class="btn sm"
                class:en-cours={occupe === e.cle}
                disabled={!!occupe}
                onclick={() => relancer(e)}>Relancer</button
              >
            {/if}
          </div>
        </div>
      {/each}
    </div>

    <h3 class="sous-titre">Relire tous les documents en attente</h3>
    <p class="aide">
      Remet en lecture les documents en attente ou illisibles qui ne sont pas déjà dans la file. Les mails récents
      passent avant eux, et la limite du jour compte.
    </p>
    <button type="button" class="btn" class:en-cours={occupe === 'tout'} disabled={!!occupe} onclick={() => (dlgTout = true)}
      >Tout relire</button
    >
  {/if}
</section>

<Confirmation bind:ouvert={dlgTout} titre="Relire tous les documents en attente ?" libelle="Tout relire" onconfirmer={toutRelire}>
  Chaque document relu compte dans la limite du jour de Claude.
</Confirmation>

<style>
  .etape .a {
    display: flex;
    align-items: center;
    gap: var(--e2);
    flex-wrap: wrap;
  }
  .etape .b {
    white-space: normal;
    color: var(--text);
    font-size: 14px;
    line-height: 21px;
    padding-top: 2px;
  }
  .resume,
  .detail {
    display: block;
  }
  .detail {
    color: var(--text2);
    font-size: 13px;
  }
  .detail.ko {
    color: var(--err-ink);
    font-weight: 500;
  }
  .attente {
    color: var(--text2);
    font-size: 13px;
  }
  .aide {
    color: var(--text2);
    margin: 0 0 var(--e3);
    max-width: 60ch;
  }
</style>
