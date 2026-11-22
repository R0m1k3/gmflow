<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { classeCategorie, libelleCategorie, libelleStatutReponse } from '#lib/reponses.ts';
  import type { BoiteVisible, LigneMail, Page, SocieteVisible } from '#lib/types.ts';

  let societes = $state<SocieteVisible[]>([]);
  let boites = $state<BoiteVisible[]>([]);
  let resultat = $state<Page<LigneMail> | null>(null);
  let erreur = $state('');
  let chargement = $state(true);

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
    } finally {
      chargement = false;
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
  const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
  function categorie(c: string) {
    filtres.categorie = c;
    filtres.page = 1;
    charger();
  }
  const ONGLETS: { v: string; l: string }[] = [
    { v: 'hors_publicite', l: 'Tous sauf publicités' },
    { v: 'question', l: 'Questions' },
    { v: 'information', l: 'Informations' },
    { v: 'document_seul', l: 'Documents seuls' },
    { v: 'notification', l: 'Notifications' },
    { v: 'publicite', l: 'Publicités' },
    { v: 'non_trie', l: 'Non triés' },
    { v: 'toutes', l: 'Tous' }
  ];
  let filtresActifs = $derived(!!(filtres.societe || filtres.boite || filtres.du || filtres.au || filtres.q || filtres.categorie !== 'hors_publicite'));
  function toutEffacer() {
    filtres.societe = filtres.boite = filtres.du = filtres.au = filtres.q = '';
    filtres.categorie = 'hors_publicite';
    filtres.page = 1;
    charger();
  }
  function allerPage(n: number) {
    filtres.page = n;
    charger();
  }
</script>

<div class="entete">
  <div>
    <h1>Mails</h1>
    <p class="sous">Tous les mails reçus dans vos boîtes mail. Cherchez un message, ou ouvrez-en un pour le lire et y répondre.</p>
  </div>
  <div class="actions"><a class="bouton second" href="/a-repondre">Voir ceux à répondre</a></div>
</div>

<div class="onglets" role="group" aria-label="Type de mail">
  {#each ONGLETS as o (o.v)}
    <button class:actif={filtres.categorie === o.v} aria-pressed={filtres.categorie === o.v} onclick={() => categorie(o.v)}>{o.l}</button>
  {/each}
</div>

<form class="mail-filtres" onsubmit={chercher}>
  <div class="champs">
    <div><label for="q">Expéditeur ou objet</label><input id="q" type="search" bind:value={filtres.q} placeholder="Nom, adresse ou mot de l'objet" /></div>
    <div>
      <label for="s">Société</label>
      <select id="s" bind:value={filtres.societe} onchange={() => (filtres.boite = '')}>
        <option value="">Toutes les sociétés</option>
        {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
      </select>
    </div>
    <div>
      <label for="b">Boîte mail</label>
      <select id="b" bind:value={filtres.boite}>
        <option value="">Toutes les boîtes mail</option>
        {#each boitesFiltrees as b (b.id)}<option value={String(b.id)}>{b.libelle}</option>{/each}
      </select>
    </div>
  </div>
  <details open={!!(filtres.du || filtres.au)}>
    <summary>Choisir une période</summary>
    <div class="champs mail-periode">
      <div><label for="du">Du</label><input id="du" type="date" bind:value={filtres.du} /></div>
      <div><label for="au">Au</label><input id="au" type="date" bind:value={filtres.au} /></div>
    </div>
  </details>
  <div class="actions">
    <button>Rechercher</button>
    {#if filtresActifs}<button type="button" class="discret" onclick={toutEffacer}>Effacer les filtres</button>{/if}
  </div>
</form>

{#if erreur}
  <div class="bandeau erreur" role="alert">
    {erreur}
    <div class="actions"><button class="second petit" onclick={charger}>Réessayer</button></div>
  </div>
{/if}
{#if resultat}
  <p class="discret">{resultat.total === 0 ? 'Aucun mail' : resultat.total + ' mail' + (resultat.total > 1 ? 's' : '')}{filtresActifs ? ' avec ces filtres' : ''}</p>
  {#if resultat.elements.length}
    <div class="liste">
      {#each resultat.elements as m (m.id)}
        <a class="ligne" href="/mails/{m.id}">
          <div class="principal">
            <div>{m.objet || '(sans objet)'}</div>
            <span class="discret">{m.expediteur || 'Non renseigné'} · <span class="date">{quand(m.date_mail)}</span> · {m.boites.join(', ')}</span>
          </div>
          <div class="droite">
            {#if m.nb_pieces}<span class="discret">{m.nb_pieces} pièce{m.nb_pieces > 1 ? 's' : ''} jointe{m.nb_pieces > 1 ? 's' : ''}</span>{/if}
            {#if m.categorie}<span class="etiquette {classeCategorie[m.categorie]}">{libelleCategorie[m.categorie]}</span>{/if}
            {#if m.statut_reponse}<span class="etiquette {m.statut_reponse === 'a_repondre' ? 'att' : m.statut_reponse === 'repondu' ? 'ok' : ''}">{cap(libelleStatutReponse[m.statut_reponse])}</span>{/if}
          </div>
        </a>
      {/each}
    </div>
  {:else}
    <div class="vide">
      <strong>Aucun mail trouvé</strong>
      <p>{filtresActifs ? 'Aucun mail ne correspond à ces filtres.' : 'Aucun mail n\'a encore été relevé. Ils apparaîtront ici après la prochaine relève des boîtes mail.'}</p>
      {#if filtresActifs}<div class="actions"><button class="second" onclick={toutEffacer}>Effacer les filtres</button></div>{/if}
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
