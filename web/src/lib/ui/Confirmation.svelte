<script lang="ts">
  // Dialogue de confirmation (élément <dialog> natif, modal) :
  //   <Confirmation bind:ouvert titre="Écarter ce document ?" libelle="Écarter" danger onconfirmer={ecarter}>
  //     Il ne sera pas envoyé à Dext.
  //   </Confirmation>
  // Échap ou « Annuler » ferment sans rien faire ; le focus revient où il était.
  import type { Snippet } from 'svelte';

  let {
    ouvert = $bindable(false),
    titre,
    libelle = 'Confirmer',
    libelleAnnuler = 'Annuler',
    danger = false,
    onconfirmer,
    children
  }: {
    ouvert?: boolean;
    titre: string;
    libelle?: string;
    libelleAnnuler?: string;
    danger?: boolean;
    onconfirmer: () => void | Promise<void>;
    children?: Snippet;
  } = $props();

  let dlg = $state<HTMLDialogElement>();
  let enCours = $state(false);
  const uid = $props.id();

  $effect(() => {
    if (!dlg) return;
    if (ouvert && !dlg.open) dlg.showModal();
    else if (!ouvert && dlg.open) dlg.close();
  });

  async function confirmer() {
    enCours = true;
    try {
      await onconfirmer();
      ouvert = false;
    } finally {
      enCours = false;
    }
  }
</script>

<dialog
  bind:this={dlg}
  class="confirmation"
  aria-labelledby="{uid}-titre"
  onclose={() => (ouvert = false)}
  onclick={(e) => {
    if (e.target === dlg) ouvert = false;
  }}
>
  <div class="dlg-corps">
    <h2 id="{uid}-titre">{titre}</h2>
    {#if children}<div class="dlg-texte">{@render children()}</div>{/if}
  </div>
  <div class="dlg-pied">
    <button type="button" class="btn" onclick={() => (ouvert = false)}>{libelleAnnuler}</button>
    <button type="button" class="btn principal" class:danger class:en-cours={enCours} disabled={enCours} onclick={confirmer}>
      {libelle}
    </button>
  </div>
</dialog>
