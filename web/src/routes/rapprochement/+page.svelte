<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { libelleMethode } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { DocRap, Rapproche, SocieteVisible, VueRapprochement } from '#lib/types.ts';

  type Onglet = 'avec_ecart' | 'bl_sans_facture' | 'factures_sans_bl' | 'sans_ecart';
  const onglets: { cle: Onglet; texte: string; aide: string }[] = [
    { cle: 'avec_ecart', texte: 'Écarts à vérifier', aide: 'Le montant ou les quantités de la facture ne correspondent pas aux bons de livraison reliés. Ouvrez la facture pour trancher.' },
    { cle: 'factures_sans_bl', texte: 'Factures sans bon de livraison', aide: 'Aucun bon de livraison n’a été relié à ces factures : vérifiez que la marchandise a bien été reçue.' },
    { cle: 'bl_sans_facture', texte: 'Bons de livraison sans facture', aide: 'Marchandise reçue, facture pas encore arrivée : à réclamer au fournisseur si elle tarde.' },
    { cle: 'sans_ecart', texte: 'Tout correspond', aide: 'Factures dont les bons de livraison correspondent : rien à faire.' }
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
  let chargement = $state(true);
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
    } finally {
      chargement = false;
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
    if (!vue || !confirm(`Relire ${vue.bl_sans_lignes} BL pour en extraire les lignes ? Chaque relecture compte dans la limite du jour.`)) return;
    erreur = '';
    try {
      const r = await api.post<{ n: number }>('/api/admin/rapprochement/relire-bl');
      info = `${r.n} bon${r.n > 1 ? 's' : ''} de livraison remis en lecture.`;
      await charger();
    } catch (e) {
      erreur = message(e);
    }
  }
  const liste = (o: Onglet): (DocRap | Rapproche)[] => (vue ? vue[o] : []);
  let nbEcarts = $derived(vue ? vue.avec_ecart.length : 0);
  let nbSansBl = $derived(vue ? vue.factures_sans_bl.length : 0);
  let nbSansFacture = $derived(vue ? vue.bl_sans_facture.length : 0);
  let nbOk = $derived(vue ? vue.sans_ecart.length : 0);
  const s = (n: number, un: string, plusieurs: string) => (n > 1 ? plusieurs : un);
  const ecartClasse = (e: number | null) => (e !== null && Math.abs(e) > 0.05 ? 'ko' : 'ok');
</script>

<div class="entete">
  <div>
    <h1>Factures et bons de livraison</h1>
    <p class="sous">Chaque facture est reliée aux bons de livraison qui la justifient. Contrôlez les écarts avant de payer.</p>
  </div>
</div>

{#if erreur}
  <div class="bandeau erreur">{erreur} <button class="discret petit" onclick={charger}>Réessayer</button></div>
{/if}
{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}

<form class="carte filtres" onsubmit={filtrer}>
  <div class="champs">
    <div>
      <label for="s">Société</label>
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
  <div class="actions"><button class="second">Filtrer</button></div>
</form>

{#if chargement && !vue}
  <div class="charge" aria-label="Chargement"><span></span></div>
{/if}

{#if vue}
  {#if vue.tronque}<div class="bandeau attention">Trop de documents : seuls les 5 000 plus récents sont examinés. Réduisez la période.</div>{/if}
  {#if vue.bl_sans_lignes > 0}
    <div class="bandeau attention rangee">
      <span>{vue.bl_sans_lignes} {s(vue.bl_sans_lignes, 'bon de livraison a été lu', 'bons de livraison ont été lus')} sans ses lignes : les quantités ne peuvent pas être comparées.</span>
      {#if session.moi?.role === 'admin'}<button class="second petit" onclick={relireBl}>Relire ces bons de livraison</button>{/if}
    </div>
  {/if}

  <p class="resume" aria-live="polite">
    {#if nbEcarts + nbSansBl + nbSansFacture === 0}
      {nbOk} {s(nbOk, 'facture correspond', 'factures correspondent')} à ses bons de livraison. Rien à contrôler.
    {:else}
      <span class:rouge={nbEcarts > 0}>{nbEcarts} {s(nbEcarts, 'écart', 'écarts')}</span>,
      {nbSansBl} {s(nbSansBl, 'facture sans bon de livraison', 'factures sans bon de livraison')},
      {nbSansFacture} {s(nbSansFacture, 'bon de livraison sans facture', 'bons de livraison sans facture')} ;
      {nbOk} {s(nbOk, 'facture correspond', 'factures correspondent')}.
    {/if}
  </p>

  <div class="onglets" role="tablist" aria-label="Filtrer par situation">
    {#each onglets as o (o.cle)}
      <button role="tab" aria-selected={onglet === o.cle} class:actif={onglet === o.cle} onclick={() => (onglet = o.cle)}>
        {o.texte} <span class="n">{liste(o.cle).length}</span>
      </button>
    {/each}
  </div>
  <p class="discret">{aide}</p>

  {#if onglet === 'bl_sans_facture' || onglet === 'factures_sans_bl'}
    {#if liste(onglet).length}
      <ul class="liste">
        {#each liste(onglet) as d ((d as DocRap).id)}
          {@const doc = d as DocRap}
          <li>
            <div class="principal">
              <a href="/documents/{doc.id}">{doc.fournisseur ?? 'Fournisseur non renseigné'} · {doc.numero ?? doc.nom_fichier}</a>
              <span class="discret">
                {dateSimple(doc.date_document)} · {doc.societe ?? 'Société non renseignée'}{#if doc.numero_commande} · commande {doc.numero_commande}{/if}
              </span>
            </div>
            <div class="droite">
              <span class="nombre">{montant(doc.montant_ht)} HT</span>
              <a class="bouton second" href="/documents/{doc.id}">Ouvrir</a>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="vide"><strong>Rien à signaler</strong><p>{onglet === 'factures_sans_bl' ? 'Toutes les factures ont un bon de livraison.' : 'Tous les bons de livraison ont leur facture.'}</p></div>
    {/if}
  {:else if liste(onglet).length}
    <ul class="liste">
      {#each liste(onglet) as x ((x as Rapproche).facture.id)}
        {@const r = x as Rapproche}
        <li class="rap">
          <div class="principal">
            <a href="/documents/{r.facture.id}">{r.facture.fournisseur ?? 'Fournisseur non renseigné'} · facture {r.facture.numero ?? r.facture.nom_fichier}</a>
            <span class="discret">{dateSimple(r.facture.date_document)} · {montant(r.facture.montant_ht)} HT · {r.facture.societe ?? 'Société non renseignée'}</span>
            <div class="liens">
              <span class="fleche" aria-hidden="true">↔</span>
              <span>
                {#each r.bls as b (b.lien_id)}
                  <span class="bl"><a href="/documents/{b.id}">BL {b.numero ?? b.id}</a> <span class="discret">({libelleMethode[b.methode]})</span></span>
                {/each}
              </span>
            </div>
            {#if r.ecarts.length}
              <ul class="qtes">
                {#each r.ecarts.slice(0, 4) as e, i (i)}
                  <li>{e.reference ?? e.designation ?? 'Article'} : {e.qte_bl.toLocaleString('fr-FR')} livré{e.qte_bl > 1 ? 's' : ''}, {e.qte_facture.toLocaleString('fr-FR')} facturé{e.qte_facture > 1 ? 's' : ''}</li>
                {/each}
                {#if r.ecarts.length > 4}<li>et {r.ecarts.length - 4} autre{r.ecarts.length - 4 > 1 ? 's' : ''} article{r.ecarts.length - 4 > 1 ? 's' : ''}</li>{/if}
              </ul>
            {/if}
          </div>
          <div class="droite">
            <div class="ecart">
              {#if r.ecart_montant === null}
                <span class="discret">Écart non calculable</span>
              {:else}
                <span class="etiquette {ecartClasse(r.ecart_montant)}">{ecartClasse(r.ecart_montant) === 'ko' ? 'Écart' : 'Conforme'} {montant(r.ecart_montant)}</span>
              {/if}
            </div>
            <a class="bouton {onglet === 'avec_ecart' ? '' : 'second'}" href="/documents/{r.facture.id}">{onglet === 'avec_ecart' ? 'Vérifier' : 'Ouvrir'}</a>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="vide"><strong>{onglet === 'avec_ecart' ? 'Aucun écart' : 'Aucune facture ici'}</strong><p>{onglet === 'avec_ecart' ? 'Les montants et les quantités correspondent aux bons de livraison sur cette période.' : 'Aucune facture reliée sans écart sur cette période.'}</p></div>
  {/if}

  <div class="section">
    <div class="titre"><h2>Par fournisseur</h2></div>
    {#if vue.resume.length}
      <p class="discret">Cliquez sur un fournisseur pour ne voir que lui.</p>
      <table>
        <thead>
          <tr><th>Fournisseur</th><th class="nombre">Bons de livraison</th><th class="nombre">Factures</th><th class="nombre">BL sans facture</th><th class="nombre">Factures sans BL</th><th class="nombre">Avec écart</th></tr>
        </thead>
        <tbody>
          {#each vue.resume as r (`${r.fournisseur_id}-${r.fournisseur}`)}
            <tr class:cliquable={r.fournisseur_id !== null} onclick={() => voirFournisseur(r.fournisseur_id)}>
              <td>{r.fournisseur ?? 'Fournisseur non identifié'}</td>
              <td class="nombre">{r.bls}</td>
              <td class="nombre">{r.factures}</td>
              <td class="nombre" class:alerte={r.bl_sans_facture > 0}>{r.bl_sans_facture}</td>
              <td class="nombre" class:alerte={r.factures_sans_bl > 0}>{r.factures_sans_bl}</td>
              <td class="nombre" class:alerte={r.factures_avec_ecart > 0}>{r.factures_avec_ecart}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="vide"><strong>Aucun bon de livraison ni facture</strong><p>Rien n’a été lu sur cette période. Élargissez les dates ci-dessus.</p></div>
    {/if}
  </div>
{/if}

<style>
  .filtres { margin-bottom: var(--e4); }
  .resume { margin: 0 0 var(--e4); font-size: var(--t-moyen); }
  td.alerte { color: var(--alerte); font-weight: 600; }
  .bandeau.rangee { display: flex; align-items: center; justify-content: space-between; gap: var(--e3); flex-wrap: wrap; }
  .liens { display: flex; gap: var(--e2); margin-top: var(--e1); font-weight: 400; }
  .fleche { color: var(--gris); }
  .bl { display: inline-block; margin-right: var(--e3); }
  .qtes { margin: var(--e1) 0 0; padding-left: var(--e4); font-size: var(--t-petit); color: var(--attention); }
  .ecart { text-align: right; }
</style>
