<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { CATEGORIES, classeCategorie, libelleCategorie, libelleStatutReponse } from '#lib/reponses.ts';
  import type { BoiteVisible, LigneMail, Page, SocieteVisible } from '#lib/types.ts';

  let societes = $state<SocieteVisible[]>([]);
  let boites = $state<BoiteVisible[]>([]);
  let resultat = $state<Page<LigneMail> | null>(null);
  let erreur = $state('');

  const p = page.url.searchParams;
  let filtres = $state({
    societe: p.get('societe') ?? '',
    boite: p.get('boite') ?? '',
    du: p.get('du') ?? '',
    au: p.get('au') ?? '',
    q: p.get('q') ?? '',
    // Par défaut, les publicités sont masquées ; « toutes » n'est pas envoyé à l'API.
    categorie: p.get('categorie') ?? 'hors_publicite',
    page: Number(p.get('page') ?? '1')
  });

  let boitesFiltrees = $derived(filtres.societe ? boites.filter((b) => String(b.societe_id) === filtres.societe) : boites);
  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);

  $effect(() => {
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    api.get<BoiteVisible[]>('/api/boites').then((v) => (boites = v));
  });

  function requete() {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filtres))
      if (v !== '' && !(k === 'page' && v === 1) && !(k === 'categorie' && v === 'hors_publicite')) q.set(k, String(v));
    return q.toString();
  }
  function requeteApi() {
    const q = new URLSearchParams(requete());
    if (q.get('categorie') === 'toutes') q.delete('categorie');
    else if (!q.has('categorie')) q.set('categorie', 'hors_publicite');
    return q.toString();
  }

  async function charger() {
    erreur = '';
    const q = requete();
    try {
      resultat = await api.get<Page<LigneMail>>(`/api/mails?${requeteApi()}`);
      history.replaceState(history.state, '', `/mails${q ? '?' + q : ''}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });

  function chercher(e: SubmitEvent) {
    e.preventDefault();
    filtres.page = 1;
    charger();
  }
  function allerPage(n: number) {
    filtres.page = n;
    charger();
  }
</script>

<h1>Mails</h1>
<form class="carte filtres" onsubmit={chercher}>
  <div class="champs">
    <div>
      <label for="s">Structure</label>
      <select id="s" bind:value={filtres.societe} onchange={() => (filtres.boite = '')}>
        <option value="">Toutes</option>
        {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
      </select>
    </div>
    <div>
      <label for="b">Boîte</label>
      <select id="b" bind:value={filtres.boite}>
        <option value="">Toutes</option>
        {#each boitesFiltrees as b (b.id)}<option value={String(b.id)}>{b.libelle}</option>{/each}
      </select>
    </div>
    <div><label for="du">Du</label><input id="du" type="date" bind:value={filtres.du} /></div>
    <div><label for="au">Au</label><input id="au" type="date" bind:value={filtres.au} /></div>
    <div>
      <label for="c">Catégorie</label>
      <select id="c" bind:value={filtres.categorie}>
        <option value="hors_publicite">Toutes sauf publicités</option>
        <option value="toutes">Toutes</option>
        <option value="non_trie">Non triés</option>
        {#each CATEGORIES as c (c)}<option value={c}>{libelleCategorie[c]}</option>{/each}
      </select>
    </div>
    <div><label for="q">Expéditeur ou objet</label><input id="q" type="search" bind:value={filtres.q} /></div>
  </div>
  <button>Filtrer</button>
</form>

{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if resultat}
  <p class="discret">{resultat.total} mail{resultat.total > 1 ? 's' : ''}</p>
  <table>
    <thead><tr><th>Date</th><th>Expéditeur</th><th>Objet</th><th>Catégorie</th><th>PJ</th><th>Boîtes</th></tr></thead>
    <tbody>
      {#each resultat.elements as m (m.id)}
        <tr class="cliquable" onclick={() => goto(`/mails/${m.id}`)}>
          <td class="date">{quand(m.date_mail)}</td>
          <td>{m.expediteur || '—'}</td>
          <td><a href="/mails/{m.id}">{m.objet || '(sans objet)'}</a></td>
          <td>
            {#if m.categorie}<span class="etiquette {classeCategorie[m.categorie]}">{libelleCategorie[m.categorie]}</span>{/if}
            {#if m.statut_reponse}<span class="discret">{libelleStatutReponse[m.statut_reponse]}</span>{/if}
          </td>
          <td class="nombre">{m.nb_pieces || ''}</td>
          <td class="discret">{m.boites.join(', ')}</td>
        </tr>
      {:else}
        <tr><td colspan="6" class="discret">Aucun mail.</td></tr>
      {/each}
    </tbody>
  </table>
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={filtres.page <= 1} onclick={() => allerPage(filtres.page - 1)}>Précédent</button>
      <span class="discret">Page {filtres.page} sur {pages}</span>
      <button class="second" disabled={filtres.page >= pages} onclick={() => allerPage(filtres.page + 1)}>Suivant</button>
    </div>
  {/if}
{/if}

<style>
  .filtres { margin-bottom: 1rem; }
</style>
