<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { classeStatutDepot, libelleStatutDepot, libelleType } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import type { LigneDocument, LigneSuivi, Page, StatutDepot } from '#lib/types.ts';

  const onglets: { statut: StatutDepot; texte: string; aide: string }[] = [
    { statut: 'pret', texte: 'Prêtes', aide: 'Tous les contrôles sont bons. Déposez-les chez Dext ou Yooz.' },
    { statut: 'a_verifier', texte: 'À vérifier', aide: 'Un contrôle a échoué : ouvrez le document pour le corriger ou le valider tel quel.' },
    { statut: 'erreur', texte: 'Refusées', aide: 'Le dépôt a échoué ou a été refusé par Dext ou Yooz. Ouvrez le document pour voir pourquoi.' },
    { statut: 'doublon', texte: 'Déjà déposées', aide: 'Même fichier, ou même fournisseur, numéro et montant qu’un document déjà reçu : elles ne partent pas une deuxième fois.' },
    { statut: 'depose', texte: 'Déposées', aide: 'Envoyées à l’adresse de dépôt de la société.' }
  ];

  const p = page.url.searchParams;
  let onglet = $state<StatutDepot>((p.get('onglet') as StatutDepot) ?? 'pret');
  let numero = $state(1);
  let resultat = $state<Page<LigneDocument> | null>(null);
  let erreur = $state('');
  let info = $state('');
  let selection = $state<number[]>([]);
  let enCours = $state(false);
  let chargement = $state(true);

  const maintenant = new Date();
  let mois = $state(`${maintenant.getFullYear()}-${String(maintenant.getMonth() + 1).padStart(2, '0')}`);
  let suivi = $state<LigneSuivi[]>([]);

  let pages = $derived(resultat ? Math.max(1, Math.ceil(resultat.total / 50)) : 1);
  let aide = $derived(onglets.find((o) => o.statut === onglet)?.aide ?? '');

  async function charger() {
    erreur = '';
    chargement = true;
    try {
      const q = new URLSearchParams({ statut_depot: onglet, etat: 'lu' });
      if (numero > 1) q.set('page', String(numero));
      resultat = await api.get<Page<LigneDocument>>(`/api/documents?${q}`);
      history.replaceState(history.state, '', `/depots?onglet=${onglet}`);
    } catch (e) {
      erreur = message(e);
    } finally {
      chargement = false;
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
    selection = [];
    info = '';
    charger();
  }

  async function deposer(ev: MouseEvent, d: LigneDocument) {
    ev.stopPropagation();
    erreur = '';
    info = '';
    try {
      await api.post(`/api/documents/${d.id}/deposer`, {});
      info = `Déposée : ${d.numero ?? d.nom_fichier}. Le dépôt est lancé.`;
      selection = selection.filter((i) => i !== d.id);
      await charger();
      await chargerSuivi();
    } catch (e) {
      erreur = message(e);
    }
  }

  let prets = $derived(onglet === 'pret' && resultat ? resultat.elements.filter((d) => d.statut_depot === 'pret') : []);
  let toutCoche = $derived(prets.length > 0 && prets.every((d) => selection.includes(d.id)));
  function basculerTout() {
    selection = toutCoche ? [] : prets.map((d) => d.id);
  }
  function basculer(id: number) {
    selection = selection.includes(id) ? selection.filter((i) => i !== id) : [...selection, id];
  }
  async function deposerSelection() {
    if (!selection.length || enCours) return;
    enCours = true;
    erreur = '';
    info = '';
    const ids = [...selection];
    let faits = 0;
    const echecs: string[] = [];
    for (const id of ids) {
      try {
        await api.post(`/api/documents/${id}/deposer`, {});
        faits++;
      } catch (e) {
        const d = resultat?.elements.find((x) => x.id === id);
        echecs.push(`${d?.numero ?? d?.nom_fichier ?? id} : ${message(e)}`);
      }
    }
    selection = [];
    if (faits) info = `Déposées : ${faits} ${faits > 1 ? 'factures' : 'facture'}. Les dépôts sont lancés.`;
    if (echecs.length) erreur = `${echecs.length} dépôt${echecs.length > 1 ? 's' : ''} refusé${echecs.length > 1 ? 's' : ''}. ${echecs.join(' ; ')}`;
    enCours = false;
    await charger();
    await chargerSuivi();
  }

  const moisLibelle = $derived(
    new Date(`${mois}-01T00:00:00Z`).toLocaleDateString('fr-FR', { timeZone: 'UTC', month: 'long', year: 'numeric' })
  );
  const s = (n: number, un: string, plusieurs: string) => (n > 1 ? plusieurs : un);
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

<div class="entete">
  <div>
    <h1>Dépôts chez Dext et Yooz</h1>
    <p class="sous">Les factures et avoirs lus par Claude partent d’ici vers votre outil de comptabilité. Déposez celles qui sont prêtes.</p>
  </div>
</div>

{#if erreur}
  <div class="bandeau erreur">
    {erreur}
    <button class="discret petit" onclick={() => { charger(); chargerSuivi(); }}>Réessayer</button>
  </div>
{/if}
{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}

<p class="resume" aria-live="polite">
  <strong>{moisLibelle}</strong> :
  {total.attente} {s(total.attente, 'facture attend', 'factures attendent')} un dépôt,
  {total.deposees} {s(total.deposees, 'déposée', 'déposées')}{#if total.bloquees > 0},
    <span class="rouge">{total.bloquees} {s(total.bloquees, 'bloquée', 'bloquées')}</span>{/if}.
</p>

<div class="onglets" role="tablist" aria-label="Filtrer les dépôts">
  {#each onglets as o (o.statut)}
    <button role="tab" aria-selected={onglet === o.statut} class:actif={onglet === o.statut} onclick={() => choisir(o.statut)}>
      {o.texte}{#if onglet === o.statut && resultat}<span class="n">{resultat.total}</span>{/if}
    </button>
  {/each}
</div>
<p class="discret">{aide}</p>

{#if onglet === 'pret' && prets.length}
  <div class="actions groupe">
    <label class="case"><input type="checkbox" checked={toutCoche} onchange={basculerTout} /> Tout sélectionner</label>
    <button disabled={!selection.length || enCours} onclick={deposerSelection}>
      {enCours ? 'Dépôt en cours…' : selection.length ? `Déposer la sélection (${selection.length})` : 'Déposer la sélection'}
    </button>
  </div>
{/if}

{#if chargement && !resultat}
  <div class="charge" aria-label="Chargement"><span></span></div>
{:else if resultat}
  {#if resultat.elements.length}
    <ul class="liste">
      {#each resultat.elements as d (d.id)}
        <li>
          {#if onglet === 'pret'}
            <input type="checkbox" aria-label="Sélectionner {d.numero ?? d.nom_fichier}" checked={selection.includes(d.id)} onchange={() => basculer(d.id)} />
          {/if}
          <div class="principal">
            <a href="/documents/{d.id}">{d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non renseigné'} · {d.type ? libelleType[d.type] : 'Document'} {d.numero ?? ''}</a>
            <span class="discret">
              {dateSimple(d.date_document ?? d.date_mail?.slice(0, 10))} · {d.societe ?? 'Société non renseignée'}
            </span>
            {#if d.controles.length}
              <ul class="controles">
                {#each d.controles as c (c.code)}<li>{c.message}</li>{/each}
              </ul>
            {/if}
          </div>
          <div class="droite">
            <span class="nombre montant">{montant(d.montant_ttc)}</span>
            {#if d.statut_depot}<span class="etiquette {classeStatutDepot[d.statut_depot]}">{libelleStatutDepot[d.statut_depot]}</span>{/if}
            {#if d.statut_depot === 'pret'}
              <button onclick={(ev) => deposer(ev, d)} disabled={enCours}>Déposer</button>
            {:else}
              <a class="bouton second" href="/documents/{d.id}">{d.statut_depot === 'a_verifier' ? 'Vérifier' : 'Ouvrir'}</a>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="vide">
      <strong>{onglet === 'pret' ? 'Rien à déposer pour l’instant' : 'Aucune facture dans cette liste'}</strong>
      <p>{onglet === 'pret' ? 'Les factures lues et contrôlées apparaîtront ici, prêtes à partir chez Dext ou Yooz.' : 'Tout est en ordre de ce côté.'}</p>
      {#if onglet !== 'pret'}<div class="actions"><button class="second" onclick={() => choisir('pret')}>Voir les prêtes</button></div>{/if}
    </div>
  {/if}
  {#if pages > 1}
    <div class="pagination">
      <button class="second" disabled={numero <= 1} onclick={() => { numero--; selection = []; charger(); }}>Précédent</button>
      <span>Page {numero} sur {pages}</span>
      <button class="second" disabled={numero >= pages} onclick={() => { numero++; selection = []; charger(); }}>Suivant</button>
    </div>
  {/if}
{/if}

<div class="section">
  <div class="titre"><h2>Suivi du mois</h2></div>
  <div class="actions mois">
    <label for="mois">Mois</label>
    <input id="mois" type="month" bind:value={mois} onchange={chargerSuivi} />
  </div>
  {#if suivi.length}
    <table>
      <thead>
        <tr><th>Société</th><th class="nombre">Reçues</th><th class="nombre">Déposées</th><th class="nombre">Prêtes</th><th class="nombre">À vérifier</th><th class="nombre">Refusées</th><th class="nombre">Déjà déposées</th><th class="nombre">Montant déposé</th></tr>
      </thead>
      <tbody>
        {#each suivi as l (l.societe_id)}
          <tr>
            <td>{l.societe ?? 'Société non reconnue'}</td>
            <td class="nombre">{l.recues}</td>
            <td class="nombre">{l.deposees}</td>
            <td class="nombre">{l.pretes}</td>
            <td class="nombre" class:rouge={l.a_verifier > 0}>{l.a_verifier}</td>
            <td class="nombre" class:rouge={l.en_erreur > 0}>{l.en_erreur}</td>
            <td class="nombre">{l.doublons}</td>
            <td class="nombre">{montant(l.montant_depose)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="discret">Factures et avoirs du mois, hors doublons.</p>
  {:else}
    <div class="vide"><strong>Aucune facture ce mois-ci</strong><p>Choisissez un autre mois ou attendez la prochaine relève des boîtes mail.</p></div>
  {/if}
</div>

<style>
  .resume { margin: 0 0 var(--e4); font-size: var(--t-moyen); }
  .controles { margin: var(--e1) 0 0; padding-left: var(--e4); font-size: var(--t-petit); color: var(--attention); }
  .montant { min-width: 90px; text-align: right; }
  .groupe { justify-content: space-between; margin-bottom: var(--e3); }
  .groupe label { margin: 0; }
  .mois { margin-bottom: var(--e3); }
  .mois label { margin: 0; }
  .mois input { width: auto; }
</style>
