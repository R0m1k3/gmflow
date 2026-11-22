<script lang="ts">
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
    } finally {
      chargement = false;
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
  let choix = $state<number | null>(null);
  let info = $state('');
  let chargement = $state(true);

  /** Classement à la main depuis la file : il sert d'exemple au tri des mails suivants. */
  async function classer(ev: Event, m: LigneARepondre, categorie: 'question' | 'information' | 'publicite') {
    ev.stopPropagation();
    enCours = m.id;
    erreur = '';
    info = '';
    try {
      await api.put(`/api/mails/${m.id}/categorie`, { categorie });
      choix = null;
      if (categorie === 'question') {
        m.categorie_par = 'humain';
        info = 'Question confirmée : elle reste dans la liste.';
      } else {
        await charger();
        info = `Mail classé en « ${libelleCategorie[categorie]} » : il ne demande plus de réponse.`;
      }
      api.get<SemaineTri[]>('/api/tri/semaines').then((v) => (semaines = v));
    } catch (e) {
      erreur = message(e);
    } finally {
      enCours = null;
    }
  }

  const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
  const prioritesChoix = [
    { v: '', l: 'Toutes' },
    { v: 'haute', l: 'Haute' },
    { v: 'normale', l: 'Normale' },
    { v: 'basse', l: 'Basse' }
  ];

  function justesse(s: SemaineTri) {
    return Math.round((100 * (s.revus - s.corriges)) / s.revus);
  }

  function lien(s: SemaineTri, categorie: string | null) {
    return `/mails?du=${s.debut}&au=${s.fin}&categorie=${categorie ?? 'non_trie'}`;
  }
</script>

<div class="entete">
  <div>
    <h1>À répondre</h1>
    <p class="sous">Les mails qui posent une question. Ouvrez-en un pour préparer la réponse, ou dites que ce n'est pas une question.</p>
  </div>
</div>

<div class="champs filtres">
  <div>
    <label for="s">Société</label>
    <select id="s" bind:value={filtres.societe} onchange={filtrer}>
      <option value="">Toutes les sociétés</option>
      {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
    </select>
  </div>
</div>
<div class="onglets" role="group" aria-label="Priorité">
  {#each prioritesChoix as o (o.v)}
    <button class:actif={filtres.priorite === o.v} aria-pressed={filtres.priorite === o.v} onclick={() => { filtres.priorite = o.v; filtrer(); }}>
      {o.v ? 'Priorité ' + o.l.toLowerCase() : 'Toutes les priorités'}
    </button>
  {/each}
</div>

{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}
{#if erreur}
  <div class="bandeau erreur" role="alert">
    {erreur}
    <div class="actions"><button class="second petit" onclick={charger}>Réessayer</button></div>
  </div>
{/if}

{#if resultat}
  <p class="discret">
    {resultat.total === 0 ? 'Aucun mail' : resultat.total + ' mail' + (resultat.total > 1 ? 's attendent' : ' attend')} une réponse, priorité haute d'abord puis les plus anciens.
  </p>
  {#if resultat.elements.length}
    <ul class="liste">
      {#each resultat.elements as m (m.id)}
        <li class="mail-ligne">
          <div class="principal">
            <a href="/mails/{m.id}">{m.resume || m.objet || '(sans objet)'}</a>
            {#if m.resume && m.objet}<span class="discret">Objet : {m.objet}</span>{/if}
            <span class="discret">{m.expediteur || 'Non renseigné'} · <span class="date">{quand(m.date_mail)}</span> · {m.boites.join(', ')}{m.structures.length > 1 ? ` (${m.structures.join(', ')})` : ''}</span>
            <span class="mail-etats">
              {#if m.priorite}<span class="etiquette {classePriorite[m.priorite]}">Priorité {libellePriorite[m.priorite]}</span>{/if}
              {#if m.brouillon}<span class="etiquette att">{cap(libelleBrouillon[m.brouillon])}</span>{/if}
              {#if m.categorie_par === 'humain'}<span class="etiquette ok">Question confirmée</span>{/if}
            </span>
          </div>
          <div class="droite">
            <a class="bouton" href="/mails/{m.id}">Répondre</a>
            {#if choix === m.id}
              <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'information')}>Information</button>
              <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'publicite')}>Publicité</button>
              <button class="discret" onclick={() => (choix = null)}>Annuler</button>
            {:else}
              {#if m.categorie_par !== 'humain'}
                <button class="second" disabled={enCours === m.id} onclick={(ev) => classer(ev, m, 'question')}>Vraie question</button>
              {/if}
              <button class="second" disabled={enCours === m.id} onclick={() => (choix = m.id)}>Ce n'est pas une question</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="vide">
      <strong>Rien à répondre</strong>
      <p>Aucun mail n'attend de réponse{filtres.societe || filtres.priorite ? ' avec ces filtres' : ''}. Les nouvelles questions apparaîtront ici après la prochaine relève des boîtes mail.</p>
      <div class="actions">
        {#if filtres.societe || filtres.priorite}
          <button class="second" onclick={() => { filtres.societe = ''; filtres.priorite = ''; filtrer(); }}>Retirer les filtres</button>
        {/if}
        <a class="bouton second" href="/mails">Voir tous les mails</a>
      </div>
    </div>
  {/if}
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={filtres.page <= 1} onclick={() => allerPage(filtres.page - 1)}>Précédent</button>
      <span class="discret">Page {filtres.page} sur {pages}</span>
      <button class="second" disabled={filtres.page >= pages} onclick={() => allerPage(filtres.page + 1)}>Suivant</button>
    </div>
  {/if}
{:else if chargement && !erreur}
  <div class="charge" aria-busy="true"><span></span></div>
{/if}

{#if semaines.length}
  <div class="section">
    <div class="titre"><h2>Vérifier le tri des mails</h2></div>
    <p class="discret">
      Claude range chaque mail reçu. Chaque classement que vous faites ici lui sert d'exemple : après trois mails
      d'un même expéditeur classés pareil, ses mails suivants le sont directement. Chaque nombre ouvre les mails
      concernés, pour vérifier qu'aucune question ne s'est glissée ailleurs.
    </p>
    {#each semaines as s, i (s.debut)}
      <details class="carte mail-semaine" open={i === 0}>
        <summary>{i === 0 ? 'Cette semaine' : 'Semaine dernière'} <span class="discret">du {dateSimple(s.debut)} au {dateSimple(s.fin)}</span></summary>
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
        <p class="discret">{s.a_repondre} à répondre, {s.repondu} répondu{s.repondu > 1 ? 's' : ''}, {s.sans_suite} sans suite</p>
        {#if s.revus}
          <p class="discret">Tri automatique juste à {justesse(s)} % : {s.corriges} corrigé{s.corriges > 1 ? 's' : ''} sur {s.revus} revu{s.revus > 1 ? 's' : ''} à la main</p>
        {/if}
      </details>
    {/each}
  </div>
{/if}
