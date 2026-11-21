<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { LigneJournal, Page } from '#lib/types.ts';

  let objet = $state('');
  let numero = $state(1);
  let r = $state<Page<LigneJournal> | null>(null);
  let erreur = $state('');
  let pages = $derived(r ? Math.max(1, Math.ceil(r.total / 50)) : 1);

  async function charger() {
    const q = new URLSearchParams({ page: String(numero) });
    if (objet) q.set('objet_type', objet);
    try {
      r = await api.get<Page<LigneJournal>>(`/api/admin/journal?${q}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });
  const types = ['', 'boite_mail', 'societe', 'utilisateur', 'mail', 'piece_jointe', 'claude'];
</script>

<h1>Journal</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
<p style="max-width:260px">
  <label for="t">Objet</label>
  <select id="t" bind:value={objet} onchange={() => { numero = 1; charger(); }}>
    {#each types as t (t)}<option value={t}>{t || 'Tous'}</option>{/each}
  </select>
</p>
{#if r}
  <table>
    <thead><tr><th>Quand</th><th>Auteur</th><th>Action</th><th>Objet</th><th>Détails</th></tr></thead>
    <tbody>
      {#each r.elements as l, i (i)}
        <tr>
          <td class="date">{quand(l.quand)}</td>
          <td>{l.auteur ?? 'système'}</td>
          <td>{l.action}</td>
          <td class="discret">{l.objet_type}{l.objet_id ? ` #${l.objet_id}` : ''}</td>
          <td class="discret details">{Object.keys(l.details ?? {}).length ? JSON.stringify(l.details) : ''}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={numero <= 1} onclick={() => { numero--; charger(); }}>Précédent</button>
      <span class="discret">Page {numero} sur {pages}</span>
      <button class="second" disabled={numero >= pages} onclick={() => { numero++; charger(); }}>Suivant</button>
    </div>
  {/if}
{/if}

<style>
  .details { font-size: 0.8rem; word-break: break-word; max-width: 420px; }
</style>
