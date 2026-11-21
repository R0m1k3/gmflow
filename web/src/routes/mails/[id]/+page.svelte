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
      .catch((e) => (erreur = (e as { code?: number }).code === 404 ? 'Mail introuvable.' : message(e)));
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
    }, 'Catégorie enregistrée : le tri automatique ne la changera plus.');

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
    });
  }

  function abandonner() {
    if (!confirm('Abandonner ce brouillon ?')) return;
    return action(async () => {
      await api.post(`/api/brouillons/${actif!.id}/abandonner`);
      await lire();
    });
  }
</script>

<p><a href="/mails">← Mails</a> · <a href="/a-repondre">À répondre</a></p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}
{#if m}
  <h1>{m.objet || '(sans objet)'}</h1>
  <div class="carte entetes">
    <div><span class="discret">De</span> {m.expediteur || '—'}</div>
    <div><span class="discret">À</span> {m.destinataires.join(', ') || '—'}</div>
    <div><span class="discret">Date</span> <span class="date">{quand(m.date_mail)}</span></div>
    <div><span class="discret">Reçu dans</span> {m.boites.map((b) => `${b.libelle} (${b.dossier})`).join(', ')}</div>
    <div class="discret mid">Message-ID : {m.message_id}</div>
  </div>

  <div class="carte tri">
    <div class="ligne">
      {#if m.categorie}
        <span class="etiquette {classeCategorie[m.categorie]}">{libelleCategorie[m.categorie]}</span>
        <span class="discret">{m.categorie_par ? libelleCategoriePar[m.categorie_par] : ''}</span>
      {:else if m.tri_etat === 'erreur'}
        <span class="etiquette ko">tri en erreur</span>
      {:else if m.tri_etat}
        <span class="etiquette att">à trier</span>
      {:else}
        <span class="etiquette">non trié</span>
      {/if}
      {#if m.priorite && m.categorie === 'question'}
        <span class="etiquette {classePriorite[m.priorite]}">priorité {libellePriorite[m.priorite]}</span>
      {/if}
      {#if m.statut_reponse}
        <span class="etiquette {m.statut_reponse === 'a_repondre' ? 'att' : 'ok'}">{libelleStatutReponse[m.statut_reponse]}</span>
      {/if}
      <span style="flex:1"></span>
      <label class="en-ligne" for="cat">Catégorie</label>
      <select
        id="cat"
        class="court"
        value={m.categorie ?? ''}
        disabled={occupe}
        onchange={(e) => changerCategorie((e.currentTarget as HTMLSelectElement).value as Categorie)}
      >
        {#if !m.categorie}<option value="" disabled>choisir…</option>{/if}
        {#each CATEGORIES as c (c)}<option value={c}>{libelleCategorie[c]}</option>{/each}
      </select>
    </div>
    {#if m.resume}<p class="resume">{m.resume}</p>{/if}
    {#if m.tri_message && m.categorie_par !== 'humain'}<p class="discret">{m.tri_message}</p>{/if}
    <div class="actions">
      {#if m.statut_reponse === 'a_repondre'}
        <button class="second" disabled={occupe} onclick={() => statut('sans_suite', 'Classé sans suite.')}>Sans suite</button>
        <button class="second" disabled={occupe} onclick={() => statut('repondu', 'Noté comme répondu ailleurs.')}>Répondu ailleurs</button>
      {:else if m.statut_reponse}
        <button class="second" disabled={occupe} onclick={() => statut('a_repondre', 'Remis dans la file à répondre.')}>Rouvrir</button>
      {:else if m.categorie !== 'publicite'}
        <button class="second" disabled={occupe} onclick={() => statut('a_repondre', 'Ajouté à la file à répondre.')}>Ajouter à « À répondre »</button>
      {/if}
      {#if admin && m.expediteur.includes('@')}
        <span style="flex:1"></span>
        <span class="discret">Toujours publicité :</span>
        <button class="second" disabled={occupe} onclick={() => marquer('adresse')}>cet expéditeur</button>
        <button class="second" disabled={occupe} onclick={() => marquer('domaine')}>ce domaine</button>
      {/if}
    </div>
  </div>

  {#if m.categorie === 'question' || m.statut_reponse || m.brouillons.length}
    <h2>Réponse</h2>
    {#if actif}
      <div class="carte brouillon">
        <div class="ligne">
          <span class="etiquette {EN_COURS.includes(actif.statut) ? 'att' : ''}">{libelleBrouillon[actif.statut]}</span>
          <span class="discret">depuis {actif.boite}{actif.modele ? ` · ${actif.modele}` : ''} · {quand(actif.modifie_le)}</span>
        </div>
        {#if actif.erreur}<div class="bandeau attention">{actif.erreur}</div>{/if}
        {#if actif.statut === 'generation'}
          <p class="discret">Claude prépare un brouillon dans le ton de vos réponses précédentes…</p>
        {:else}
          <textarea rows="12" bind:value={texte} disabled={actif.statut === 'envoi' || occupe}></textarea>
          {#if !actif.smtp}
            <p class="discret">L'envoi n'est pas possible : le serveur SMTP de cette boîte n'est pas configuré (Administration, Boîtes surveillées).</p>
          {/if}
          {#if actif.statut !== 'envoi'}
            <div class="actions">
              <button onclick={envoyer} disabled={occupe || !actif.smtp || !texte.trim()}>Envoyer</button>
              <button class="second" onclick={enregistrer} disabled={occupe || !modifie}>Enregistrer</button>
              <button class="second" onclick={preparer} disabled={occupe}>Régénérer</button>
              <span style="flex:1"></span>
              <button class="danger" onclick={abandonner} disabled={occupe}>Abandonner</button>
            </div>
            <p class="discret">Rien ne part sans votre clic sur « Envoyer ». La réponse reste dans le fil du mail reçu.</p>
          {/if}
        {/if}
      </div>
    {:else}
      <div class="carte actions">
        {#if m.boites.length > 1}
          <label class="en-ligne" for="be">Répondre depuis</label>
          <select id="be" class="court" bind:value={boiteEnvoi}>
            {#each m.boites as b (b.boite_id)}<option value={String(b.boite_id)}>{b.libelle}</option>{/each}
          </select>
        {/if}
        <button onclick={preparer} disabled={occupe}>Préparer un brouillon</button>
        <span class="discret">Claude propose un texte ; vous le relisez et l'envoyez vous-même.</span>
      </div>
    {/if}
    {#each envoyes as b (b.id)}
      <details class="carte envoye">
        <summary>
          <span class="etiquette ok">envoyé</span>
          <span class="discret">{quand(b.envoye_le)}{b.envoye_par ? ` par ${b.envoye_par}` : ''} depuis {b.boite}</span>
        </summary>
        {#if b.erreur}<div class="bandeau attention">{b.erreur}</div>{/if}
        <pre class="corps">{b.texte}</pre>
      </details>
    {/each}
  {/if}

  {#if m.pieces.length}
    <h2>Pièces jointes</h2>
    <table>
      <tbody>
        {#each m.pieces as p (p.id)}
          <tr>
            <td><a href="/api/pieces/{p.id}" download>{p.nom}</a></td>
            <td class="discret">{p.type_mime}</td>
            <td class="nombre">{taille(p.taille)}</td>
            <td>
              {#if p.document_id && p.document_etat}
                <a href="/documents/{p.document_id}">{p.document_type ? libelleType[p.document_type] : 'Document'}</a>
                <span class="etiquette {classeEtat[p.document_etat]}">{libelleEtat[p.document_etat]}</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <h2>Message</h2>
  <div class="carte"><pre class="corps">{m.corps_texte || '(corps vide)'}</pre></div>
{/if}

<style>
  .entetes { display: grid; gap: 0.3rem; }
  .entetes span.discret { display: inline-block; min-width: 5.5rem; }
  .mid { word-break: break-all; font-size: 0.8rem; }
  .tri, .brouillon { margin-top: 1rem; display: grid; gap: 0.6rem; }
  .ligne { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .resume { margin: 0; font-weight: 600; }
  .en-ligne { margin: 0; }
  select.court { width: auto; }
  .brouillon .bandeau { margin: 0; }
  textarea { resize: vertical; line-height: 1.45; }
  .envoye { margin-top: 0.6rem; }
  .envoye summary { cursor: pointer; display: flex; gap: 0.5rem; align-items: center; }
</style>
