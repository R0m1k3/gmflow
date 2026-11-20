<script lang="ts">
  import { page } from '$app/state';
  import { api, message, sonder } from '#lib/api.ts';
  import { classeEtat, classeStatutDepot, libelleEtat, libelleOutil, libelleStatutDepot, libelleType, libelleVoie, TYPES } from '#lib/documents.ts';
  import { dateSimple, montant, quand } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
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
    return enCours(v);
  }
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
      info = 'Vérification enregistrée, contrôles relancés.';
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
    societe_id: 'structure',
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
    return { texte: `${Math.round(c * 100)} %`, classe: c >= 0.9 ? 'ok' : c >= 0.6 ? 'att' : 'ko' };
  }
</script>

<p><a href="/documents">← Documents</a></p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if d}
  <h1>{d.type ? libelleType[d.type] : 'Document'}{d.numero ? ` n° ${d.numero}` : ''}</h1>

  <div class="carte entete">
    <div class="actions">
      <span class="etiquette {classeEtat[d.etat]}">{libelleEtat[d.etat]}</span>
      {#if d.statut_depot}<span class="etiquette {classeStatutDepot[d.statut_depot]}">{libelleStatutDepot[d.statut_depot]}</span>{/if}
      {#if d.voie}<span class="discret">lu par : {libelleVoie[d.voie]}{d.modele ? ` (${d.modele}${d.effort ? `, ${d.effort}` : ''})` : ''}</span>{/if}
      {#if d.lu_le}<span class="discret">le {quand(d.lu_le)}</span>{/if}
      {#if d.copie_de}<span class="discret">repris du <a href="/documents/{d.copie_de}">document {d.copie_de}</a> (même fichier)</span>{/if}
      <span style="flex:1"></span>
      <a class="bouton second" href="/api/pieces/{d.piece_id}" download>Télécharger le fichier</a>
      {#if session.moi?.role === 'admin' && d.etat !== 'en_cours'}
        <button class="second" onclick={relire}>Relire</button>
      {/if}
    </div>
    {#if d.message}<p class="discret">{d.message}</p>{/if}
    <div class="discret">
      Fichier <strong>{d.nom_fichier}</strong> ({d.type_mime}), joint au mail
      <a href="/mails/{d.mail_id}">{d.mail_objet || '(sans objet)'}</a> de {d.mail_expediteur || '—'}
    </div>
  </div>

  {#if d.etat === 'lu'}
    <h2>Ce qui a été lu</h2>
    <table class="champs-lus">
      <tbody>
        {#each [
          ['Type', d.type ? libelleType[d.type] : '—', d.confiance.type],
          ['Structure destinataire', d.societe ?? 'aucune reconnue', d.confiance.societe],
          ['Fournisseur lu', [d.fournisseur_nom, d.fournisseur_siret && `SIRET ${d.fournisseur_siret}`, d.fournisseur_tva && `TVA ${d.fournisseur_tva}`].filter(Boolean).join(' · ') || '—', d.confiance.fournisseur],
          ['Numéro', d.numero ?? '—', d.confiance.numero],
          ['Date', dateSimple(d.date_document), d.confiance.date],
          ['N° de commande', d.numero_commande ?? '—', undefined],
          ['Montants', `HT ${montant(d.montant_ht)} · TVA ${montant(d.montant_tva)} · TTC ${montant(d.montant_ttc)}`, d.confiance.montants]
        ] as [string, string, number | undefined][] as [libelle, valeur, c] (libelle)}
          {@const conf = confiance(c)}
          <tr>
            <th>{libelle}</th>
            <td>{valeur}</td>
            <td class="conf">{#if conf}<span class="etiquette {conf.classe}" title="Confiance de la lecture">{conf.texte}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>

    {#if d.statut_depot}
      <h2>Dépôt</h2>
      <div class="carte depot">
        <div class="actions">
          <span class="etiquette {classeStatutDepot[d.statut_depot]}">{libelleStatutDepot[d.statut_depot]}</span>
          <span class="discret">
            {#if d.societe_adresse_depot}
              vers {d.societe_outil_depot ? (libelleOutil[d.societe_outil_depot] ?? d.societe_outil_depot) : 'l’outil'} · {d.societe_adresse_depot}
            {:else}
              la structure n'a pas d'adresse de dépôt
            {/if}
          </span>
          <span style="flex:1"></span>
          {#if d.statut_depot === 'a_verifier' || d.statut_depot === 'pret'}
            <button onclick={() => deposer()}>Valider et déposer</button>
          {:else if d.statut_depot === 'erreur' && admin}
            <button onclick={() => deposer()}>Redéposer</button>
          {:else if d.statut_depot === 'doublon' && admin}
            <button class="danger" onclick={() => deposer(true)}>Déposer quand même</button>
          {/if}
          {#if !fige}<button class="second" onclick={recontroler}>Relancer les contrôles</button>{/if}
        </div>
        {#if d.controles.length}
          <ul class="controles">
            {#each d.controles as c (c.code)}<li><strong>{c.code}</strong> : {c.message}</li>{/each}
          </ul>
        {:else if d.statut_depot === 'pret'}
          <p class="discret">Tous les contrôles passent.</p>
        {/if}
        {#if d.valide_le}
          <p class="discret">Validé le {quand(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}.</p>
        {/if}
        {#if d.fournisseur_id}
          <p class="discret">
            Fiche fournisseur : <a href="/fournisseurs?fiche={d.fournisseur_id}">{d.fournisseur}</a>
            {#if d.fournisseur_statut === 'a_confirmer'}<span class="etiquette att">à confirmer</span>{/if}
          </p>
        {/if}
        {#if d.envois.length}
          <table>
            <thead><tr><th>Envoi</th><th>Adresse</th><th>Résultat</th><th>Par</th></tr></thead>
            <tbody>
              {#each d.envois as e (e.id)}
                <tr>
                  <td>{quand(e.quand)}</td>
                  <td>{e.adresse}</td>
                  <td>
                    <span class="etiquette {e.etat === 'envoye' ? 'ok' : e.etat === 'erreur' ? 'ko' : 'att'}">{e.etat === 'envoye' ? 'envoyé' : e.etat === 'erreur' ? 'erreur' : 'en cours'}</span>
                    {#if e.message}<span class="discret">{e.message}</span>{/if}
                  </td>
                  <td>{e.auteur ?? 'automatique'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/if}

    <h2>Vérification et corrections</h2>
    <form class="carte" onsubmit={corriger}>
      {#if d.verifie_le}
        <p class="discret">
          Vérifié le {quand(d.verifie_le)}{d.verifie_par ? ` par ${d.verifie_par}` : ''}.
          {#if !corrections.length && !d.type_corrige && !d.societe_corrigee}Lecture confirmée sans correction.{/if}
        </p>
      {:else}
        <p class="discret">Confirmez ou corrigez ce qui a été lu. Les corrections servent d'exemples à Claude pour ce fournisseur.</p>
      {/if}
      {#if corrections.length}
        <p class="discret">
          Corrigé : {#each corrections as [champ, c], i (champ)}{i ? ' ; ' : ''}{libelleChamp[champ] ?? champ} ({valeur(c.avant)} → {valeur(c.apres)}){/each}
        </p>
      {/if}
      {#if fige}<p class="discret">Document déposé ou en cours d'envoi : plus de correction possible.</p>{/if}
      <fieldset disabled={fige}>
        <div class="champs">
          <div>
            <label for="vt">Type</label>
            <select id="vt" bind:value={choix.type} required>
              {#each TYPES as t (t)}<option value={t}>{libelleType[t]}</option>{/each}
            </select>
          </div>
          <div>
            <label for="vs">Structure destinataire</label>
            <select id="vs" bind:value={choix.societe}>
              <option value="">Aucune</option>
              {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
            </select>
          </div>
          <div>
            <label for="vf">Fiche fournisseur</label>
            <select id="vf" bind:value={choix.fournisseur}>
              <option value="">Aucune</option>
              {#each fichesProposees as f (f.id)}<option value={String(f.id)}>{f.nom}{f.statut === 'a_confirmer' ? ' (à confirmer)' : ''}</option>{/each}
            </select>
          </div>
          <div>
            <label for="rf">Chercher une fiche</label>
            <input id="rf" type="search" placeholder="nom, SIRET, domaine" bind:value={rechercheFiche} oninput={() => chercherFiches().catch(() => {})} />
          </div>
          <div><label for="vn">Numéro</label><input id="vn" bind:value={choix.numero} /></div>
          <div><label for="vd">Date</label><input id="vd" type="date" bind:value={choix.date} /></div>
          <div><label for="vc">N° de commande</label><input id="vc" bind:value={choix.commande} /></div>
          <div><label for="vh">Montant HT</label><input id="vh" inputmode="decimal" bind:value={choix.ht} /></div>
          <div><label for="vtv">TVA</label><input id="vtv" inputmode="decimal" bind:value={choix.tva} /></div>
          <div><label for="vtt">Montant TTC</label><input id="vtt" inputmode="decimal" bind:value={choix.ttc} /></div>
        </div>
        <button disabled={!choix.type}>{d.verifie_le ? 'Enregistrer à nouveau' : 'Valider la lecture'}</button>
      </fieldset>
    </form>
  {/if}

  {#if d.texte}
    <details>
      <summary>Texte du document</summary>
      <div class="carte"><pre class="corps">{d.texte}</pre></div>
    </details>
  {/if}
{/if}

<style>
  .entete { display: grid; gap: 0.5rem; }
  .champs-lus th { width: 13rem; background: var(--surface); color: var(--gris); }
  .champs-lus .conf { width: 5rem; text-align: right; }
  a.bouton { display: inline-block; text-decoration: none; padding: 0.42rem 0.9rem; border-radius: var(--rayon); border: 1px solid var(--accent); }
  a.bouton.second { background: #fff; color: var(--accent); }
  details { margin-top: 1.5rem; }
  .depot { display: grid; gap: 0.5rem; }
  .controles { margin: 0; padding-left: 1.1rem; }
  fieldset { border: none; padding: 0; margin: 0; display: grid; gap: 0.6rem; }
  fieldset > button { justify-self: start; }
  summary { cursor: pointer; font-weight: 650; margin-bottom: 0.6rem; }
</style>
