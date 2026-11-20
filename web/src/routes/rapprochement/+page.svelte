<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { libelleMethode } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { DocRap, Rapproche, SocieteVisible, VueRapprochement } from '#lib/types.ts';

  type Onglet = 'avec_ecart' | 'bl_sans_facture' | 'factures_sans_bl' | 'sans_ecart';
  const onglets: { cle: Onglet; texte: string; aide: string }[] = [
    { cle: 'avec_ecart', texte: 'Écarts', aide: 'Factures rapprochées dont le montant HT ou les quantités ne correspondent pas aux BL.' },
    { cle: 'bl_sans_facture', texte: 'BL sans facture', aide: 'Livraisons reçues pour lesquelles aucune facture n’a été rapprochée.' },
    { cle: 'factures_sans_bl', texte: 'Factures sans BL', aide: 'Factures pour lesquelles aucun bon de livraison n’a été rapproché.' },
    { cle: 'sans_ecart', texte: 'Rapprochées sans écart', aide: 'Factures dont les BL correspondent.' }
  ];

  const p = page.url.searchParams;
  let filtres = $state({
    societe: p.get('societe') ?? '',
    fournisseur: p.get('fournisseur') ?? '',
    du: p.get('du') ?? '',
    au: p.get('au') ?? ''
  });
  let onglet = $state<Onglet>((p.get('onglet') as Onglet) ?? 'avec_ecart');
  let societes = $state<SocieteVisible[]>([]);
  let vue = $state<VueRapprochement | null>(null);
  let fournisseurs = $state<{ id: number; nom: string }[]>([]);
  let erreur = $state('');
  let info = $state('');
  let aide = $derived(onglets.find((o) => o.cle === onglet)?.aide ?? '');

  $effect(() => {
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
  });

  function requete() {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(filtres)) if (v) q.set(k, v);
    return q;
  }
  async function charger() {
    erreur = '';
    const q = requete();
    try {
      vue = await api.get<VueRapprochement>(`/api/rapprochement${q.size ? `?${q}` : ''}`);
      if (!filtres.fournisseur) {
        fournisseurs = vue.resume
          .filter((r) => r.fournisseur_id !== null)
          .map((r) => ({ id: r.fournisseur_id as number, nom: r.fournisseur ?? '?' }));
      }
      q.set('onglet', onglet);
      history.replaceState(history.state, '', `/rapprochement?${q}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });
  function filtrer(e: SubmitEvent) {
    e.preventDefault();
    charger();
  }
  function voirFournisseur(id: number | null) {
    if (id === null) return;
    filtres.fournisseur = String(id);
    charger();
  }
  async function relireBl() {
    if (!vue || !confirm(`Relire ${vue.bl_sans_lignes} BL pour en extraire les lignes ? Chaque relecture compte dans le quota du jour.`)) return;
    erreur = '';
    try {
      const r = await api.post<{ n: number }>('/api/admin/rapprochement/relire-bl');
      info = `${r.n} BL remis en lecture.`;
      await charger();
    } catch (e) {
      erreur = message(e);
    }
  }
  const liste = (o: Onglet): (DocRap | Rapproche)[] => (vue ? vue[o] : []);
  const ecartClasse = (e: number | null) => (e !== null && Math.abs(e) > 0.05 ? 'ko' : 'ok');
</script>

<h1>Rapprochement BL et factures</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

<form class="carte filtres" onsubmit={filtrer}>
  <div class="champs">
    <div>
      <label for="s">Structure</label>
      <select id="s" bind:value={filtres.societe}>
        <option value="">Toutes</option>
        {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
      </select>
    </div>
    <div>
      <label for="f">Fournisseur</label>
      <select id="f" bind:value={filtres.fournisseur}>
        <option value="">Tous</option>
        {#each fournisseurs as f (f.id)}<option value={String(f.id)}>{f.nom}</option>{/each}
      </select>
    </div>
    <div><label for="du">Du</label><input id="du" type="date" bind:value={filtres.du} /></div>
    <div><label for="au">Au</label><input id="au" type="date" bind:value={filtres.au} /></div>
  </div>
  <button>Filtrer</button>
</form>

{#if vue}
  {#if vue.tronque}<div class="bandeau attention">Trop de documents : seuls les 5 000 plus récents sont examinés. Réduisez la période.</div>{/if}
  {#if vue.bl_sans_lignes > 0}
    <div class="bandeau attention actions">
      <span>{vue.bl_sans_lignes} BL ont été lus sans leurs lignes (avant ce module) : les quantités ne peuvent pas être comparées.</span>
      {#if session.moi?.role === 'admin'}<button class="second" onclick={relireBl}>Relire ces BL</button>{/if}
    </div>
  {/if}

  <h2>Par fournisseur</h2>
  <table>
    <thead>
      <tr><th>Fournisseur</th><th class="nombre">BL</th><th class="nombre">Factures</th><th class="nombre">BL sans facture</th><th class="nombre">Factures sans BL</th><th class="nombre">Factures avec écart</th></tr>
    </thead>
    <tbody>
      {#each vue.resume as r (`${r.fournisseur_id}-${r.fournisseur}`)}
        <tr class:cliquable={r.fournisseur_id !== null} onclick={() => voirFournisseur(r.fournisseur_id)}>
          <td>{r.fournisseur ?? 'fournisseur non identifié'}</td>
          <td class="nombre">{r.bls}</td>
          <td class="nombre">{r.factures}</td>
          <td class="nombre" class:alerte={r.bl_sans_facture > 0}>{r.bl_sans_facture}</td>
          <td class="nombre" class:alerte={r.factures_sans_bl > 0}>{r.factures_sans_bl}</td>
          <td class="nombre" class:alerte={r.factures_avec_ecart > 0}>{r.factures_avec_ecart}</td>
        </tr>
      {:else}
        <tr><td colspan="6" class="discret">Aucun BL ni facture lus sur la période.</td></tr>
      {/each}
    </tbody>
  </table>

  <div class="onglets" role="tablist">
    {#each onglets as o (o.cle)}
      <button role="tab" aria-selected={onglet === o.cle} class:choisi={onglet === o.cle} onclick={() => (onglet = o.cle)}>
        {o.texte} <span class="discret">{liste(o.cle).length}</span>
      </button>
    {/each}
  </div>
  <p class="discret">{aide}</p>

  {#if onglet === 'bl_sans_facture' || onglet === 'factures_sans_bl'}
    <table>
      <thead><tr><th>Date</th><th>Numéro</th><th>Commande</th><th>Fournisseur</th><th>Structure</th><th class="nombre">HT</th></tr></thead>
      <tbody>
        {#each liste(onglet) as d ((d as DocRap).id)}
          {@const doc = d as DocRap}
          <tr>
            <td class="date">{dateSimple(doc.date_document)}</td>
            <td><a href="/documents/{doc.id}">{doc.numero ?? doc.nom_fichier}</a></td>
            <td>{doc.numero_commande ?? '—'}</td>
            <td>{doc.fournisseur ?? '—'}</td>
            <td>{doc.societe ?? '—'}</td>
            <td class="nombre">{montant(doc.montant_ht)}</td>
          </tr>
        {:else}
          <tr><td colspan="6" class="discret">Rien à signaler.</td></tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <table>
      <thead><tr><th>Facture</th><th>Fournisseur</th><th>BL rapprochés</th><th class="nombre">Écart HT</th><th>Quantités</th></tr></thead>
      <tbody>
        {#each liste(onglet) as x ((x as Rapproche).facture.id)}
          {@const r = x as Rapproche}
          <tr>
            <td>
              <a href="/documents/{r.facture.id}">{r.facture.numero ?? r.facture.nom_fichier}</a>
              <div class="discret">{dateSimple(r.facture.date_document)} · {montant(r.facture.montant_ht)} HT</div>
            </td>
            <td>{r.facture.fournisseur ?? '—'}<div class="discret">{r.facture.societe ?? ''}</div></td>
            <td>
              {#each r.bls as b (b.lien_id)}
                <div><a href="/documents/{b.id}">{b.numero ?? `BL ${b.id}`}</a> <span class="discret">{libelleMethode[b.methode]}</span></div>
              {/each}
            </td>
            <td class="nombre">
              {#if r.ecart_montant === null}<span class="discret">non calculable</span>
              {:else}<span class="etiquette {ecartClasse(r.ecart_montant)}">{montant(r.ecart_montant)}</span>{/if}
            </td>
            <td>
              {#each r.ecarts.slice(0, 4) as e, i (i)}
                <div class="discret">{e.reference ?? e.designation} : livré {e.qte_bl.toLocaleString('fr-FR')}, facturé {e.qte_facture.toLocaleString('fr-FR')}</div>
              {:else}
                <span class="discret">—</span>
              {/each}
              {#if r.ecarts.length > 4}<div class="discret">et {r.ecarts.length - 4} autre(s)</div>{/if}
            </td>
          </tr>
        {:else}
          <tr><td colspan="5" class="discret">Rien à signaler.</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
{/if}

<style>
  .filtres { margin-bottom: 1rem; }
  .onglets { display: flex; gap: 0.3rem; flex-wrap: wrap; border-bottom: 1px solid var(--trait); margin: 1.5rem 0 0.6rem; }
  .onglets button { background: none; color: var(--encre); border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.45rem 0.8rem; }
  .onglets button.choisi { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
  td.alerte { color: var(--alerte); font-weight: 600; }
  .bandeau.actions { justify-content: space-between; }
</style>
