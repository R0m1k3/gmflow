<script lang="ts">
    import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { aideEtat, classeEtat, classeStatutDepot, libelleEtat, libelleStatutDepot, libelleType, libelleVoie, STATUTS_DEPOT, TYPES } from '#lib/documents.ts';
  import { dateSimple, montant } from '#lib/format.ts';
  import type { Bilan, EtatDocument, LigneDocument, Page, SocieteVisible, Voie } from '#lib/types.ts';

  let societes = $state<SocieteVisible[]>([]);
  let resultat = $state<Page<LigneDocument> | null>(null);
  let bilan = $state<Bilan | null>(null);
  let erreur = $state('');

  const p = page.url.searchParams;
  // Sans filtre dans l'adresse, on montre d'abord ce qui attend une action ; « vue=tous » garde la vue complète.
  const parDefaut = page.url.search === '';
  let filtres = $state({
    societe: p.get('societe') ?? '',
    type: p.get('type') ?? '',
    etat: p.get('etat') ?? (parDefaut ? 'lu' : ''),
    voie: p.get('voie') ?? '',
    verifie: p.get('verifie') ?? (parDefaut ? 'non' : ''),
    statut_depot: p.get('statut_depot') ?? '',
    fournisseur: p.get('fournisseur') ?? '',
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
      history.replaceState(history.state, '', `/documents?${q || 'vue=tous'}`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    charger();
  });
  const onglets = [
    { cle: 'averifier', libelle: 'À vérifier', etat: 'lu', verifie: 'non', compte: null },
    { cle: 'tous', libelle: 'Tous', etat: '', verifie: '', compte: null },
    { cle: 'alire', libelle: 'À lire par Claude', etat: 'a_lire', verifie: '', compte: ['a_lire', 'en_cours'] },
    { cle: 'erreur', libelle: 'Non lus', etat: 'erreur', verifie: '', compte: ['erreur'] },
    { cle: 'verifies', libelle: 'Vérifiés', etat: 'lu', verifie: 'oui', compte: null },
    { cle: 'ignore', libelle: 'Sans intérêt', etat: 'ignore', verifie: '', compte: ['ignore'] }
  ];
  function nombreOnglet(o: (typeof onglets)[number]): number | null {
    if (resultat && filtres.etat === o.etat && filtres.verifie === o.verifie) return resultat.total;
    return o.compte && bilan ? o.compte.reduce((a, c) => a + compte(bilan!.par_etat, c), 0) : null;
  }
  function choisir(o: (typeof onglets)[number]) {
    filtres.etat = o.etat;
    filtres.verifie = o.verifie;
    filtres.page = 1;
    charger();
  }
  function reinitialiser() {
    filtres.societe = filtres.type = filtres.voie = filtres.statut_depot = filtres.fournisseur = filtres.du = filtres.au = filtres.q = '';
    filtres.etat = 'lu';
    filtres.verifie = 'non';
    filtres.page = 1;
    charger();
  }
  let filtresAvances = $derived(
    [filtres.societe, filtres.type, filtres.voie, filtres.statut_depot, filtres.fournisseur, filtres.du, filtres.au].filter((v) => v !== '').length
  );
  const vueVide = $derived(!filtres.q && !filtresAvances);
  const s = (n: number, un: string, plusieurs: string) => (n > 1 ? plusieurs : un);

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

<div class="entete">
  <div>
    <h1>Documents</h1>
    <p class="sous">Les factures, avoirs et bons de livraison lus par Claude. Ouvrez un document pour comparer ce qu’il a lu avec le fichier d’origine, puis validez-le.</p>
  </div>
</div>

<nav class="onglets" aria-label="Filtrer les documents">
  {#each onglets as o (o.cle)}
    {@const n = nombreOnglet(o)}
    <button type="button" class:actif={filtres.etat === o.etat && filtres.verifie === o.verifie} aria-current={filtres.etat === o.etat && filtres.verifie === o.verifie ? 'page' : undefined} onclick={() => choisir(o)}>
      {o.libelle}{#if n !== null}<span class="n">{n}</span>{/if}
    </button>
  {/each}
</nav>

<form class="carte filtres" onsubmit={chercher}>
  <div class="barre">
    <div class="recherche">
      <label for="q">Fournisseur, numéro ou nom de fichier</label>
      <input id="q" type="search" bind:value={filtres.q} placeholder="Par exemple : Pointcarré, F2026-0412" />
    </div>
    <button>Chercher</button>
  </div>
  <details class="repli" open={filtresAvances > 0}>
    <summary>Plus de filtres{#if filtresAvances} ({filtresAvances}){/if}</summary>
    <div class="champs">
      <div>
        <label for="s">Société</label>
        <select id="s" bind:value={filtres.societe}>
          <option value="">Toutes</option>
          {#each societes as so (so.id)}<option value={String(so.id)}>{so.raison_sociale}</option>{/each}
        </select>
      </div>
      <div>
        <label for="t">Type de document</label>
        <select id="t" bind:value={filtres.type}>
          <option value="">Tous</option>
          {#each TYPES as t (t)}<option value={t}>{libelleType[t]}</option>{/each}
        </select>
      </div>
      <div>
        <label for="e">Lecture par Claude</label>
        <select id="e" bind:value={filtres.etat}>
          <option value="">Tous sauf sans intérêt</option>
          {#each etats as e (e)}<option value={e}>{libelleEtat[e]}</option>{/each}
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
      <div>
        <label for="v">Type de fichier lu</label>
        <select id="v" bind:value={filtres.voie}>
          <option value="">Tous</option>
          {#each voies as v (v)}<option value={v}>{libelleVoie[v]}</option>{/each}
        </select>
      </div>
      <div>
        <label for="sd">Dépôt chez Dext ou Yooz</label>
        <select id="sd" bind:value={filtres.statut_depot}>
          <option value="">Tous</option>
          {#each STATUTS_DEPOT as st (st)}<option value={st}>{libelleStatutDepot[st]}</option>{/each}
        </select>
      </div>
      <div><label for="du">Reçus à partir du</label><input id="du" type="date" bind:value={filtres.du} /></div>
      <div><label for="au">Reçus jusqu’au</label><input id="au" type="date" bind:value={filtres.au} /></div>
    </div>
    <div class="actions">
      <button>Appliquer</button>
      <button type="button" class="discret" onclick={reinitialiser}>Revenir à « À vérifier »</button>
    </div>
  </details>
</form>

{#if erreur}
  <div class="bandeau erreur">
    {erreur}
    <button type="button" class="second petit" onclick={charger}>Réessayer</button>
  </div>
{/if}
{#if !resultat && !erreur}<div class="charge" aria-label="Chargement des documents"><span></span></div>{/if}
{#if resultat}
  <p class="discret" aria-live="polite">{resultat.total} {s(resultat.total, 'document', 'documents')}</p>
  {#if resultat.elements.length}
    <ul class="liste">
      {#each resultat.elements as d (d.id)}
        {@const aVerifier = d.etat === 'lu' && !d.verifie}
        <li>
          <div class="principal">
            <a class="titre-doc" href="/documents/{d.id}">{d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non renseigné'}</a>
            <span class="discret">
              {d.type ? libelleType[d.type] : 'Type non renseigné'}{d.numero ? ` n° ${d.numero}` : ''} · <span class="date">{dateSimple(d.date_document ?? d.date_mail?.slice(0, 10))}</span>{d.societe ? ` · ${d.societe}` : ''}
            </span>
            <span class="discret fichier">{d.nom_fichier}{d.voie ? ` (${libelleVoie[d.voie]})` : ''}</span>
            {#if d.message && d.etat !== 'lu'}<span class="discret" title={aideEtat[d.etat]}>{d.message}</span>{/if}
          </div>
          <div class="droite">
            <span class="nombre montant">{montant(d.montant_ttc)}{#if d.montant_ttc !== null} <span class="discret">TTC</span>{/if}</span>
            <span class="etats">
              {#if aVerifier}
                <span class="etiquette att" title={aideEtat.lu}>À vérifier</span>
              {:else if d.etat === 'lu'}
                <span class="etiquette ok" title="Vérifié à la main">Vérifié</span>
              {:else}
                <span class="etiquette {classeEtat[d.etat]}" title={aideEtat[d.etat]}>{libelleEtat[d.etat]}</span>
              {/if}
              {#if d.statut_depot}
                <span class="etiquette {classeStatutDepot[d.statut_depot]}" title={d.controles.map((c) => c.message).join('\n')}>{libelleStatutDepot[d.statut_depot]}</span>
              {/if}
            </span>
            <a class="bouton" class:second={!aVerifier} href="/documents/{d.id}">{aVerifier ? 'Vérifier' : 'Ouvrir'}</a>
          </div>
        </li>
      {/each}
    </ul>
    {#if pages > 1}
      <div class="pagination">
        <button class="second" disabled={filtres.page <= 1} onclick={() => allerPage(filtres.page - 1)}>Précédent</button>
        <span>Page {filtres.page} sur {pages}</span>
        <button class="second" disabled={filtres.page >= pages} onclick={() => allerPage(filtres.page + 1)}>Suivant</button>
      </div>
    {/if}
  {:else}
    <div class="vide">
      {#if vueVide && filtres.etat === 'lu' && filtres.verifie === 'non'}
        <strong>Aucun document à vérifier</strong>
        <p>Tout ce que Claude a lu est vérifié. Les nouveaux documents apparaîtront ici dès qu’il les aura lus.</p>
        <div class="actions"><button class="second" onclick={() => choisir(onglets[1])}>Voir tous les documents</button></div>
      {:else}
        <strong>Aucun document ne correspond</strong>
        <p>Essayez d’autres mots, élargissez les dates ou revenez à la vue complète.</p>
        <div class="actions"><button class="second" onclick={() => { reinitialiser(); choisir(onglets[1]); }}>Voir tous les documents</button></div>
      {/if}
    </div>
  {/if}
{/if}

{#if bilan}
  <details class="repli bilan">
    <summary>Bilan de la lecture : Claude lit-il juste ?</summary>
    <div class="grille">
      <div class="carte">
        <div class="discret">Lus par Claude</div>
        <div class="chiffre">{compte(bilan.par_etat, 'lu')}</div>
        <div class="discret">{compte(bilan.par_etat, 'a_lire') + compte(bilan.par_etat, 'en_cours')} à lire, {compte(bilan.par_etat, 'erreur')} non lus</div>
      </div>
      <div class="carte">
        <div class="discret">Vérifiés à la main</div>
        <div class="chiffre">{bilan.verifies}</div>
        <div class="discret">objectif : 50 documents</div>
      </div>
      <div class="carte">
        <div class="discret">Type de document juste</div>
        <div class="chiffre" class:vert={bilan.verifies > 0 && bilan.type_justes / bilan.verifies > 0.95}>{pourcent(bilan.type_justes, bilan.verifies)}</div>
        <div class="discret">des documents vérifiés (objectif : plus de 95 %)</div>
      </div>
      <div class="carte">
        <div class="discret">Société destinataire juste</div>
        <div class="chiffre" class:vert={bilan.verifies > 0 && bilan.societe_justes / bilan.verifies > 0.95}>{pourcent(bilan.societe_justes, bilan.verifies)}</div>
        <div class="discret">des documents vérifiés (objectif : plus de 95 %)</div>
      </div>
    </div>
    {#if bilan.semaines.length}
      <h3>Documents validés sans correction, semaine par semaine</h3>
      <table>
        <thead><tr><th>Semaine du</th><th class="nombre">Vérifiés ou validés</th><th class="nombre">Sans correction</th></tr></thead>
        <tbody>
          {#each bilan.semaines as se (se.semaine)}
            <tr>
              <td>{dateSimple(se.semaine)}</td>
              <td class="nombre">{se.valides}</td>
              <td class="nombre">{pourcent(se.sans_correction, se.valides)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </details>
{/if}

<style>
  .filtres { margin-bottom: var(--e4); }
  .barre { display: flex; gap: var(--e3); align-items: flex-end; }
  .recherche { flex: 1; min-width: 0; }
  .titre-doc { font-weight: 500; color: var(--encre); text-decoration: none; }
  .titre-doc:hover { color: var(--accent); text-decoration: underline; }
  .fichier { word-break: break-word; }
  .montant { min-width: 7rem; text-align: right; }
  .etats { display: flex; gap: var(--e1); flex-wrap: wrap; justify-content: flex-end; min-width: 9rem; }
  .bandeau button { margin-left: var(--e3); vertical-align: middle; }
  @media (max-width: 640px) {
    .barre { flex-wrap: wrap; }
    .barre > button { width: 100%; }
    .montant { text-align: left; min-width: 0; }
    .etats { justify-content: flex-start; min-width: 0; }
  }
</style>
