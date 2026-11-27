<script lang="ts">
  // Notes internes d'une pièce (audit U23), en carte de la fiche : <NotesInternes documentId={d.id} />.
  // Plusieurs notes signées et datées, la plus récente en haut ; ajout par tout utilisateur qui
  // voit la pièce, « Supprimer » pour son auteur ou un administrateur (le serveur décide :
  // `supprimable`). Jamais envoyées à l'extérieur. Ctrl+Entrée ajoute la note.
  import { message } from '#lib/api.ts';
  import { ajouterNote, NOTE_MAX, notesDocument, supprimerNote, type NoteInterne } from '#lib/notes.ts';
  import Confirmation from './Confirmation.svelte';
  import { jourHeure } from './DocOutils.ts';

  let { documentId, onchange }: { documentId: number; onchange?: (n: number) => void } = $props();

  const uid = $props.id();
  let notes = $state<NoteInterne[]>([]);
  let charge = $state(false);
  let texte = $state('');
  let enCours = $state(false);
  let erreur = $state('');
  let aSupprimer = $state<NoteInterne | null>(null);
  let dlgSupprimer = $state(false);

  let jeton = 0;
  async function charger(id: number) {
    const moi = ++jeton;
    try {
      const n = await notesDocument(id);
      if (moi !== jeton) return;
      notes = n;
      erreur = '';
    } catch (e) {
      if (moi === jeton) erreur = message(e);
    } finally {
      if (moi === jeton) charge = true;
    }
  }
  $effect(() => {
    const id = documentId;
    charge = false;
    notes = [];
    texte = '';
    erreur = '';
    void charger(id);
  });

  let vide = $derived(texte.trim() === '');
  let trop = $derived(texte.trim().length > NOTE_MAX);

  async function ajouter() {
    if (vide || trop || enCours) return;
    enCours = true;
    erreur = '';
    try {
      const n = await ajouterNote(documentId, texte.trim());
      notes = [n, ...notes];
      texte = '';
      onchange?.(notes.length);
    } catch (e) {
      erreur = message(e);
    } finally {
      enCours = false;
    }
  }

  async function supprimer() {
    const n = aSupprimer;
    if (!n) return;
    erreur = '';
    try {
      await supprimerNote(n.id);
      notes = notes.filter((x) => x.id !== n.id);
      onchange?.(notes.length);
    } catch (e) {
      erreur = message(e);
    } finally {
      aSupprimer = null;
    }
  }

  function touche(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void ajouter();
    }
  }
</script>

<div class="carte-fiche notes-internes">
  <h3 class="titre-carte" id="{uid}-titre">
    Notes internes
    {#if notes.length}<span class="etiquette-carte">{notes.length}</span>{/if}
  </h3>
  {#if charge && notes.length}
    <ul class="notes" aria-labelledby="{uid}-titre">
      {#each notes as n (n.id)}
        <li class="note">
          <div class="qui">
            <span class="auteur">{n.auteur_nom}</span>
            <time class="t2 petit" datetime={n.cree_le}>{jourHeure(n.cree_le)}</time>
            {#if n.supprimable}
              <button
                type="button"
                class="btn-lien petit suppr"
                onclick={() => {
                  aSupprimer = n;
                  dlgSupprimer = true;
                }}>Supprimer</button
              >
            {/if}
          </div>
          <p class="texte">{n.texte}</p>
        </li>
      {/each}
    </ul>
  {/if}
  <form
    class="ajout"
    onsubmit={(e) => {
      e.preventDefault();
      void ajouter();
    }}
  >
    <label class="sr-seul" for="{uid}-texte">Nouvelle note interne</label>
    <textarea
      id="{uid}-texte"
      rows="2"
      maxlength={NOTE_MAX}
      placeholder="Ajouter une note pour l’équipe"
      aria-describedby="{uid}-aide"
      bind:value={texte}
      onkeydown={touche}
    ></textarea>
    <div class="pied">
      <span class="t2 petit" id="{uid}-aide">Visible seulement dans GMFlow : jamais envoyée au fournisseur ni à Dext ou Yooz.</span>
      <button type="submit" class="btn sm" class:en-cours={enCours} disabled={vide || trop || enCours}>Ajouter</button>
    </div>
  </form>
  {#if erreur}<p class="erreur petit" role="alert">{erreur}</p>{/if}
</div>

<Confirmation bind:ouvert={dlgSupprimer} titre="Supprimer cette note ?" libelle="Supprimer" danger onconfirmer={supprimer}>
  {#if aSupprimer}<p>Note de {aSupprimer.auteur_nom}, du {jourHeure(aSupprimer.cree_le)}. La suppression est notée dans le journal.</p>{/if}
</Confirmation>

<style>
  .notes {
    list-style: none;
    margin: 0 0 var(--e3);
    padding: 0;
  }
  .note {
    padding: var(--e2) 0;
    border-bottom: 1px solid var(--line-faint);
  }
  .note:first-child {
    padding-top: 0;
  }
  .qui {
    display: flex;
    align-items: baseline;
    gap: var(--e2);
    flex-wrap: wrap;
  }
  .auteur {
    font-weight: 500;
  }
  .suppr {
    margin-left: auto;
  }
  .texte {
    margin: 2px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .ajout textarea {
    min-height: 64px;
    resize: vertical;
  }
  .pied {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--e3);
    margin-top: var(--e2);
  }
  .pied .btn {
    flex: none;
  }
  .erreur {
    color: var(--err);
    margin: var(--e2) 0 0;
  }
  @media (max-width: 760px) {
    .suppr {
      min-height: 44px;
    }
  }
</style>
