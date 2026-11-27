<script lang="ts">
  // Fiche document plein écran (Plein écran F de la fiche). Même fiche que le volet des listes ;
  // la file « À contrôler » (GET /api/a-traiter) sert à la position, à « Ensuite », et à J/K. Après une
  // validation, la fiche suivante s'ouvre ; après la dernière, retour à l'accueil.
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { aTraiter } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { FileATraiter } from '#lib/types.ts';
  import DocFiche, { type PositionFiche, type ResultatFiche } from '#lib/ui/DocFiche.svelte';
  import { nomCourtDocument } from '#lib/ui/DocOutils.ts';

  let id = $derived(Number(page.params.id));
  let file = $state<FileATraiter | null>(null);
  let annonce = $state('');
  let annoncePour = 0;

  $effect(() => {
    void societe.id;
    untrack(() => {
      aTraiter
        .file(societe.id)
        .then((f) => (file = f))
        .catch(() => (file = null));
    });
  });
  $effect(() => {
    if (id !== annoncePour) annonce = '';
  });

  let liste = $derived(file?.a_valider ?? []);
  let i = $derived(liste.findIndex((l) => l.id === id));
  let position = $derived<PositionFiche | null>(i >= 0 && file ? { section: 'Pièce', n: i + 1, total: file.compteurs.a_valider } : null);
  /** Le suivant dans la file : après celui-ci, sinon le premier (comme le serveur). */
  let suivantListe = $derived(i >= 0 ? (liste[i + 1] ?? (i > 0 ? liste[0] : undefined)) : liste[0]);
  let precedentListe = $derived(i > 0 ? liste[i - 1] : undefined);

  function aller(cible: number | undefined) {
    if (cible) void goto(`/documents/${cible}`, { replace: true });
  }

  $effect(() =>
    raccourcis({
      j: () => (suivantListe ? aller(suivantListe.id) : false),
      k: () => (precedentListe ? aller(precedentListe.id) : false)
    })
  );

  async function apres(r: ResultatFiche) {
    if (r.suivant) {
      annonce = r.info;
      annoncePour = r.suivant;
      // La file a changé : relue pour la position et « Ensuite ».
      aTraiter.file(societe.id).then((f) => (file = f)).catch(() => {});
      await goto(`/documents/${r.suivant}`, { replace: true });
    } else {
      await goto('/');
    }
  }
</script>

<svelte:head><title>Fiche document · GMFlow</title></svelte:head>

<div class="pleine fixe">
  {#if Number.isInteger(id) && id > 0}
    <DocFiche
      {id}
      pleinEcran
      retour="/"
      {position}
      ensuite={suivantListe && suivantListe.id !== id ? nomCourtDocument(suivantListe) : undefined}
      sansSuite="Aucune autre pièce à contrôler"
      {annonce}
      onfait={apres}
      onchange={() => void aTraiter.file(societe.id).then((f) => (file = f)).catch(() => {})}
      onprecedent={precedentListe ? () => aller(precedentListe?.id) : undefined}
      onsuivant={suivantListe && suivantListe.id !== id ? () => aller(suivantListe?.id) : undefined}
    />
  {/if}
</div>
