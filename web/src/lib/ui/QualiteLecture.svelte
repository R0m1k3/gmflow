<script lang="ts">
  // « Qualité de la lecture » du mois (administrateur, page Bilan) : pièces validées, part corrigée
  // à la main par champ, voie, modèle du dernier essai et fournisseur, coût estimé des appels.
  // Chargé à l'ouverture du repli ; tableaux seulement, nombres alignés à droite.
  import { message, qualiteLecture } from '#lib/api.ts';
  import { libelleChampCorrige, libelleVoie } from '#lib/libelles.ts';
  import type { QualiteLecture } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Chargement from './Chargement.svelte';
  import Repli from './Repli.svelte';

  let { mois, societe = null }: { mois: string; societe?: number | null } = $props();

  let ouvert = $state(false);
  let q = $state<QualiteLecture | null>(null);
  let erreur = $state('');
  let jeton = 0;
  $effect(() => {
    if (!ouvert) return;
    const moi = ++jeton;
    const demande = { mois, societe };
    q = null;
    erreur = '';
    qualiteLecture(demande).then(
      (v) => {
        if (moi === jeton) q = v;
      },
      (e) => {
        if (moi === jeton) erreur = message(e);
      }
    );
  });

  const pct = (n: number, sur: number) =>
    sur ? `${((100 * n) / sur).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} %` : '';
  const dollars = (v: number | null) =>
    v === null
      ? ''
      : `${v.toLocaleString('fr-FR', { minimumFractionDigits: v < 1 ? 3 : 2, maximumFractionDigits: v < 1 ? 3 : 2 })} $`;
  const voie = (v: string | null) => {
    const l = libelleVoie(v as Parameters<typeof libelleVoie>[0]);
    return l ? l.charAt(0).toUpperCase() + l.slice(1) : 'Voie inconnue';
  };
  const essai = (m: string | null, nouvel: boolean) =>
    m === null ? 'Sans appel de lecture mesuré' : nouvel ? `${m}, nouvel essai` : `${m} seul`;
  let resume = $derived(q ? `${q.validees} validées, ${pct(q.corrigees, q.validees) || '0 %'} corrigées` : '');
</script>

<Repli icone="graphique" titre="Qualité de la lecture" {resume} bind:ouvert>
  {#if erreur}
    <Bandeau ton="erreur">{erreur}</Bandeau>
  {:else if !q}
    <Chargement />
  {:else}
    <p class="petit muted">
      Pièces validées par un humain dans le mois choisi. Le coût est une estimation donnée par Claude CLI pour chaque appel, pas le
      montant facturé.
    </p>
    <table class="ql">
      <tbody>
        <tr><td>Pièces validées</td><td class="n"><span class="num">{q.validees}</span></td><td></td></tr>
        <tr>
          <td>Corrigées à la main</td>
          <td class="n"><span class="num">{q.corrigees}</span></td>
          <td class="n t2"><span class="num">{pct(q.corrigees, q.validees)}</span></td>
        </tr>
        <tr>
          <td>Coût estimé de lecture ce mois</td>
          <td class="n"><span class="num">{dollars(q.cout.total_usd)}</span></td>
          <td class="n t2 petit">
            <span class="num">{q.cout.appels}</span> appels{#if q.cout.appels_sans_cout}, dont <span class="num"
                >{q.cout.appels_sans_cout}</span
              > sans coût connu{/if}
          </td>
        </tr>
        <tr>
          <td>Coût moyen par pièce</td>
          <td class="n"><span class="num">{dollars(q.cout.moyen_par_piece_usd)}</span></td>
          <td class="n t2 petit"><span class="num">{q.cout.pieces}</span> pièces lues</td>
        </tr>
      </tbody>
    </table>

    {#if q.validees}
      <h3>Champs corrigés</h3>
      {#if q.par_champ.length}
        <table class="ql">
          <thead><tr><th>Champ</th><th class="n">Pièces corrigées</th><th class="n">Part des validées</th></tr></thead>
          <tbody>
            {#each q.par_champ as c (c.champ)}
              <tr>
                <td>{libelleChampCorrige(c.champ)}</td>
                <td class="n"><span class="num">{c.pieces}</span></td>
                <td class="n"><span class="num">{pct(c.pieces, q.validees)}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="muted">Aucun champ corrigé.</p>
      {/if}

      <h3>Par voie de lecture</h3>
      <table class="ql">
        <thead><tr><th>Voie</th><th class="n">Validées</th><th class="n">Corrigées</th><th class="n">Part corrigée</th></tr></thead>
        <tbody>
          {#each q.par_voie as v (v.cle)}
            <tr>
              <td>{voie(v.cle)}</td>
              <td class="n"><span class="num">{v.validees}</span></td>
              <td class="n"><span class="num">{v.corrigees}</span></td>
              <td class="n"><span class="num">{pct(v.corrigees, v.validees)}</span></td>
            </tr>
          {/each}
        </tbody>
      </table>

      <h3>Par modèle du dernier essai</h3>
      <table class="ql">
        <thead><tr><th>Modèle</th><th class="n">Validées</th><th class="n">Corrigées</th><th class="n">Part corrigée</th></tr></thead>
        <tbody>
          {#each q.par_modele as m (`${m.modele}-${m.nouvel_essai}`)}
            <tr>
              <td>{essai(m.modele, m.nouvel_essai)}</td>
              <td class="n"><span class="num">{m.validees}</span></td>
              <td class="n"><span class="num">{m.corrigees}</span></td>
              <td class="n"><span class="num">{pct(m.corrigees, m.validees)}</span></td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="petit muted">« Nouvel essai » : relu avec le modèle du nouvel essai après une lecture douteuse, ou relu en détail.</p>

      {#if q.fournisseurs.length}
        <h3>Fournisseurs les plus corrigés</h3>
        <table class="ql">
          <thead><tr><th>Fournisseur</th><th class="n">Validées</th><th class="n">Corrigées</th><th class="n">Part corrigée</th></tr></thead>
          <tbody>
            {#each q.fournisseurs as f (f.fournisseur_id)}
              <tr>
                <td><a href="/fournisseurs/{f.fournisseur_id}">{f.nom}</a></td>
                <td class="n"><span class="num">{f.validees}</span></td>
                <td class="n"><span class="num">{f.corrigees}</span></td>
                <td class="n"><span class="num">{pct(f.corrigees, f.validees)}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}
  {/if}
</Repli>

<style>
  .ql {
    margin-bottom: var(--e3);
  }
  .ql .num {
    white-space: nowrap;
  }
</style>
