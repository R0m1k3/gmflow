<script lang="ts">
  // Dialogue « Raccourcis clavier » (touche ?). Même habillage que Confirmation (<dialog> natif, modal).
  //   <AideClavier bind:ouvert />
  // Échap ou « Fermer » ferment ; le focus revient où il était.
  import Kbd from './Kbd.svelte';

  let { ouvert = $bindable(false) }: { ouvert?: boolean } = $props();

  let dlg = $state<HTMLDialogElement>();
  const uid = $props.id();

  const GROUPES: { titre: string; lignes: { touches: string[][]; texte: string }[] }[] = [
    {
      titre: 'Dans une liste',
      lignes: [
        { touches: [['J'], ['ArrowDown']], texte: 'Document ou mail suivant' },
        { touches: [['K'], ['ArrowUp']], texte: 'Document ou mail précédent' },
        { touches: [['Enter']], texte: 'Ouvrir le premier de la liste' },
        { touches: [['Escape']], texte: 'Fermer la fiche' },
        { touches: [['/']], texte: 'Chercher' }
      ]
    },
    {
      titre: 'Dans une fiche document',
      lignes: [
        { touches: [['Enter']], texte: 'Valider (le bouton principal), ou remettre à contrôler une pièce écartée' },
        { touches: [['E']], texte: 'Écarter' },
        { touches: [['Ctrl', 'Z']], texte: 'Annuler la dernière validation, tant que l’envoi n’est pas parti' },
        { touches: [['F']], texte: 'Ouvrir en pleine page' }
      ]
    },
    {
      titre: 'Dans un mail',
      lignes: [
        { touches: [['Ctrl', 'Enter']], texte: 'Envoyer la réponse (une confirmation est demandée)' },
        { touches: [['J'], ['K']], texte: 'Mail suivant, précédent' }
      ]
    },
    { titre: 'Partout', lignes: [{ touches: [['?']], texte: 'Afficher cette aide' }] }
  ];

  $effect(() => {
    if (!dlg) return;
    if (ouvert && !dlg.open) dlg.showModal();
    else if (!ouvert && dlg.open) dlg.close();
  });
</script>

<dialog
  bind:this={dlg}
  class="confirmation aide-clavier"
  aria-labelledby="{uid}-titre"
  onclose={() => (ouvert = false)}
  onclick={(e) => {
    if (e.target === dlg) ouvert = false;
  }}
>
  <div class="dlg-corps">
    <h2 id="{uid}-titre">Raccourcis clavier</h2>
    <div class="dlg-texte">
      {#each GROUPES as g (g.titre)}
        <h3>{g.titre}</h3>
        <dl>
          {#each g.lignes as l (l.texte)}
            <div>
              <dt>
                {#each l.touches as combo, i (i)}{#if i > 0}<span class="ou">ou</span>{/if}<Kbd t={combo} />{/each}
              </dt>
              <dd>{l.texte}</dd>
            </div>
          {/each}
        </dl>
      {/each}
      <p class="note">Les raccourcis ne s’activent pas quand vous tapez dans un champ.</p>
    </div>
  </div>
  <div class="dlg-pied">
    <button type="button" class="btn principal" onclick={() => (ouvert = false)}>Fermer</button>
  </div>
</dialog>

<style>
  .aide-clavier {
    max-height: calc(100vh - 32px);
    overflow: auto;
  }
  h3 {
    margin: var(--e4) 0 var(--e1);
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  dl {
    margin: 0;
  }
  dl > div {
    display: grid;
    grid-template-columns: 9rem minmax(0, 1fr);
    gap: var(--e3);
    align-items: center;
    padding: var(--e1) 0;
    border-top: 1px solid var(--line);
  }
  dt {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e1);
  }
  dd {
    margin: 0;
  }
  .ou {
    color: var(--muted);
    font-size: 14px;
  }
  .note {
    margin-top: var(--e4);
    font-size: 14px;
    color: var(--muted);
  }
</style>
