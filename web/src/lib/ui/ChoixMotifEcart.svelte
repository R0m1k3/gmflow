<script lang="ts">
  // Motif d'« Écarter » (audit U23), à mettre dans une <Confirmation desactive={!motif}> :
  //   <ChoixMotifEcart bind:motif bind:texte />
  // Motifs fréquents (MOTIFS_ECARTEMENT) et précision facultative ; enregistrés sur la pièce et
  // dans le journal.
  import { MOTIFS_ECARTEMENT } from '#lib/libelles.ts';
  import type { MotifEcartement } from '#lib/types.ts';

  let { motif = $bindable(null), texte = $bindable('') }: { motif?: MotifEcartement | null; texte?: string } = $props();
  const uid = $props.id();
</script>

<fieldset class="motif-ecart">
  <legend>Pourquoi ?</legend>
  {#each MOTIFS_ECARTEMENT as m (m.valeur)}
    <label class="case"><input type="radio" name="{uid}-motif" value={m.valeur} bind:group={motif} /> {m.libelle}</label>
  {/each}
  <label class="champ"><span class="libelle">Précision</span><input type="text" maxlength="500" bind:value={texte} placeholder="Facultatif" /></label>
</fieldset>

<style>
  .motif-ecart {
    border: 0;
    margin: var(--e4) 0 0;
    padding: 0;
    display: grid;
    gap: var(--e2);
  }
  .motif-ecart legend {
    font-weight: 500;
    color: var(--text);
    margin-bottom: var(--e2);
    padding: 0;
  }
  .motif-ecart .champ {
    margin-top: var(--e2);
  }
  @media (max-width: 760px) {
    .motif-ecart .case {
      min-height: 44px;
    }
  }
</style>
