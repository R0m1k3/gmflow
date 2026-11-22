<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Fournisseur, LigneFournisseur, Page, SocieteVisible, StatutFournisseur } from '#lib/types.ts';

  type Form = {
    id: number | null;
    nom: string;
    siret: string;
    tva: string;
    domaines: string;
    statut: StatutFournisseur;
    societe_defaut_id: string;
    format_bl: string;
    notes: string;
    cree_le: string | null;
  };
  const vide = (): Form => ({ id: null, nom: '', siret: '', tva: '', domaines: '', statut: 'valide', societe_defaut_id: '', format_bl: '', notes: '', cree_le: null });

  let resultat = $state<Page<LigneFournisseur> | null>(null);
  let societes = $state<SocieteVisible[]>([]);
  let filtres = $state({ q: '', statut: '', page: 1 });
  let f = $state<Form | null>(null);
  let cible = $state('');
  let erreur = $state('');
  let info = $state('');
  let chargement = $state(true);
  let admin = $derived(session.moi?.role === 'admin');
  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);

  async function charger() {
    const q = new URLSearchParams();
    if (filtres.q.trim()) q.set('q', filtres.q.trim());
    if (filtres.statut) q.set('statut', filtres.statut);
    if (filtres.page > 1) q.set('page', String(filtres.page));
    resultat = await api.get<Page<LigneFournisseur>>(`/api/fournisseurs${q.size ? `?${q}` : ''}`);
  }
  async function ouvrir(id: number) {
    erreur = '';
    info = '';
    const v = await api.get<Fournisseur>(`/api/fournisseurs/${id}`);
    f = {
      id: v.id,
      nom: v.nom,
      siret: v.siret ?? '',
      tva: v.tva ?? '',
      domaines: v.domaines.join('\n'),
      statut: v.statut,
      societe_defaut_id: v.societe_defaut_id === null ? '' : String(v.societe_defaut_id),
      format_bl: v.format_bl ?? '',
      notes: v.notes ?? '',
      cree_le: v.cree_le
    };
    cible = '';
    window.scrollTo({ top: 0 });
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e))).finally(() => (chargement = false));
  });
  $effect(() => {
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    const fiche = page.url.searchParams.get('fiche');
    if (fiche) ouvrir(Number(fiche)).catch((e) => (erreur = message(e)));
  });

  function filtrerStatut(st: string) {
    filtres.statut = st;
    filtres.page = 1;
  }
  function chercher(e: SubmitEvent) {
    e.preventDefault();
    filtres.page = 1;
    charger().catch((err) => (erreur = message(err)));
  }

  function corps(x: Form, statut = x.statut) {
    return {
      nom: x.nom,
      siret: x.siret,
      tva: x.tva,
      domaines: x.domaines.split(/[\n,;]/).map((d) => d.trim()).filter(Boolean),
      statut,
      societe_defaut_id: x.societe_defaut_id === '' ? null : Number(x.societe_defaut_id),
      format_bl: x.format_bl,
      notes: x.notes
    };
  }
  async function enregistrer(e: SubmitEvent, statut?: StatutFournisseur) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    info = '';
    try {
      const v = f.id
        ? await api.put<Fournisseur>(`/api/admin/fournisseurs/${f.id}`, corps(f, statut))
        : await api.post<Fournisseur>('/api/admin/fournisseurs', corps(f, statut));
      await ouvrir(v.id);
      info = statut === 'valide' ? 'Fiche validée ; les factures de ce fournisseur ont repassé les contrôles.' : 'Fiche enregistrée ; les factures de ce fournisseur ont repassé les contrôles.';
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }
  async function valider() {
    if (!f) return;
    await enregistrer(new SubmitEvent('submit'), 'valide');
  }
  async function fusionner() {
    if (!f?.id || !cible) return;
    const nomCible = resultat?.elements.find((l) => l.id === Number(cible))?.nom ?? cible;
    if (!confirm(`Fusionner « ${f.nom} » dans « ${nomCible} » ? Ses documents, identifiants et domaines passent sur cette fiche.`)) return;
    erreur = '';
    try {
      const r = await api.post<{ documents: number }>(`/api/admin/fournisseurs/${f.id}/fusionner`, { cible_id: Number(cible) });
      await ouvrir(Number(cible));
      info = `Fiches fusionnées : ${r.documents} document${r.documents > 1 ? 's' : ''} déplacé${r.documents > 1 ? 's' : ''}.`;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }

  const s = (n: number, un: string, plusieurs: string) => (n > 1 ? plusieurs : un);
  const filtreActif = $derived(filtres.q.trim() !== '' || filtres.statut !== '');
  const statuts = [
    { cle: '', texte: 'Tous' },
    { cle: 'a_confirmer', texte: 'À confirmer' },
    { cle: 'valide', texte: 'Validés' }
  ];
</script>

<div class="entete">
  <div>
    <h1>Fournisseurs</h1>
    <p class="sous">Le référentiel se remplit au fil des factures. Une facture ne part seule vers Dext ou Yooz qu’une fois la fiche de son fournisseur validée.</p>
  </div>
  {#if admin && !f}
    <div class="actions"><button onclick={() => { f = vide(); info = ''; window.scrollTo({ top: 0 }); }}>Nouvelle fiche</button></div>
  {/if}
</div>

{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}

{#if f}
  <form class="carte fiche" onsubmit={(e) => enregistrer(e)}>
    <div class="actions tete">
      <h2>{f.id ? f.nom : 'Nouvelle fiche'}</h2>
      {#if f.id}<span class="etiquette {f.statut === 'valide' ? 'ok' : 'att'}">{f.statut === 'valide' ? 'Validée' : 'À confirmer'}</span>{/if}
      {#if f.cree_le}<span class="discret">créée le {quand(f.cree_le)}</span>{/if}
      {#if f.id}<a class="vers" href="/documents?fournisseur={f.id}">Voir ses documents</a>{/if}
    </div>
    {#if f.id && f.statut === 'a_confirmer' && admin}
      <div class="bandeau attention">Ce fournisseur a été repéré dans une facture. Vérifiez son nom, son SIRET et sa société, puis validez la fiche pour autoriser le dépôt automatique.</div>
    {/if}
    {#if !admin}<div class="bandeau info">Seul un administrateur peut modifier une fiche.</div>{/if}
    <fieldset disabled={!admin}>
      <div class="champs">
        <div><label for="fn">Nom</label><input id="fn" bind:value={f.nom} required /></div>
        <div><label for="fs">SIRET</label><input id="fs" bind:value={f.siret} /></div>
        <div><label for="ft">N° de TVA intracommunautaire</label><input id="ft" bind:value={f.tva} /></div>
        <div>
          <label for="fst">Statut</label>
          <select id="fst" bind:value={f.statut}><option value="a_confirmer">À confirmer</option><option value="valide">Validée</option></select>
        </div>
        <div>
          <label for="fsd">Société par défaut</label>
          <select id="fsd" bind:value={f.societe_defaut_id}>
            <option value="">Aucune</option>
            {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
        </div>
        <div><label for="fb">Format des bons de livraison</label><input id="fb" bind:value={f.format_bl} placeholder="ex. Excel, colonnes réf. / qté" /></div>
      </div>
      <div>
        <label for="fd">Domaines mail (un par ligne)</label>
        <textarea id="fd" rows="2" bind:value={f.domaines} placeholder="durand.fr"></textarea>
        <p class="aide">Appris tout seul quand une facture est reconnue par son SIRET ou son n° de TVA.</p>
      </div>
      <div><label for="fno">Notes</label><textarea id="fno" rows="2" bind:value={f.notes}></textarea></div>
      {#if admin}
        <div class="actions">
          {#if f.id && f.statut === 'a_confirmer'}<button type="button" onclick={valider}>Valider la fiche</button>{/if}
          <button class={f.id && f.statut === 'a_confirmer' ? 'second' : ''}>Enregistrer</button>
          <button type="button" class="discret" onclick={() => (f = null)}>Fermer</button>
        </div>
      {/if}
    </fieldset>
    {#if !admin}<div class="actions"><button type="button" class="second" onclick={() => (f = null)}>Fermer</button></div>{/if}
    {#if admin && f.id && resultat}
      <details class="fusion">
        <summary>Cette fiche est un doublon d’une autre ?</summary>
        <div class="actions">
          <div class="choix">
            <label for="fc">Fusionner dans la fiche à garder</label>
            <select id="fc" bind:value={cible}>
              <option value="">Choisir la fiche à garder</option>
              {#each resultat.elements.filter((l) => l.id !== f?.id) as l (l.id)}<option value={String(l.id)}>{l.nom}</option>{/each}
            </select>
          </div>
          <button type="button" class="second" disabled={!cible} onclick={fusionner}>Fusionner</button>
        </div>
        <p class="aide">Les documents, identifiants et domaines de cette fiche passent sur la fiche gardée.</p>
      </details>
    {/if}
  </form>
{/if}

<form class="recherche" onsubmit={chercher} role="search">
  <label for="q">Rechercher un fournisseur</label>
  <input id="q" type="search" bind:value={filtres.q} placeholder="Nom, SIRET, TVA ou domaine mail" />
</form>

<div class="onglets" role="tablist" aria-label="Filtrer par statut">
  {#each statuts as st (st.cle)}
    <button role="tab" aria-selected={filtres.statut === st.cle} class:actif={filtres.statut === st.cle} onclick={() => filtrerStatut(st.cle)}>
      {st.texte}{#if filtres.statut === st.cle && resultat}<span class="n">{resultat.total}</span>{/if}
    </button>
  {/each}
</div>

{#if chargement && !resultat}
  <div class="charge" aria-label="Chargement"><span></span></div>
{:else if resultat}
  {#if resultat.elements.length}
    <ul class="liste">
      {#each resultat.elements as l (l.id)}
        <li>
          <div class="principal">
            <span>{l.nom}</span>
            <span class="discret">
              {l.siret ? `SIRET ${l.siret}` : 'SIRET non renseigné'}{l.tva ? ` · TVA ${l.tva}` : ''}
            </span>
            <span class="discret">{l.domaines.join(', ') || 'Aucun domaine mail'} · {l.societe_defaut ?? 'Société non renseignée'}</span>
          </div>
          <div class="droite">
            <span class="discret nb">{l.nb_documents} {s(l.nb_documents, 'document', 'documents')}{#if l.a_verifier}<br /><span class="rouge">{l.a_verifier} à vérifier</span>{/if}</span>
            {#if l.statut === 'a_confirmer'}<span class="etiquette att">À confirmer</span>{:else}<span class="etiquette ok">Validé</span>{/if}
            <button class={l.statut === 'a_confirmer' ? '' : 'second'} onclick={() => ouvrir(l.id).catch((e) => (erreur = message(e)))}>
              {l.statut === 'a_confirmer' ? 'Vérifier' : 'Modifier'}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="vide">
      <strong>{filtreActif ? 'Aucun fournisseur ne correspond' : 'Aucun fournisseur pour l’instant'}</strong>
      <p>{filtreActif ? 'Essayez un autre nom ou retirez le filtre.' : 'Les fournisseurs apparaissent ici dès qu’une facture est lue.'}</p>
      {#if filtreActif}<div class="actions"><button class="second" onclick={() => { filtres.q = ''; filtrerStatut(''); }}>Effacer la recherche</button></div>{/if}
    </div>
  {/if}
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={filtres.page <= 1} onclick={() => { filtres.page--; }}>Précédent</button>
      <span>Page {filtres.page} sur {pages}</span>
      <button class="second" disabled={filtres.page >= pages} onclick={() => { filtres.page++; }}>Suivant</button>
    </div>
  {/if}
{/if}

<style>
  .fiche { margin-bottom: var(--e6); display: grid; gap: var(--e4); }
  .fiche h2 { margin: 0; }
  .tete .vers { margin-left: auto; }
  fieldset { border: none; padding: 0; margin: 0; display: grid; gap: var(--e4); }
  .fusion { border-top: 1px solid var(--trait); padding-top: var(--e3); }
  .fusion summary { cursor: pointer; color: var(--accent); min-height: var(--cible); display: flex; align-items: center; }
  .fusion .actions { align-items: flex-end; margin-top: var(--e2); }
  .choix { flex: 1; min-width: 220px; }
  .recherche { margin-bottom: var(--e4); max-width: 480px; }
  .nb { text-align: right; }
</style>
