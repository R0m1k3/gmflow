<script lang="ts">
  import { page } from '$app/state';
  import { api, message, sonder } from '#lib/api.ts';
  import { aideEtat, libelleMethode, libelleUnite, SEUIL_ECART } from '#lib/documents.ts';
  import { classeEtat, classeStatutDepot, libelleEtat, libelleOutil, libelleStatutDepot, libelleType, libelleVoie, TYPES } from '#lib/documents.ts';
  import { dateSimple, montant, quand } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { DetailRapprochement } from '#lib/types.ts';
  import type { FicheDocument, LigneFournisseur, Page, SocieteVisible, TypeDocument } from '#lib/types.ts';

  let d = $state<FicheDocument | null>(null);
  let societes = $state<SocieteVisible[]>([]);
  let erreur = $state('');
  let info = $state('');
  type Choix = {
    type: TypeDocument | '';
    societe: string;
    fournisseur: string;
    numero: string;
    date: string;
    commande: string;
    ht: string;
    tva: string;
    ttc: string;
  };
  let choix = $state<Choix>({ type: '', societe: '', fournisseur: '', numero: '', date: '', commande: '', ht: '', tva: '', ttc: '' });
  let fiches = $state<LigneFournisseur[]>([]);
  let rechercheFiche = $state('');
  let arret: (() => void) | null = null;
  let rap = $state<DetailRapprochement | null>(null);
  let candidat = $state('');

  const texte = (v: string | number | null) => (v === null ? '' : String(v));
  const enCours = (v: FicheDocument) =>
    v.etat === 'a_lire' || v.etat === 'en_cours' || v.statut_depot === 'en_envoi';

  async function charger() {
    const v = await api.get<FicheDocument>(`/api/documents/${page.params.id}`);
    d = v;
    choix = {
      type: v.type ?? '',
      societe: v.societe_id === null ? '' : String(v.societe_id),
      fournisseur: v.fournisseur_id === null ? '' : String(v.fournisseur_id),
      numero: v.numero ?? '',
      date: v.date_document ?? '',
      commande: v.numero_commande ?? '',
      ht: texte(v.montant_ht),
      tva: texte(v.montant_tva),
      ttc: texte(v.montant_ttc)
    };
    if (v.etat === 'lu') {
      rap = await api.get<DetailRapprochement>(`/api/documents/${v.id}/rapprochement`);
    }
    return enCours(v);
  }

  async function actionRap(f: () => Promise<unknown>, ok: string) {
    erreur = '';
    info = '';
    try {
      await f();
      await charger();
      info = ok;
    } catch (err) {
      erreur = message(err);
    }
  }
  function lier() {
    if (!d || !candidat) return;
    const autre = Number(candidat);
    const [facture_id, bl_id] = d.type === 'facture' ? [d.id, autre] : [autre, d.id];
    candidat = '';
    actionRap(() => api.post('/api/rapprochement', { facture_id, bl_id }), 'Documents reliés.');
  }
  function delier(id: number) {
    if (!confirm('Défaire ce lien ? Un lien trouvé automatiquement ne sera pas recréé.')) return;
    actionRap(() => api.del(`/api/rapprochement/${id}`), 'Lien défait.');
  }
  function basculerIgnore() {
    if (!d || !rap) return;
    const ignore = !rap.ignore;
    actionRap(
      () => api.post(`/api/documents/${d!.id}/rapprochement-ignore`, { ignore }),
      ignore ? 'Document sorti du rapprochement avec les bons de livraison.' : 'Document remis dans le rapprochement avec les bons de livraison.'
    );
  }
  const quantite = (v: number | null) => (v === null ? '—' : v.toLocaleString('fr-FR'));
  async function chercherFiches() {
    const q = rechercheFiche.trim();
    fiches = (await api.get<Page<LigneFournisseur>>(`/api/fournisseurs${q ? `?q=${encodeURIComponent(q)}` : ''}`)).elements;
  }
  $effect(() => {
    charger()
      .then((attendre) => {
        if (attendre) arret = sonder(charger);
      })
      .catch((e) => (erreur = (e as { code?: number }).code === 404 ? 'Document introuvable.' : message(e)));
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    chercherFiches().catch(() => {});
    return () => arret?.();
  });

  // La fiche actuelle reste dans la liste même si la recherche ne la rend pas.
  let fichesProposees = $derived.by(() => {
    if (!d?.fournisseur_id || fiches.some((f) => f.id === d?.fournisseur_id)) return fiches;
    return [{ id: d.fournisseur_id, nom: d.fournisseur ?? `Fiche ${d.fournisseur_id}` } as LigneFournisseur, ...fiches];
  });

  const nombre = (v: string) => (v.trim() === '' ? null : Number(v.replace(',', '.')));
  const ou = (v: string) => (v.trim() === '' ? null : v.trim());

  async function corriger(e: SubmitEvent) {
    e.preventDefault();
    if (!d || !choix.type) return;
    erreur = '';
    info = '';
    try {
      await api.put(`/api/documents/${d.id}`, {
        type: choix.type,
        societe_id: choix.societe === '' ? null : Number(choix.societe),
        fournisseur_id: choix.fournisseur === '' ? null : Number(choix.fournisseur),
        numero: ou(choix.numero),
        date_document: ou(choix.date),
        numero_commande: ou(choix.commande),
        montant_ht: nombre(choix.ht),
        montant_tva: nombre(choix.tva),
        montant_ttc: nombre(choix.ttc)
      });
      await charger();
      info = 'Document validé, contrôles relancés.';
    } catch (err) {
      erreur = message(err);
    }
  }

  async function deposer(forcer = false) {
    if (!d) return;
    if (forcer && !confirm('Déposer malgré le blocage ? Le document partira tel quel.')) return;
    erreur = '';
    info = '';
    try {
      await api.post(`/api/documents/${d.id}/deposer`, { forcer });
      await charger();
      info = 'Dépôt lancé.';
      arret?.();
      arret = sonder(charger);
    } catch (err) {
      erreur = message(err);
    }
  }

  async function recontroler() {
    if (!d) return;
    erreur = '';
    info = '';
    try {
      await api.post(`/api/documents/${d.id}/controler`);
      await charger();
      info = 'Contrôles relancés.';
    } catch (err) {
      erreur = message(err);
    }
  }

  let admin = $derived(session.moi?.role === 'admin');
  let fige = $derived(d?.statut_depot === 'en_envoi' || d?.statut_depot === 'depose');
  let corrections = $derived(d ? Object.entries(d.corrections ?? {}) : []);
  const libelleChamp: Record<string, string> = {
    type: 'type',
    societe_id: 'société',
    fournisseur_id: 'fournisseur',
    numero: 'numéro',
    date_document: 'date',
    numero_commande: 'n° de commande',
    montant_ht: 'HT',
    montant_tva: 'TVA',
    montant_ttc: 'TTC'
  };
  const valeur = (v: unknown) => (v === null || v === undefined || v === '' ? 'vide' : String(v));

  async function relire() {
    if (!d || !confirm('Relire ce document ? La lecture et la vérification actuelles seront remplacées.')) return;
    erreur = '';
    info = '';
    try {
      await api.post(`/api/admin/documents/${d.id}/relire`);
      await charger();
      arret?.();
      arret = sonder(charger);
    } catch (err) {
      erreur = message(err);
    }
  }

  function confiance(c: number | undefined) {
    if (c === undefined) return null;
    const pct = `${Math.round(c * 100)} %`;
    if (c >= 0.9) return { texte: 'Lecture sûre', classe: 'ok', pct };
    if (c >= 0.6) return { texte: 'À contrôler', classe: 'att', pct };
    return { texte: 'Peu fiable', classe: 'ko', pct };
  }
  // Champs que Claude a lus avec moins de certitude : à regarder en premier.
  let douteux = $derived.by(() => {
    if (!d) return [] as string[];
    const noms: [string, number | undefined][] = [
      ['le type', d.confiance.type],
      ['la société destinataire', d.confiance.societe],
      ['le fournisseur', d.confiance.fournisseur],
      ['le numéro', d.confiance.numero],
      ['la date', d.confiance.date],
      ['les montants', d.confiance.montants]
    ];
    return noms.filter(([, c]) => c !== undefined && c < 0.9).map(([n]) => n);
  });
  let estImage = $derived(!!d && d.type_mime.startsWith('image/'));
  const aideDepot: Record<string, string> = {
    a_verifier: 'Validez les données ci-dessus : le dépôt sera alors possible.',
    pret: 'Tous les contrôles passent : le document peut être déposé.',
    en_envoi: 'Le dépôt est en cours, cette page se met à jour toute seule.',
    depose: 'Ce document a été envoyé : il ne peut plus être corrigé.',
    erreur: 'Dext ou Yooz a refusé le document. Un administrateur peut le déposer à nouveau.',
    doublon: 'Ce document semble déjà déposé. Seul un administrateur peut le déposer quand même.'
  };
</script>

{#snippet conf(c: number | undefined)}
  {@const cf = confiance(c)}
  {#if cf}<span class="etiquette {cf.classe}" title="Confiance de la lecture : {cf.pct}">{cf.texte}</span>{/if}
{/snippet}

<p><a href="/documents">← Tous les documents</a></p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}
{#if !d && !erreur}<div class="charge" aria-label="Chargement du document"><span></span></div>{/if}
{#if !d && erreur}<div class="actions"><a class="bouton second" href="/documents">Revenir à la liste des documents</a></div>{/if}

{#if d}
  <div class="entete">
    <div>
      <h1>{d.type ? libelleType[d.type] : 'Document'}{d.numero ? ` n° ${d.numero}` : ''}</h1>
      <p class="sous">
        {d.fournisseur ?? d.fournisseur_nom ?? 'Fournisseur non renseigné'} · {dateSimple(d.date_document)} · {montant(d.montant_ttc)} TTC
      </p>
    </div>
    <div class="actions">
      <span class="etiquette {classeEtat[d.etat]}">{d.etat === 'lu' && !d.verifie_le ? 'À vérifier' : d.etat === 'lu' ? 'Vérifié' : libelleEtat[d.etat]}</span>
      {#if d.statut_depot}<span class="etiquette {classeStatutDepot[d.statut_depot]}">{libelleStatutDepot[d.statut_depot]}</span>{/if}
      <a class="bouton second" href="/api/pieces/{d.piece_id}" download>Télécharger le fichier</a>
      {#if session.moi?.role === 'admin' && d.etat !== 'en_cours'}
        <button class="discret" onclick={relire} title="Claude relit le fichier ; la lecture et la vérification actuelles sont remplacées">Relire avec Claude</button>
      {/if}
    </div>
  </div>

  {#if d.etat === 'erreur'}
    <div class="bandeau erreur">
      <strong>{aideEtat.erreur}</strong>
      {#if d.message}<span class="detail">{d.message}</span>{/if}
      Téléchargez le fichier pour le traiter à la main{session.moi?.role === 'admin' ? ', ou demandez à Claude de le relire' : ', ou demandez à un administrateur de le faire relire'}.
    </div>
  {:else if d.etat === 'a_lire' || d.etat === 'en_cours'}
    <div class="bandeau info" role="status">{aideEtat[d.etat]}</div>
  {:else if d.etat === 'ignore'}
    <div class="bandeau info">{aideEtat.ignore}{#if d.message} <span class="detail">{d.message}</span>{/if}</div>
  {:else if d.message}
    <div class="bandeau attention">{d.message}</div>
  {/if}

  <div class="fiche-doc">
    <div class="apercu">
      {#if estImage}
        <img src="/api/pieces/{d.piece_id}" alt="Document {d.nom_fichier}" />
      {:else if d.texte}
        <div class="corps-apercu">
          <p class="discret">Aperçu du fichier indisponible ici : voici le texte qu’il contient.</p>
          <pre class="corps">{d.texte}</pre>
        </div>
      {:else}
        <div class="corps-apercu">
          <p><strong>Pas d’aperçu ici</strong></p>
          <p class="discret">Téléchargez le fichier pour l’ouvrir et le comparer avec les données lues.</p>
        </div>
      {/if}
      <div class="pied-apercu discret">
        <span>{d.nom_fichier} ({d.type_mime})</span>
        <span>Joint au mail <a href="/mails/{d.mail_id}">{d.mail_objet || '(sans objet)'}</a> de {d.mail_expediteur || '—'}</span>
        {#if d.voie}<span>Lu par : {libelleVoie[d.voie]}{d.modele ? ` (${d.modele}${d.effort ? `, ${d.effort}` : ''})` : ''}{d.lu_le ? `, le ${quand(d.lu_le)}` : ''}</span>{/if}
        {#if d.copie_de}<span>Même fichier que le <a href="/documents/{d.copie_de}">document {d.copie_de}</a>, dont la lecture est reprise</span>{/if}
      </div>
    </div>

    <div class="pile">
      {#if d.etat === 'lu'}
        <form class="carte" onsubmit={corriger}>
          <h2 class="h-fiche">Données lues par Claude</h2>
          {#if d.verifie_le}
            <p class="discret">
              Vérifié le {quand(d.verifie_le)}{d.verifie_par ? ` par ${d.verifie_par}` : ''}.
              {#if !corrections.length && !d.type_corrige && !d.societe_corrigee}Lecture confirmée sans correction.{/if}
            </p>
          {:else}
            <p>Comparez avec le document, corrigez si besoin, puis cliquez sur Valider. Vos corrections servent d’exemples à Claude pour ce fournisseur.</p>
          {/if}
          {#if douteux.length && !d.verifie_le}
            <div class="bandeau attention">Claude est moins sûr de : {douteux.join(', ')}. Regardez ces champs en premier.</div>
          {/if}
          {#if corrections.length}
            <p class="discret">
              Corrigé : {#each corrections as [champ, c], i (champ)}{i ? ' ; ' : ''}{libelleChamp[champ] ?? champ} ({valeur(c.avant)} → {valeur(c.apres)}){/each}
            </p>
          {/if}
          {#if fige}<div class="bandeau info">Document déposé ou en cours de dépôt : plus de correction possible.</div>{/if}
          <fieldset disabled={fige}>
            <legend>Le document</legend>
            <div class="champs">
              <div>
                <div class="lib"><label for="vt">Type</label>{@render conf(d.confiance.type)}</div>
                <select id="vt" bind:value={choix.type} required>
                  {#each TYPES as t (t)}<option value={t}>{libelleType[t]}</option>{/each}
                </select>
              </div>
              <div>
                <div class="lib"><label for="vs">Société destinataire</label>{@render conf(d.confiance.societe)}</div>
                <select id="vs" bind:value={choix.societe}>
                  <option value="">Aucune</option>
                  {#each societes as so (so.id)}<option value={String(so.id)}>{so.raison_sociale}</option>{/each}
                </select>
              </div>
              <div>
                <div class="lib"><label for="vn">Numéro</label>{@render conf(d.confiance.numero)}</div>
                <input id="vn" bind:value={choix.numero} />
              </div>
              <div>
                <div class="lib"><label for="vd">Date</label>{@render conf(d.confiance.date)}</div>
                <input id="vd" type="date" bind:value={choix.date} />
              </div>
              <div><div class="lib"><label for="vc">N° de commande</label></div><input id="vc" bind:value={choix.commande} /></div>
            </div>
          </fieldset>
          <fieldset disabled={fige}>
            <legend>Le fournisseur</legend>
            <p class="discret">
              Lu sur le document : {[d.fournisseur_nom, d.fournisseur_siret && `SIRET ${d.fournisseur_siret}`, d.fournisseur_tva && `TVA ${d.fournisseur_tva}`].filter(Boolean).join(' · ') || 'Non renseigné'}
              {#if d.fournisseur_id}
                · fiche : <a href="/fournisseurs?fiche={d.fournisseur_id}">{d.fournisseur}</a>
                {#if d.fournisseur_statut === 'a_confirmer'}<span class="etiquette att">À confirmer</span>{/if}
              {/if}
            </p>
            <div class="champs">
              <div>
                <div class="lib"><label for="vf">Fiche fournisseur</label>{@render conf(d.confiance.fournisseur)}</div>
                <select id="vf" bind:value={choix.fournisseur}>
                  <option value="">Aucune</option>
                  {#each fichesProposees as f (f.id)}<option value={String(f.id)}>{f.nom}{f.statut === 'a_confirmer' ? ' (à confirmer)' : ''}</option>{/each}
                </select>
              </div>
              <div>
                <div class="lib"><label for="rf">Chercher une fiche</label></div>
                <input id="rf" type="search" placeholder="Nom, SIRET, domaine" bind:value={rechercheFiche} oninput={() => chercherFiches().catch(() => {})} />
              </div>
            </div>
          </fieldset>
          <fieldset disabled={fige}>
            <legend>Les montants {#if confiance(d.confiance.montants)}{@const cf = confiance(d.confiance.montants)!}<span class="etiquette {cf.classe}" title="Confiance de la lecture : {cf.pct}">{cf.texte}</span>{/if}</legend>
            <div class="champs">
              <div><div class="lib"><label for="vh">Montant HT</label></div><input id="vh" inputmode="decimal" bind:value={choix.ht} /></div>
              <div><div class="lib"><label for="vtv">TVA</label></div><input id="vtv" inputmode="decimal" bind:value={choix.tva} /></div>
              <div><div class="lib"><label for="vtt">Montant TTC</label></div><input id="vtt" inputmode="decimal" bind:value={choix.ttc} /></div>
            </div>
          </fieldset>
          <div class="actions">
            <button class:second={!!d.verifie_le} disabled={!choix.type || fige}>{d.verifie_le ? 'Enregistrer à nouveau' : 'Valider'}</button>
            {#if !d.verifie_le}<span class="discret">Valider confirme que les données correspondent au document.</span>{/if}
          </div>
        </form>

        {#if d.statut_depot}
          <section class="carte" aria-labelledby="h-depot">
            <h2 class="h-fiche" id="h-depot">Dépôt chez Dext ou Yooz</h2>
            <p>{aideDepot[d.statut_depot]}</p>
            <p class="discret">
              {#if d.societe_adresse_depot}
                Destination : {d.societe_outil_depot ? (libelleOutil[d.societe_outil_depot] ?? d.societe_outil_depot) : 'l’outil de la société'} ({d.societe_adresse_depot})
              {:else}
                La société n’a pas d’adresse de dépôt : ajoutez-la dans l’administration avant de déposer.
              {/if}
            </p>
            {#if d.controles.length}
              <h3>Contrôles avant dépôt</h3>
              <ul class="controles">
                {#each d.controles as c (c.code)}<li>{c.message}</li>{/each}
              </ul>
            {:else if d.statut_depot === 'pret'}
              <p class="vert">Tous les contrôles passent.</p>
            {/if}
            <div class="actions">
              {#if d.statut_depot === 'a_verifier' || d.statut_depot === 'pret'}
                <button class:second={!d.verifie_le} onclick={() => deposer()}>Valider et déposer</button>
              {:else if d.statut_depot === 'erreur' && admin}
                <button onclick={() => deposer()}>Redéposer</button>
              {:else if d.statut_depot === 'doublon' && admin}
                <button class="danger" onclick={() => deposer(true)}>Déposer quand même</button>
              {/if}
              {#if !fige}<button class="second" onclick={recontroler} title="Refait les contrôles avec les données actuelles">Relancer les contrôles</button>{/if}
            </div>
            {#if d.valide_le}
              <p class="discret">Validé le {quand(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}.</p>
            {/if}
            {#if d.envois.length}
              <h3>Envois déjà faits</h3>
              <table>
                <thead><tr><th>Quand</th><th>Adresse</th><th>Résultat</th><th>Par</th></tr></thead>
                <tbody>
                  {#each d.envois as e (e.id)}
                    <tr>
                      <td>{quand(e.quand)}</td>
                      <td>{e.adresse}</td>
                      <td>
                        <span class="etiquette {e.etat === 'envoye' ? 'ok' : e.etat === 'erreur' ? 'ko' : 'att'}">{e.etat === 'envoye' ? 'Envoyé' : e.etat === 'erreur' ? 'En erreur' : 'En cours'}</span>
                        {#if e.message}<span class="discret">{e.message}</span>{/if}
                      </td>
                      <td>{e.auteur ?? 'Automatique'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </section>
        {/if}
      {/if}
    </div>
  </div>

  {#if d.etat === 'lu' && rap}
    {#if d.type === 'facture' || d.type === 'bl'}
      <div class="section">
        <div class="titre"><h2>{d.type === 'facture' ? 'Facture reliée à son bon de livraison' : 'Bon de livraison relié à sa facture'}</h2></div>
        <div class="carte pile">
          {#if d.type === 'facture' && rap.numeros_bl.length}
            <p class="discret">Bons de livraison cités par la facture : {rap.numeros_bl.join(', ')}</p>
          {/if}
          {#if rap.ignore}
            <p class="discret">Pas de correspondance attendue : ce document n’est pas relié automatiquement.</p>
          {/if}
          {#each rap.liens as l (l.id)}
            <div class="actions">
              <span>{l.autre.type === 'bl' ? 'Bon de livraison' : 'Facture'} <a href="/documents/{l.autre.id}">{l.autre.numero ?? l.autre.nom_fichier}</a></span>
              <span class="discret">{dateSimple(l.autre.date_document)} · {montant(l.autre.montant_ht)} HT · {libelleMethode[l.methode]}{l.auteur ? ` (${l.auteur})` : ''}</span>
              <span class="espace"></span>
              <button class="second petit" onclick={() => delier(l.id)}>Défaire le lien</button>
            </div>
          {:else}
            <p class="discret">{d.type === 'facture' ? 'Aucun bon de livraison relié.' : 'Aucune facture reliée.'}</p>
          {/each}
          {#if rap.liens.length}
            {@const l0 = rap.liens[0]}
            <div>
              Écart de montant HT (facture moins bon de livraison) :
              {#if l0.ecart_montant === null}<span class="discret">non calculable (montant manquant)</span>
              {:else}<span class="etiquette {Math.abs(l0.ecart_montant) > SEUIL_ECART ? 'ko' : 'ok'}">{montant(l0.ecart_montant)}</span>{/if}
            </div>
            {#if l0.ecarts.length}
              <table>
                <thead><tr><th>Article</th><th class="nombre">Livré (bon de livraison)</th><th class="nombre">Facturé</th></tr></thead>
                <tbody>
                  {#each l0.ecarts as e, i (i)}
                    <tr><td>{e.reference ?? ''} {e.designation ?? ''}</td><td class="nombre">{quantite(e.qte_bl)}</td><td class="nombre">{quantite(e.qte_facture)}</td></tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          {/if}
          {#if rap.candidats.length}
            <form class="actions" onsubmit={(e) => { e.preventDefault(); lier(); }}>
              <select bind:value={candidat} class="choix-lien" aria-label="Document à relier">
                <option value="">Relier à {d.type === 'facture' ? 'un bon de livraison' : 'une facture'} du même fournisseur…</option>
                {#each rap.candidats as c (c.id)}
                  <option value={String(c.id)}>{c.numero ?? c.nom_fichier} · {dateSimple(c.date_document)} · {montant(c.montant_ht)}</option>
                {/each}
              </select>
              <button class="second" disabled={!candidat}>Relier</button>
            </form>
          {/if}
          <div><button class="discret" onclick={basculerIgnore}>{rap.ignore ? 'Remettre dans le rapprochement' : 'Aucune correspondance attendue'}</button></div>
        </div>
      </div>
    {/if}
    {#if rap.lignes.length}
      <div class="section">
        <div class="titre"><h2>Lignes du document</h2><span class="discret">{rap.lignes.length} {rap.lignes.length > 1 ? 'lignes' : 'ligne'}</span></div>
        <table>
          <thead><tr><th>Référence</th><th>Désignation</th><th class="nombre">Quantité</th><th class="nombre">Prix unitaire HT</th><th class="nombre">Montant HT</th></tr></thead>
          <tbody>
            {#each rap.lignes as l (l.rang)}
              <tr>
                <td>{l.reference ?? '—'}</td>
                <td>{l.designation ?? '—'}</td>
                <td class="nombre">{quantite(l.quantite)}{l.unite ? ` ${libelleUnite(l.unite)}` : ''}</td>
                <td class="nombre">{l.prix_unitaire_ht === null ? '—' : montant(l.prix_unitaire_ht)}</td>
                <td class="nombre">{montant(l.montant_ht)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else if d.type === 'bl' && !rap.lignes_lues}
      <p class="discret">Bon de livraison lu sans ses lignes (avant le suivi des livraisons) : un administrateur peut le faire relire.</p>
    {/if}
  {/if}

  {#if d.texte && estImage}
    <details class="repli">
      <summary>Texte lu dans le document</summary>
      <div class="carte"><pre class="corps">{d.texte}</pre></div>
    </details>
  {/if}
{/if}

<style>
  .h-fiche { margin: 0 0 var(--e3); }
  .lib { display: flex; align-items: center; justify-content: space-between; gap: var(--e2); min-height: 22px; margin-bottom: var(--e1); }
  .lib label { margin: 0; }
  fieldset:disabled { opacity: 0.85; }
  .controles { margin: 0 0 var(--e4); padding-left: var(--e5); }
  .espace { flex: 1; }
  .choix-lien { max-width: 28rem; width: auto; flex: 1 1 14rem; }
  .detail { display: block; font-weight: 400; margin-top: var(--e1); }
  .actions > .espace { flex: 1 1 0; }
</style>
