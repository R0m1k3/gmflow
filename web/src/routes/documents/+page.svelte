<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { classeEtat, libelleEtat, libelleType, libelleVoie, TYPES } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import type { Bilan, EtatDocument, LigneDocument, Page, SocieteVisible, Voie } from '#lib/types.ts';

  let societes = $state<SocieteVisible[]>([]);
  let resultat = $state<Page<LigneDocument> | null>(null);
  let bilan = $state<Bilan | null>(null);
  let erreur = $state('');

  const p = page.url.searchParams;
  let filtres = $state({
    societe: p.get('societe') ?? '',
    type: p.get('type') ?? '',
    etat: p.get('etat') ?? '',
    voie: p.get('voie') ?? '',
    verifie: p.get('verifie') ?? '',
    du: p.get('du') ?? '',
    au: p.get('au') ?? '',
    q: p.get('q') ?? '',
    page: Number(p.get('page') ?? '1')
  });
  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);

  const etats = Object.keys(libelleEtat) as EtatDocument[];
  const voies = Object.keys(libelleVoie) as Voie[];

  $effect(() => {
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    api.get<Bilan>('/api/documents/bilan').then((v) => (bilan = v));
  });

  function requete() {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filtres)) if (v !== '' && !(k === 'page' && v === 1)) q.set(k, String(v));
    return q.toString();
  }
  async function charger() {
    erreur = '';
    const q = requete();
    try {
      resultat = await api.get<Page<LigneDocument>>(`/api/documents${q ? '?' + q : ''}`);
      history.replaceState(history.state, '', `/documents${q ? '?' + q : ''}`);
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

  function compte(liste: { cle: string | null; n: number }[], cle: string) {
    return liste.find((c) => c.cle === cle)?.n ?? 0;
  }
  function pourcent(n: number, sur: number) {
    return sur ? `${((100 * n) / sur).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} %` : '—';
  }
</script>

<h1>Documents</h1>

{#if bilan}
  <div class="grille bilan">
    <div class="carte stat">
      <div class="discret">Lus</div>
      <div class="valeur nombre">{compte(bilan.par_etat, 'lu')}</div>
      <div class="discret">
        {compte(bilan.par_etat, 'a_lire') + compte(bilan.par_etat, 'en_cours')} à lire, {compte(bilan.par_etat, 'erreur')} en erreur
      </div>
    </div>
    <div class="carte stat">
      <div class="discret">Vérifiés à la main</div>
      <div class="valeur nombre">{bilan.verifies}</div>
      <div class="discret">objectif : 50 documents</div>
    </div>
    <div class="carte stat">
      <div class="discret">Type juste</div>
      <div class="valeur nombre" class:bon={bilan.verifies > 0 && bilan.type_justes / bilan.verifies > 0.95}>{pourcent(bilan.type_justes, bilan.verifies)}</div>
      <div class="discret">des documents vérifiés (objectif : plus de 95 %)</div>
    </div>
    <div class="carte stat">
      <div class="discret">Structure juste</div>
      <div class="valeur nombre" class:bon={bilan.verifies > 0 && bilan.societe_justes / bilan.verifies > 0.95}>{pourcent(bilan.societe_justes, bilan.verifies)}</div>
      <div class="discret">des documents vérifiés (objectif : plus de 95 %)</div>
    </div>
  </div>
{/if}

<form class="carte filtres" onsubmit={chercher}>
  <div class="champs">
    <div>
      <label for="s">Structure</label>
      <select id="s" bind:value={filtres.societe}>
        <option value="">Toutes</option>
        {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
      </select>
    </div>
    <div>
      <label for="t">Type</label>
      <select id="t" bind:value={filtres.type}>
        <option value="">Tous</option>
        {#each TYPES as t (t)}<option value={t}>{libelleType[t]}</option>{/each}
      </select>
    </div>
    <div>
      <label for="e">État</label>
      <select id="e" bind:value={filtres.etat}>
        <option value="">Tous sauf ignorés</option>
        {#each etats as e (e)}<option value={e}>{libelleEtat[e]}</option>{/each}
      </select>
    </div>
    <div>
      <label for="v">Voie de lecture</label>
      <select id="v" bind:value={filtres.voie}>
        <option value="">Toutes</option>
        {#each voies as v (v)}<option value={v}>{libelleVoie[v]}</option>{/each}
      </select>
    </div>
    <div>
      <label for="vf">Vérification</label>
      <select id="vf" bind:value={filtres.verifie}>
        <option value="">Tous</option>
        <option value="non">À vérifier</option>
        <option value="oui">Vérifiés</option>
      </select>
    </div>
    <div><label for="du">Du</label><input id="du" type="date" bind:value={filtres.du} /></div>
    <div><label for="au">Au</label><input id="au" type="date" bind:value={filtres.au} /></div>
    <div><label for="q">Fournisseur, numéro ou objet</label><input id="q" type="search" bind:value={filtres.q} /></div>
  </div>
  <button>Filtrer</button>
</form>

{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if resultat}
  <p class="discret">{resultat.total} document{resultat.total > 1 ? 's' : ''}</p>
  <table>
    <thead>
      <tr><th>Date</th><th>Type</th><th>Structure</th><th>Fournisseur</th><th>Numéro</th><th>TTC</th><th>État</th><th>Fichier</th></tr>
    </thead>
    <tbody>
      {#each resultat.elements as d (d.id)}
        <tr class="cliquable" onclick={() => goto(`/documents/${d.id}`)}>
          <td class="date">{dateSimple(d.date_document ?? d.date_mail?.slice(0, 10))}</td>
          <td>{d.type ? libelleType[d.type] : '—'}</td>
          <td>{d.societe ?? '—'}</td>
          <td>{d.fournisseur_nom ?? '—'}</td>
          <td>{d.numero ?? '—'}</td>
          <td class="nombre">{montant(d.montant_ttc)}</td>
          <td>
            <span class="etiquette {classeEtat[d.etat]}" title={d.message ?? ''}>{libelleEtat[d.etat]}</span>
            {#if d.verifie}<span class="etiquette ok" title="Vérifié à la main">✓</span>{/if}
          </td>
          <td class="discret fichier"><a href="/documents/{d.id}">{d.nom_fichier}</a>{#if d.voie}<br />{libelleVoie[d.voie]}{/if}</td>
        </tr>
      {:else}
        <tr><td colspan="8" class="discret">Aucun document.</td></tr>
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
  .filtres, .bilan { margin-bottom: 1rem; }
  .stat { display: grid; gap: 0.2rem; }
  .valeur { font-size: 1.5rem; font-weight: 650; }
  .bon { color: var(--accent); }
  .fichier { word-break: break-word; max-width: 16rem; }
</style>
