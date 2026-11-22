<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { quand, taille } from '#lib/format.ts';
  import type { Tableau } from '#lib/types.ts';

  let t = $state<Tableau | null>(null);
  let erreur = $state('');
  $effect(() => {
    api.get<Tableau>('/api/tableau-de-bord').then((v) => (t = v)).catch((e) => (erreur = message(e)));
  });

  const libelleClaude: Record<string, string> = { valide: 'Connecté', erreur: 'Connexion en erreur', absent: 'Pas connecté', en_test: 'Test en cours' };
  const jour = new Date().toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' });
  const s = (n: number, un: string, plusieurs: string) => (n > 1 ? plusieurs : un);

  type Tache = { n: number; texte: string; href: string; action: string; urgent?: boolean };
  let taches = $derived.by<Tache[]>(() => {
    if (!t) return [];
    const l: Tache[] = [];
    const d = t.depots;
    const doc = t.documents;
    if (d.bloques) l.push({ n: d.bloques, texte: `${s(d.bloques, 'facture n’a', 'factures n’ont')} pas pu être déposée${s(d.bloques, '', 's')} (erreur ou doublon)`, href: '/depots?onglet=erreur', action: 'Voir', urgent: true });
    if (t.a_repondre) l.push({ n: t.a_repondre, texte: `${s(t.a_repondre, 'mail attend', 'mails attendent')} une réponse`, href: '/a-repondre', action: 'Répondre' });
    if (doc.a_verifier) l.push({ n: doc.a_verifier, texte: `${s(doc.a_verifier, 'document lu par Claude attend', 'documents lus par Claude attendent')} votre vérification`, href: '/documents?verifie=non&etat=lu', action: 'Vérifier' });
    if (d.prets) l.push({ n: d.prets, texte: `${s(d.prets, 'facture est prête', 'factures sont prêtes')} à être déposée${s(d.prets, '', 's')} chez Dext ou Yooz`, href: '/depots', action: 'Déposer' });
    if (doc.erreur) l.push({ n: doc.erreur, texte: `${s(doc.erreur, 'document n’a', 'documents n’ont')} pas pu être lu${s(doc.erreur, '', 's')}`, href: '/documents?etat=erreur', action: 'Voir', urgent: true });
    return l;
  });
  let totalMails = $derived(t ? t.mails_du_jour.reduce((a, x) => a + x.n, 0) : 0);
  let techAlerte = $derived(
    !!t &&
      (t.claude?.etat === 'erreur' || t.claude?.etat === 'absent' ||
        (t.lecture !== undefined && t.lecture.appels_du_jour >= t.lecture.quota) ||
        (t.tri !== undefined && (t.tri.appels_du_jour >= t.tri.quota || t.tri.en_erreur > 0)) ||
        (t.stockage?.libre_pourcent != null && t.stockage.libre_pourcent < 10))
  );
</script>

<div class="entete">
  <div>
    <h1>Aujourd’hui</h1>
    <p class="sous">{jour}</p>
  </div>
</div>

{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if !t && !erreur}<div class="charge" aria-label="Chargement"><span></span></div>{/if}
{#if t}
  <section aria-labelledby="afaire">
    <div class="titre-bloc"><h2 id="afaire">À faire maintenant</h2></div>
    {#each t.boites_alerte as b (b.id)}
      <div class="bandeau {b.motif === 'pause_auto' ? 'erreur' : 'attention'}">
        <strong>La boîte {b.libelle}</strong>
        {b.motif === 'pause_auto' ? 'est en pause : trois connexions ont été refusées.' : 'n’a pas pu être relevée.'}
        {#if b.depuis}Depuis {quand(b.depuis)}.{/if}
        {#if b.message}<span class="detail">{b.message}</span>{/if}
        {#if t.claude}<a href="/admin/boites/{b.id}">Voir la boîte</a>{/if}
      </div>
    {/each}
    {#if taches.length}
      <ul class="liste taches">
        {#each taches as k (k.href + k.texte)}
          <li class:urgent={k.urgent}>
            <span class="gros nombre">{k.n}</span>
            <span class="principal">{k.texte}</span>
            <span class="droite"><a class="bouton {k.urgent ? 'second' : ''}" href={k.href}>{k.action}</a></span>
          </li>
        {/each}
      </ul>
    {:else if t.boites_alerte.length === 0}
      <div class="vide"><strong>Rien à traiter</strong><p>Tout est à jour : aucune réponse, vérification ni dépôt n’attend.</p></div>
    {/if}
  </section>

  <section class="section" aria-labelledby="activite">
    <div class="titre"><h2 id="activite">Activité du jour</h2></div>
    <p class="resume">
      <span class="nombre">{totalMails}</span> {s(totalMails, 'mail reçu', 'mails reçus')} ·
      <span class="nombre">{t.documents.lus_aujourdhui}</span> {s(t.documents.lus_aujourdhui, 'document lu', 'documents lus')} ·
      <span class="nombre">{t.depots.deposes_mois}</span> {s(t.depots.deposes_mois, 'facture déposée', 'factures déposées')} ce mois-ci
    </p>
    {#if t.mails_du_jour.length}
      <table>
        <thead><tr><th>Société</th><th class="nombre">Mails reçus aujourd’hui</th></tr></thead>
        <tbody>
          {#each t.mails_du_jour as x (x.societe_id)}
            <tr>
              <td><a href="/mails?societe={x.societe_id}">{x.raison_sociale}</a></td>
              <td class="nombre">{x.n}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="vide">
        <strong>Aucune société</strong>
        <p>Les mails apparaissent dès qu’une société et une boîte mail sont créées.</p>
        {#if t.claude}<div class="actions"><a class="bouton" href="/admin/structures">Créer la première société</a></div>{/if}
      </div>
    {/if}
  </section>

  {#if t.claude || t.stockage || t.lecture || t.tri}
    <details class="tech section">
      <summary>
        <span>État technique</span>
        {#if techAlerte}<span class="etiquette ko">À regarder</span>{/if}
      </summary>
      <dl class="kv">
        {#if t.claude}
          <dt>Connexion à Claude</dt>
          <dd>
            <span class="etiquette {t.claude.etat === 'valide' ? 'ok' : t.claude.etat === 'erreur' ? 'ko' : 'att'}">{libelleClaude[t.claude.etat] ?? t.claude.etat}</span>
            {#if t.claude.teste_le}<span class="discret"> testée {quand(t.claude.teste_le)}</span>{/if}
            <a class="lien-tech" href="/admin/claude">Réglages</a>
          </dd>
        {/if}
        {#if t.lecture}
          <dt>Lecture des documents</dt>
          <dd>
            <span class="nombre" class:rouge={t.lecture.appels_du_jour >= t.lecture.quota}>{t.lecture.appels_du_jour} sur {t.lecture.quota}</span> appels aujourd’hui
            <span class="discret"> · lecture automatique {t.lecture.auto ? 'activée' : 'coupée'}{t.documents.a_lire ? `, ${t.documents.a_lire} en attente` : ''}</span>
          </dd>
        {/if}
        {#if t.tri}
          <dt>Tri des mails</dt>
          <dd>
            <span class="nombre" class:rouge={t.tri.appels_du_jour >= t.tri.quota}>{t.tri.appels_du_jour} sur {t.tri.quota}</span> appels aujourd’hui
            <span class="discret"> · tri automatique {t.tri.auto ? 'activé' : 'coupé'}{t.tri.en_attente ? `, ${t.tri.en_attente} en attente` : ''}</span>
            {#if t.tri.en_erreur}<span class="rouge"> · {t.tri.en_erreur} en erreur</span>{/if}
          </dd>
        {/if}
        {#if t.stockage}
          <dt>Stockage</dt>
          <dd>
            <span class="nombre">{taille(t.stockage.octets)}</span> utilisés
            <span class="discret" class:rouge={t.stockage.libre_pourcent !== null && t.stockage.libre_pourcent < 10}>
              · {t.stockage.libre_pourcent === null ? 'place libre inconnue' : `${t.stockage.libre_pourcent.toLocaleString('fr-FR')} % de place libre`}
            </span>
          </dd>
        {/if}
      </dl>
    </details>
  {/if}
{/if}

<style>
  .titre-bloc { margin-bottom: var(--e3); }
  .titre-bloc h2 { margin: 0; font-size: 1.25rem; }
  .detail { display: block; font-size: var(--t-petit); margin-top: var(--e1); }
  .taches li { min-height: 64px; border-left: 3px solid var(--attention); }
  .taches li.urgent { border-left-color: var(--alerte); }
  .taches .gros { font-family: var(--serif); font-weight: 600; font-size: 1.75rem; line-height: 1; min-width: 48px; text-align: right; }
  .taches li.urgent .gros { color: var(--alerte); }
  .taches .principal { font-weight: 400; font-size: var(--t-moyen); }
  .resume { color: var(--gris); margin-bottom: var(--e4); }
  .resume .nombre { color: var(--encre); font-weight: 600; }
  .tech summary { cursor: pointer; display: flex; align-items: center; gap: var(--e3); min-height: var(--cible); border-bottom: 1px solid var(--trait-fort); color: var(--gris); font-family: var(--serif); font-weight: 600; font-size: var(--t-moyen); list-style: none; }
  .tech summary::-webkit-details-marker { display: none; }
  .tech summary::after { content: 'Afficher'; margin-left: auto; font-family: var(--sans); font-weight: 400; font-size: var(--t-petit); }
  .tech[open] summary::after { content: 'Masquer'; }
  .tech summary:hover { color: var(--encre); }
  .tech .kv { border-top: 0; }
  .lien-tech { margin-left: var(--e3); font-size: var(--t-petit); }
  @media (max-width: 640px) {
    .taches .gros { min-width: 36px; font-size: 1.5rem; }
    .taches li { flex-wrap: nowrap; }
    .taches .droite { width: auto; margin-left: auto; }
  }
</style>
