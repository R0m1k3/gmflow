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
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    const fiche = page.url.searchParams.get('fiche');
    if (fiche) ouvrir(Number(fiche)).catch((e) => (erreur = message(e)));
  });

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
      info = 'Fiche enregistrée ; les factures de ce fournisseur ont repassé les contrôles.';
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
</script>

<h1>Fournisseurs</h1>
<p class="discret">
  Le référentiel se remplit au fil des factures : chaque nouveau fournisseur arrive « à confirmer ». Une facture ne part
  seule vers Dext ou Yooz qu'une fois sa fiche validée.
</p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if f}
  <form class="carte fiche" onsubmit={(e) => enregistrer(e)}>
    <div class="actions">
      <h2>{f.id ? f.nom : 'Nouvelle fiche'}</h2>
      {#if f.id}<span class="etiquette {f.statut === 'valide' ? 'ok' : 'att'}">{f.statut === 'valide' ? 'validée' : 'à confirmer'}</span>{/if}
      {#if f.cree_le}<span class="discret">créée le {quand(f.cree_le)}</span>{/if}
      <span style="flex:1"></span>
      {#if f.id}<a href="/documents?fournisseur={f.id}">Ses documents</a>{/if}
    </div>
    <fieldset disabled={!admin}>
      <div class="champs">
        <div><label for="fn">Nom</label><input id="fn" bind:value={f.nom} required /></div>
        <div><label for="fs">SIRET</label><input id="fs" bind:value={f.siret} /></div>
        <div><label for="ft">N° TVA intracommunautaire</label><input id="ft" bind:value={f.tva} /></div>
        <div>
          <label for="fst">Statut</label>
          <select id="fst" bind:value={f.statut}><option value="a_confirmer">À confirmer</option><option value="valide">Validée</option></select>
        </div>
        <div>
          <label for="fsd">Structure par défaut</label>
          <select id="fsd" bind:value={f.societe_defaut_id}>
            <option value="">Aucune</option>
            {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
        </div>
        <div><label for="fb">Format des BL</label><input id="fb" bind:value={f.format_bl} placeholder="ex. Excel, colonnes réf. / qté" /></div>
      </div>
      <p>
        <label for="fd">Domaines mail (un par ligne)</label>
        <textarea id="fd" rows="2" bind:value={f.domaines} placeholder="durand.fr"></textarea>
        <span class="discret">Appris tout seul quand une facture est reconnue par son SIRET ou son n° de TVA.</span>
      </p>
      <p><label for="fno">Notes</label><textarea id="fno" rows="2" bind:value={f.notes}></textarea></p>
      {#if admin}
        <div class="actions">
          <button>Enregistrer</button>
          {#if f.id && f.statut === 'a_confirmer'}<button type="button" onclick={valider}>Valider la fiche</button>{/if}
          <button type="button" class="second" onclick={() => (f = null)}>Fermer</button>
        </div>
      {/if}
    </fieldset>
    {#if !admin}<div class="actions"><button type="button" class="second" onclick={() => (f = null)}>Fermer</button></div>{/if}
    {#if admin && f.id && resultat}
      <div class="actions fusion">
        <label for="fc">Doublon d'une autre fiche ? Fusionner dans</label>
        <select id="fc" bind:value={cible}>
          <option value="">choisir la fiche à garder</option>
          {#each resultat.elements.filter((l) => l.id !== f?.id) as l (l.id)}<option value={String(l.id)}>{l.nom}</option>{/each}
        </select>
        <button type="button" class="second" disabled={!cible} onclick={fusionner}>Fusionner</button>
      </div>
    {/if}
  </form>
{:else if admin}
  <p><button onclick={() => { f = vide(); info = ''; }}>Nouvelle fiche</button></p>
{/if}

<form class="carte filtres" onsubmit={chercher}>
  <div class="champs">
    <div><label for="q">Nom, SIRET, TVA ou domaine</label><input id="q" type="search" bind:value={filtres.q} /></div>
    <div>
      <label for="st">Statut</label>
      <select id="st" bind:value={filtres.statut}><option value="">Tous</option><option value="a_confirmer">À confirmer</option><option value="valide">Validées</option></select>
    </div>
  </div>
  <button>Filtrer</button>
</form>

{#if resultat}
  <table>
    <thead><tr><th>Nom</th><th>SIRET</th><th>TVA</th><th>Domaines</th><th>Structure par défaut</th><th>Documents</th><th></th></tr></thead>
    <tbody>
      {#each resultat.elements as l (l.id)}
        <tr class="cliquable" onclick={() => ouvrir(l.id).catch((e) => (erreur = message(e)))}>
          <td>{l.nom} {#if l.statut === 'a_confirmer'}<span class="etiquette att">à confirmer</span>{/if}</td>
          <td class="nombre">{l.siret ?? '—'}</td>
          <td>{l.tva ?? '—'}</td>
          <td>{l.domaines.join(', ') || '—'}</td>
          <td>{l.societe_defaut ?? '—'}</td>
          <td class="nombre">{l.nb_documents}{#if l.a_verifier}<br /><span class="discret">{l.a_verifier} à vérifier</span>{/if}</td>
          <td><button class="second">Ouvrir</button></td>
        </tr>
      {:else}
        <tr><td colspan="7" class="discret">Aucune fiche.</td></tr>
      {/each}
    </tbody>
  </table>
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={filtres.page <= 1} onclick={() => { filtres.page--; charger(); }}>Précédent</button>
      <span class="discret">Page {filtres.page} sur {pages}</span>
      <button class="second" disabled={filtres.page >= pages} onclick={() => { filtres.page++; charger(); }}>Suivant</button>
    </div>
  {/if}
{/if}

<style>
  .fiche, .filtres { margin-bottom: 1rem; display: grid; gap: 0.6rem; }
  .fiche h2 { margin: 0; }
  fieldset { border: none; padding: 0; margin: 0; display: grid; gap: 0.6rem; }
  .fusion { border-top: 1px solid var(--trait); padding-top: 0.7rem; flex-wrap: nowrap; }
  .fusion label { margin: 0; white-space: nowrap; }
  .fusion select { width: auto; flex: 1; }
  .filtres > button { justify-self: start; }
</style>
