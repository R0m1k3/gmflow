<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { classeStatutDepot, libelleStatutDepot, libelleType } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import type { LigneDocument, LigneSuivi, Page, StatutDepot } from '#lib/types.ts';

  const onglets: { statut: StatutDepot; texte: string; aide: string }[] = [
    { statut: 'a_verifier', texte: 'À vérifier', aide: 'Un contrôle a échoué : corrigez le document ou validez-le tel quel.' },
    { statut: 'pret', texte: 'Prêts', aide: 'Tous les contrôles passent. Sans dépôt automatique pour la structure, ils attendent votre validation.' },
    { statut: 'erreur', texte: 'En erreur', aide: "L'envoi a échoué ou a été interrompu. Un administrateur peut redéposer." },
    { statut: 'doublon', texte: 'Doublons', aide: 'Même fichier, ou même fournisseur, numéro et montant qu’un document déjà reçu. Bloqués.' },
    { statut: 'depose', texte: 'Déposés', aide: 'Envoyés à l’adresse de dépôt de la structure.' }
  ];

  const p = page.url.searchParams;
  let onglet = $state<StatutDepot>((p.get('onglet') as StatutDepot) ?? 'a_verifier');
  let numero = $state(1);
  let resultat = $state<Page<LigneDocument> | null>(null);
  let erreur = $state('');
  let info = $state('');

  const maintenant = new Date();
  let mois = $state(`${maintenant.getFullYear()}-${String(maintenant.getMonth() + 1).padStart(2, '0')}`);
  let suivi = $state<LigneSuivi[]>([]);

  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);
  let aide = $derived(onglets.find((o) => o.statut === onglet)?.aide ?? '');

  async function charger() {
    erreur = '';
    try {
      const q = new URLSearchParams({ statut_depot: onglet, etat: 'lu' });
      if (numero > 1) q.set('page', String(numero));
      resultat = await api.get<Page<LigneDocument>>(`/api/documents?${q}`);
      history.replaceState(history.state, '', `/depots?onglet=${onglet}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  async function chargerSuivi() {
    try {
      suivi = (await api.get<{ lignes: LigneSuivi[] }>(`/api/depots/suivi?mois=${mois}`)).lignes;
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });
  $effect(() => {
    chargerSuivi();
  });

  function choisir(s: StatutDepot) {
    onglet = s;
    numero = 1;
    charger();
  }

  async function deposer(ev: MouseEvent, d: LigneDocument) {
    ev.stopPropagation();
    erreur = '';
    info = '';
    try {
      await api.post(`/api/documents/${d.id}/deposer`, {});
      info = `Dépôt de ${d.numero ?? d.nom_fichier} lancé.`;
      await charger();
      await chargerSuivi();
    } catch (e) {
      erreur = message(e);
    }
  }

  let total = $derived(
    suivi.reduce(
      (t, l) => ({
        recues: t.recues + l.recues,
        deposees: t.deposees + l.deposees,
        attente: t.attente + l.pretes + l.a_verifier,
        bloquees: t.bloquees + l.en_erreur + l.doublons,
        montant: t.montant + l.montant_depose
      }),
      { recues: 0, deposees: 0, attente: 0, bloquees: 0, montant: 0 }
    )
  );
</script>

<h1>Dépôts Dext / Yooz</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

<div class="onglets" role="tablist">
  {#each onglets as o (o.statut)}
    <button role="tab" aria-selected={onglet === o.statut} class:choisi={onglet === o.statut} onclick={() => choisir(o.statut)}>{o.texte}</button>
  {/each}
</div>
<p class="discret">{aide}</p>

{#if resultat}
  <table>
    <thead>
      <tr><th>Date</th><th>Type</th><th>Structure</th><th>Fournisseur</th><th>Numéro</th><th>TTC</th><th>Contrôles</th><th></th></tr>
    </thead>
    <tbody>
      {#each resultat.elements as d (d.id)}
        <tr class="cliquable" onclick={() => goto(`/documents/${d.id}`)}>
          <td class="date">{dateSimple(d.date_document ?? d.date_mail?.slice(0, 10))}</td>
          <td>{d.type ? libelleType[d.type] : '—'}</td>
          <td>{d.societe ?? '—'}</td>
          <td>{d.fournisseur ?? d.fournisseur_nom ?? '—'}</td>
          <td>{d.numero ?? '—'}</td>
          <td class="nombre">{montant(d.montant_ttc)}</td>
          <td>
            {#if d.controles.length}
              <ul class="controles">
                {#each d.controles as c (c.code)}<li>{c.message}</li>{/each}
              </ul>
            {:else if d.statut_depot}
              <span class="etiquette {classeStatutDepot[d.statut_depot]}">{libelleStatutDepot[d.statut_depot]}</span>
            {/if}
          </td>
          <td>
            {#if d.statut_depot === 'pret'}
              <button onclick={(ev) => deposer(ev, d)}>Déposer</button>
            {/if}
          </td>
        </tr>
      {:else}
        <tr><td colspan="8" class="discret">Aucun document.</td></tr>
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

<h2>Suivi du mois</h2>
<div class="carte">
  <div class="actions">
    <label for="mois">Mois</label>
    <input id="mois" type="month" bind:value={mois} onchange={chargerSuivi} />
    <span class="discret">
      Factures et avoirs du mois (hors doublons) : {total.recues}, déposés {total.deposees}, en attente {total.attente}, bloqués {total.bloquees}.
    </span>
  </div>
  <table>
    <thead>
      <tr><th>Structure</th><th>Reçus</th><th>Déposés</th><th>Prêts</th><th>À vérifier</th><th>En erreur</th><th>Doublons</th><th>Montant déposé</th></tr>
    </thead>
    <tbody>
      {#each suivi as l (l.societe_id)}
        <tr>
          <td>{l.societe ?? 'Sans structure reconnue'}</td>
          <td class="nombre">{l.recues}</td>
          <td class="nombre">{l.deposees}</td>
          <td class="nombre">{l.pretes}</td>
          <td class="nombre" class:rouge={l.a_verifier > 0}>{l.a_verifier}</td>
          <td class="nombre" class:rouge={l.en_erreur > 0}>{l.en_erreur}</td>
          <td class="nombre">{l.doublons}</td>
          <td class="nombre">{montant(l.montant_depose)}</td>
        </tr>
      {:else}
        <tr><td colspan="8" class="discret">Aucune facture ce mois-ci.</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .onglets { display: flex; gap: 0.3rem; flex-wrap: wrap; border-bottom: 1px solid var(--trait); margin-bottom: 0.6rem; }
  .onglets button { background: none; color: var(--encre); border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 0.45rem 0.8rem; }
  .onglets button.choisi { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
  .controles { margin: 0; padding-left: 1rem; font-size: 0.88rem; }
  .rouge { color: var(--alerte); font-weight: 600; }
  .actions { margin-bottom: 0.6rem; flex-wrap: wrap; }
  .actions label { margin: 0; }
  .actions input { width: auto; }
</style>
