<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { dateSimple, quand } from '#lib/format.ts';
  import { classePriorite, libelleBrouillon, libelleCategorie, libellePriorite } from '#lib/reponses.ts';
  import type { LigneARepondre, Page, SemaineTri, SocieteVisible } from '#lib/types.ts';

  let societes = $state<SocieteVisible[]>([]);
  let resultat = $state<Page<LigneARepondre> | null>(null);
  let semaines = $state<SemaineTri[]>([]);
  let erreur = $state('');

  const p = page.url.searchParams;
  let filtres = $state({
    societe: p.get('societe') ?? '',
    priorite: p.get('priorite') ?? '',
    page: Number(p.get('page') ?? '1')
  });
  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);

  $effect(() => {
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    api.get<SemaineTri[]>('/api/tri/semaines').then((v) => (semaines = v));
  });

  async function charger() {
    erreur = '';
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filtres)) if (v !== '' && !(k === 'page' && v === 1)) q.set(k, String(v));
    const s = q.toString();
    try {
      resultat = await api.get<Page<LigneARepondre>>(`/api/a-repondre${s ? '?' + s : ''}`);
      history.replaceState(history.state, '', `/a-repondre${s ? '?' + s : ''}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });

  function filtrer() {
    filtres.page = 1;
    charger();
  }
  function allerPage(n: number) {
    filtres.page = n;
    charger();
  }

  let enCours = $state<number | null>(null);

  /** Classement à la main depuis la file : il sert d'exemple au tri des mails suivants. */
  async function classer(ev: Event, m: LigneARepondre, categorie: 'question' | 'information' | 'publicite') {
    ev.stopPropagation();
    enCours = m.id;
    erreur = '';
    try {
      await api.put(`/api/mails/${m.id}/categorie`, { categorie });
      if (categorie === 'question') m.categorie_par = 'humain';
      else await charger();
      api.get<SemaineTri[]>('/api/tri/semaines').then((v) => (semaines = v));
    } catch (e) {
      erreur = message(e);
    } finally {
      enCours = null;
    }
  }

  function justesse(s: SemaineTri) {
    return Math.round((100 * (s.revus - s.corriges)) / s.revus);
  }

  function lien(s: SemaineTri, categorie: string | null) {
    return `/mails?du=${s.debut}&au=${s.fin}&categorie=${categorie ?? 'non_trie'}`;
  }
</script>

<h1>À répondre</h1>
<div class="carte filtres actions">
  <div>
    <label for="s">Structure</label>
    <select id="s" bind:value={filtres.societe} onchange={filtrer}>
      <option value="">Toutes</option>
      {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
    </select>
  </div>
  <div>
    <label for="p">Priorité</label>
    <select id="p" bind:value={filtres.priorite} onchange={filtrer}>
      <option value="">Toutes</option>
      <option value="haute">Haute</option>
      <option value="normale">Normale</option>
      <option value="basse">Basse</option>
    </select>
  </div>
</div>

{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if resultat}
  <p class="discret">
    {resultat.total} question{resultat.total > 1 ? 's' : ''} en attente, priorité haute d'abord puis les plus anciennes.
    Chaque classement à la main sert d'exemple au tri des mails suivants ; après trois mails d'un même
    expéditeur classés pareil, ses mails suivants le sont directement.
  </p>
  <table>
    <thead><tr><th>Priorité</th><th>Date</th><th>Expéditeur</th><th>Question</th><th>Boîte</th><th>Classer</th></tr></thead>
    <tbody>
      {#each resultat.elements as m (m.id)}
        <tr class="cliquable" onclick={() => goto(`/mails/${m.id}`)}>
          <td>
            {#if m.priorite}<span class="etiquette {classePriorite[m.priorite]}">{libellePriorite[m.priorite]}</span>{/if}
          </td>
          <td class="date">{quand(m.date_mail)}</td>
          <td>{m.expediteur || '—'}</td>
          <td>
            <a href="/mails/{m.id}">{m.resume || m.objet || '(sans objet)'}</a>
            {#if m.resume && m.objet}<div class="discret">{m.objet}</div>{/if}
            {#if m.brouillon}<div><span class="etiquette att">{libelleBrouillon[m.brouillon]}</span></div>{/if}
          </td>
          <td class="discret">{m.boites.join(', ')}{m.structures.length > 1 ? ` (${m.structures.join(', ')})` : ''}</td>
          <td class="classer">
            {#if m.categorie_par === 'humain'}
              <span class="discret">Question confirmée</span>
            {:else}
              <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'question')}>Vraie question</button>
              <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'information')}>Information</button>
              <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'publicite')}>Publicité</button>
            {/if}
          </td>
        </tr>
      {:else}
        <tr><td colspan="6" class="discret">Aucune question en attente.</td></tr>
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

{#if semaines.length}
  <h2>Tri des mails par semaine</h2>
  <p class="discret">Pour vérifier qu'aucune question ne s'est glissée ailleurs : chaque nombre ouvre les mails de la catégorie.</p>
  <div class="grille">
    {#each semaines as s, i (s.debut)}
      <div class="carte">
        <div class="discret">{i === 0 ? 'Cette semaine' : 'Semaine dernière'} · du {dateSimple(s.debut)} au {dateSimple(s.fin)}</div>
        <table class="bilan">
          <tbody>
            {#each s.par_categorie as c (c.cle)}
              <tr>
                <td><a href={lien(s, c.cle)}>{c.cle ? libelleCategorie[c.cle] : 'Non triés'}</a></td>
                <td class="nombre">{c.n}</td>
              </tr>
            {:else}
              <tr><td class="discret">Aucun mail.</td></tr>
            {/each}
          </tbody>
        </table>
        <div class="discret">
          {s.a_repondre} à répondre, {s.repondu} répondu{s.repondu > 1 ? 's' : ''}, {s.sans_suite} sans suite
        </div>
        {#if s.revus}
          <div class="discret">
            Tri automatique juste à {justesse(s)} % : {s.corriges} corrigé{s.corriges > 1 ? 's' : ''} sur {s.revus} revu{s.revus > 1 ? 's' : ''} à la main
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .filtres { margin-bottom: 1rem; align-items: end; }
  .filtres > div { min-width: 200px; }
  .bilan { margin: 0.5rem 0; border: none; }
  .bilan td { padding: 0.25rem 0.4rem; }
  .classer { white-space: nowrap; }
  .classer button { padding: 0.2rem 0.5rem; font-size: 0.85rem; }
</style>
