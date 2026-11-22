<script lang="ts">
  import { page } from '$app/state';
  import { api, message, sonder } from '#lib/api.ts';
  import { quand, taille } from '#lib/format.ts';
  import { classeEtat, libelleEtat, libelleType } from '#lib/documents.ts';
  import {
    CATEGORIES,
    classeCategorie,
    classePriorite,
    libelleBrouillon,
    libelleCategorie,
    libelleCategoriePar,
    libellePriorite,
    libelleStatutReponse
  } from '#lib/reponses.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Brouillon, Categorie, FicheMail } from '#lib/types.ts';

  let m = $state<FicheMail | null>(null);
  let erreur = $state('');
  let info = $state('');
  let texte = $state('');
  let boiteEnvoi = $state('');
  let occupe = $state(false);
  let chargement = $state(true);
  const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
  let arret: (() => void) | null = null;

  const EN_COURS = ['generation', 'envoi'];
  let actif = $derived<Brouillon | undefined>(m?.brouillons.find((b) => ['generation', 'propose', 'modifie', 'envoi'].includes(b.statut)));
  let envoyes = $derived(m?.brouillons.filter((b) => b.statut === 'envoye') ?? []);
  let modifie = $derived(!!actif && texte !== actif.texte);
  let admin = $derived(session.moi?.role === 'admin');

  async function lire() {
    const v = await api.get<FicheMail>(`/api/mails/${page.params.id}`);
    const avant = actif;
    m = v;
    const b = v.brouillons.find((x) => ['generation', 'propose', 'modifie', 'envoi'].includes(x.statut));
    // Le texte saisi n'est remplacé que quand le brouillon change côté serveur.
    if (!b || !avant || b.id !== avant.id || b.modifie_le !== avant.modifie_le) texte = b?.texte ?? '';
    if (!boiteEnvoi && v.boites.length) boiteEnvoi = String(v.boites[0].boite_id);
    return b;
  }

  function suivre() {
    arret?.();
    arret = sonder(async () => {
      const b = await lire();
      return !!b && EN_COURS.includes(b.statut);
    });
  }

  $effect(() => {
    lire()
      .then((b) => {
        if (b && EN_COURS.includes(b.statut)) suivre();
      })
      .catch((e) => (erreur = (e as { code?: number }).code === 404 ? 'Ce mail est introuvable : il a peut-être été supprimé.' : message(e)))
      .finally(() => (chargement = false));
    return () => arret?.();
  });

  async function action(fn: () => Promise<unknown>, ok?: string) {
    erreur = '';
    info = '';
    occupe = true;
    try {
      await fn();
      if (ok) info = ok;
    } catch (e) {
      erreur = message(e);
    } finally {
      occupe = false;
    }
  }

  const changerCategorie = (c: Categorie) =>
    action(async () => {
      await api.put(`/api/mails/${m!.id}/categorie`, { categorie: c });
      await lire();
    }, 'Catégorie enregistrée : le tri automatique ne la changera plus');

  const statut = (s: string, ok: string) =>
    action(async () => {
      await api.post(`/api/mails/${m!.id}/statut-reponse`, { statut: s });
      await lire();
    }, ok);

  function marquer(portee: 'adresse' | 'domaine') {
    const cible = portee === 'adresse' ? m!.expediteur : (m!.expediteur.split('@')[1] ?? '');
    if (!confirm(`Classer en publicité tous les mails de ${cible}, y compris les prochains ?`)) return;
    return action(async () => {
      const r = await api.post<{ reclasses: number }>('/api/admin/expediteurs-marques', { mail_id: m!.id, portee });
      await lire();
      info = `${cible} classé en publicité (${r.reclasses} mail${r.reclasses > 1 ? 's' : ''} reclassé${r.reclasses > 1 ? 's' : ''}).`;
    });
  }

  const preparer = () =>
    action(async () => {
      await api.post(`/api/mails/${m!.id}/brouillons`, { boite_id: Number(boiteEnvoi) || null });
      await lire();
      suivre();
      info = 'Brouillon en préparation : il apparaît ici dans quelques instants.';
    });

  const enregistrer = () =>
    action(async () => {
      await api.put(`/api/brouillons/${actif!.id}`, { texte });
      await lire();
    }, 'Brouillon enregistré.');

  function envoyer() {
    const b = actif!;
    if (!confirm(`Envoyer cette réponse depuis « ${b.boite} » ? Elle part telle qu'affichée.`)) return;
    return action(async () => {
      if (texte !== b.texte) await api.put(`/api/brouillons/${b.id}`, { texte });
      await api.post(`/api/brouillons/${b.id}/envoyer`);
      await lire();
      suivre();
      info = 'Envoi en cours : la réponse part depuis ' + b.boite + '.';
    });
  }

  function abandonner() {
    if (!confirm('Abandonner ce brouillon ?')) return;
    return action(async () => {
      await api.post(`/api/brouillons/${actif!.id}/abandonner`);
      await lire();
      info = 'Brouillon abandonné.';
    });
  }
</script>

<p class="mail-retour"><a href="/mails">← Tous les mails</a> · <a href="/a-repondre">Mails à répondre</a></p>
{#if erreur}
  <div class="bandeau erreur" role="alert">
    {erreur}
    {#if !m}<div class="actions"><a class="bouton second petit" href="/mails">Retour aux mails</a></div>{/if}
  </div>
{/if}
{#if info}<div class="bandeau ok" role="status">{info}</div>{/if}
{#if !m && chargement && !erreur}<div class="charge" aria-busy="true"><span></span></div>{/if}
{#if m}
  <div class="entete">
    <div>
      <h1>{m.objet || '(sans objet)'}</h1>
      <p class="sous">De {m.expediteur || 'Non renseigné'} · <span class="date">{quand(m.date_mail)}</span></p>
    </div>
    <div class="mail-etats">
      {#if m.categorie}
        <span class="etiquette {classeCategorie[m.categorie]}">{libelleCategorie[m.categorie]}</span>
      {:else if m.tri_etat === 'erreur'}
        <span class="etiquette ko">Tri impossible</span>
      {:else if m.tri_etat}
        <span class="etiquette att">En attente de tri</span>
      {:else}
        <span class="etiquette">Non trié</span>
      {/if}
      {#if m.priorite && m.categorie === 'question'}
        <span class="etiquette {classePriorite[m.priorite]}">Priorité {libellePriorite[m.priorite]}</span>
      {/if}
      {#if m.statut_reponse}
        <span class="etiquette {m.statut_reponse === 'a_repondre' ? 'att' : 'ok'}">{cap(libelleStatutReponse[m.statut_reponse])}</span>
      {/if}
    </div>
  </div>

  {#if m.resume}<div class="bandeau info"><strong>En résumé :</strong> {m.resume}</div>{/if}

  {#if m.categorie === 'question' || m.statut_reponse || m.brouillons.length}
    <div class="section mail-premiere">
      <div class="titre"><h2>Votre réponse</h2></div>
      {#if actif}
        <div class="carte pile">
          <div class="mail-etats">
            <span class="etiquette {EN_COURS.includes(actif.statut) ? 'att' : ''}">{cap(libelleBrouillon[actif.statut])}</span>
            <span class="discret" title={actif.modele ?? ''}>Depuis {actif.boite} · {quand(actif.modifie_le)}</span>
          </div>
          {#if actif.erreur}<div class="bandeau attention">{actif.erreur}</div>{/if}
          {#if actif.statut === 'generation'}
            <div class="charge" aria-busy="true"><span></span></div>
            <p class="discret">Claude prépare un brouillon dans le ton de vos réponses précédentes…</p>
          {:else}
            <div>
              <label for="rep">Texte de la réponse</label>
              <textarea id="rep" rows="12" bind:value={texte} disabled={actif.statut === 'envoi' || occupe}></textarea>
            </div>
            {#if !actif.smtp}
              <div class="bandeau attention">L'envoi n'est pas possible : l'envoi des mails n'est pas réglé pour cette boîte mail (Administration, Boîtes mail).</div>
            {/if}
            {#if actif.statut !== 'envoi'}
              <div class="actions">
                <button onclick={envoyer} disabled={occupe || !actif.smtp || !texte.trim()}>Envoyer la réponse</button>
                <button class="second" onclick={enregistrer} disabled={occupe || !modifie}>Enregistrer</button>
                <button class="second" onclick={preparer} disabled={occupe}>Régénérer</button>
                <button class="danger" onclick={abandonner} disabled={occupe}>Abandonner</button>
              </div>
              <p class="aide">Rien ne part sans votre clic sur « Envoyer la réponse ». Elle reste dans le fil du mail reçu.</p>
            {/if}
          {/if}
        </div>
      {:else}
        <div class="carte pile">
          <p>Claude propose un texte ; vous le relisez, le modifiez si besoin, puis vous l'envoyez vous-même.</p>
          {#if m.boites.length > 1}
            <div>
              <label for="be">Répondre depuis</label>
              <select id="be" class="court" bind:value={boiteEnvoi}>
                {#each m.boites as b (b.boite_id)}<option value={String(b.boite_id)}>{b.libelle}</option>{/each}
              </select>
            </div>
          {/if}
          <div class="actions"><button onclick={preparer} disabled={occupe}>Préparer un brouillon</button></div>
        </div>
      {/if}
      {#each envoyes as b (b.id)}
        <details class="carte mail-envoye">
          <summary>
            <span class="etiquette ok">Envoyée</span>
            <span class="discret">{quand(b.envoye_le)}{b.envoye_par ? ` par ${b.envoye_par}` : ''} depuis {b.boite}</span>
          </summary>
          {#if b.erreur}<div class="bandeau attention">{b.erreur}</div>{/if}
          <pre class="corps">{b.texte}</pre>
        </details>
      {/each}
    </div>
  {/if}

  <div class="section">
    <div class="titre"><h2>Le message</h2></div>
    <div class="carte"><pre class="corps mail-corps">{m.corps_texte || '(corps vide)'}</pre></div>
  </div>

  {#if m.pieces.length}
    <div class="section">
      <div class="titre"><h2>Pièces jointes <span class="discret">{m.pieces.length}</span></h2></div>
      <ul class="liste">
        {#each m.pieces as p (p.id)}
          <li>
            <div class="principal">
              <a href="/api/pieces/{p.id}" download>{p.nom}</a>
              <span class="discret">{p.type_mime} · {taille(p.taille)}</span>
            </div>
            {#if p.document_id && p.document_etat}
              <div class="droite">
                <span class="etiquette {classeEtat[p.document_etat]}">{libelleEtat[p.document_etat]}</span>
                <a class="bouton second petit" href="/documents/{p.document_id}">Ouvrir {p.document_type ? libelleType[p.document_type].toLowerCase() : 'le document'}</a>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="section">
    <div class="titre"><h2>Classement</h2></div>
    <div class="carte pile">
      {#if m.tri_message && m.categorie_par !== 'humain'}<p class="discret">{m.tri_message}</p>{/if}
      <p class="discret">
        {#if m.categorie_par}Classé {libelleCategoriePar[m.categorie_par]}.{/if}
        Si le classement est faux, corrigez-le : le tri automatique s'en servira pour les mails suivants.
      </p>
      <div class="champs">
        <div>
          <label for="cat">Catégorie du mail</label>
          <select id="cat" value={m.categorie ?? ''} disabled={occupe} onchange={(e) => changerCategorie((e.currentTarget as HTMLSelectElement).value as Categorie)}>
            {#if !m.categorie}<option value="" disabled>Choisir…</option>{/if}
            {#each CATEGORIES as c (c)}<option value={c}>{libelleCategorie[c]}</option>{/each}
          </select>
        </div>
      </div>
      <div class="actions">
        {#if m.statut_reponse === 'a_repondre'}
          <button class="second" disabled={occupe} onclick={() => statut('sans_suite', 'Classé sans suite : le mail ne demande plus de réponse.')}>Sans suite</button>
          <button class="second" disabled={occupe} onclick={() => statut('repondu', 'Noté comme répondu ailleurs.')}>Répondu ailleurs</button>
        {:else if m.statut_reponse}
          <button class="second" disabled={occupe} onclick={() => statut('a_repondre', 'Remis dans les mails à répondre.')}>Rouvrir</button>
        {:else if m.categorie !== 'publicite'}
          <button class="second" disabled={occupe} onclick={() => statut('a_repondre', 'Ajouté aux mails à répondre.')}>Ajouter aux mails à répondre</button>
        {/if}
      </div>
      {#if admin && m.expediteur.includes('@')}
        <div class="actions">
          <span class="discret">Toujours classer en publicité :</span>
          <button class="second" disabled={occupe} onclick={() => marquer('adresse')}>Cet expéditeur</button>
          <button class="second" disabled={occupe} onclick={() => marquer('domaine')}>Ce domaine</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="section">
    <div class="titre"><h2>Détails du mail</h2></div>
    <dl class="kv">
      <dt>De</dt><dd>{m.expediteur || 'Non renseigné'}</dd>
      <dt>À</dt><dd>{m.destinataires.join(', ') || 'Non renseigné'}</dd>
      <dt>Date</dt><dd class="date">{quand(m.date_mail)}</dd>
      <dt>Reçu dans</dt><dd>{m.boites.map((b) => `${b.libelle} (${b.dossier})`).join(', ') || 'Non renseigné'}</dd>
    </dl>
    <details class="mail-tech"><summary class="discret">Détails techniques</summary><p class="discret mail-id">Message-ID : {m.message_id}</p></details>
  </div>
{/if}
