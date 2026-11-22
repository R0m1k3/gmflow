<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { LigneJournal, Page } from '#lib/types.ts';

  let objet = $state('');
  let numero = $state(1);
  let r = $state<Page<LigneJournal> | null>(null);
  let erreur = $state('');
  let chargement = $state(true);
  let pages = $derived(r ? Math.max(1, Math.ceil(r.total / 50)) : 1);

  async function charger() {
    const q = new URLSearchParams({ page: String(numero) });
    if (objet) q.set('objet_type', objet);
    erreur = '';
    chargement = true;
    try {
      r = await api.get<Page<LigneJournal>>(`/api/admin/journal?${q}`);
    } catch (e) {
      erreur = message(e);
    } finally {
      chargement = false;
    }
  }
  $effect(() => {
    charger();
  });
  const types = ['', 'boite_mail', 'societe', 'utilisateur', 'mail', 'piece_jointe', 'claude'];
  const libelleType: Record<string, string> = { '': 'Tout', boite_mail: 'Boîtes mail', societe: 'Sociétés', utilisateur: 'Utilisateurs', mail: 'Mails', piece_jointe: 'Pièces jointes', claude: 'Connexion à Claude' };
  const libelleObjet = (t: string) => libelleType[t] ?? t;
</script>

<div class="entete">
  <div>
    <h1>Journal</h1>
    <p class="sous">L'historique des modifications faites dans GMFlow : qui a changé quoi, et quand.</p>
  </div>
</div>
{#if erreur}<div class="bandeau erreur">{erreur} <button class="discret" onclick={charger}>Réessayer</button></div>{/if}

<div class="filtre">
  <label for="t">Afficher les actions sur</label>
  <select id="t" bind:value={objet} onchange={() => { numero = 1; charger(); }}>
    {#each types as t (t)}<option value={t}>{libelleObjet(t)}</option>{/each}
  </select>
</div>

{#if r}
  {#if r.elements.length}
    <table>
      <thead><tr><th>Quand</th><th>Auteur</th><th>Action</th><th>Concerne</th><th>Détails</th></tr></thead>
      <tbody>
        {#each r.elements as l, i (i)}
          <tr>
            <td class="date">{quand(l.quand)}</td>
            <td>{l.auteur ?? 'Système'}</td>
            <td>{l.action}</td>
            <td class="discret">{libelleObjet(l.objet_type)}</td>
            <td class="discret details">
              {#if Object.keys(l.details ?? {}).length}
                <details><summary>Voir</summary>{JSON.stringify(l.details)}</details>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if pages > 1}
      <div class="pagination">
        <button class="second" disabled={numero <= 1} onclick={() => { numero--; charger(); }}>Précédent</button>
        <span>Page {numero} sur {pages}</span>
        <button class="second" disabled={numero >= pages} onclick={() => { numero++; charger(); }}>Suivant</button>
      </div>
    {/if}
  {:else}
    <div class="vide"><strong>Aucune action enregistrée</strong><p>{objet ? 'Rien à signaler pour ce type d\'objet.' : 'Les modifications apparaîtront ici dès qu\'elles seront faites.'}</p></div>
  {/if}
{:else if chargement}
  <div class="charge" aria-busy="true"><span></span></div>
{/if}

<style>
  .filtre { max-width: 280px; margin-bottom: var(--e4); }
  .details { max-width: 420px; word-break: break-word; }
  .details summary { cursor: pointer; color: var(--accent); }
</style>
