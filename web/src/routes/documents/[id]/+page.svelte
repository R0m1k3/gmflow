<script lang="ts">
  import { page } from '$app/state';
  import { api, message, sonder } from '#lib/api.ts';
  import { classeEtat, libelleEtat, libelleType, libelleVoie, TYPES } from '#lib/documents.ts';
  import { dateSimple, montant, quand } from '#lib/format.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { FicheDocument, SocieteVisible, TypeDocument } from '#lib/types.ts';

  let d = $state<FicheDocument | null>(null);
  let societes = $state<SocieteVisible[]>([]);
  let erreur = $state('');
  let info = $state('');
  let choix = $state<{ type: TypeDocument | ''; societe: string }>({ type: '', societe: '' });
  let arret: (() => void) | null = null;

  async function charger() {
    const v = await api.get<FicheDocument>(`/api/documents/${page.params.id}`);
    d = v;
    choix = { type: v.type ?? '', societe: v.societe_id === null ? '' : String(v.societe_id) };
    return v.etat === 'a_lire' || v.etat === 'en_cours';
  }
  $effect(() => {
    charger().catch((e) => (erreur = (e as { code?: number }).code === 404 ? 'Document introuvable.' : message(e)));
    api.get<SocieteVisible[]>('/api/societes').then((v) => (societes = v));
    return () => arret?.();
  });

  async function verifier(e: SubmitEvent) {
    e.preventDefault();
    if (!d || !choix.type) return;
    erreur = '';
    info = '';
    try {
      await api.post(`/api/documents/${d.id}/verifier`, {
        type: choix.type,
        societe_id: choix.societe === '' ? null : Number(choix.societe)
      });
      await charger();
      info = 'Vérification enregistrée.';
    } catch (err) {
      erreur = message(err);
    }
  }

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
          ['Fournisseur', [d.fournisseur_nom, d.fournisseur_siret && `SIRET ${d.fournisseur_siret}`, d.fournisseur_tva && `TVA ${d.fournisseur_tva}`].filter(Boolean).join(' · ') || '—', d.confiance.fournisseur],
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

    <h2>Vérification</h2>
    <form class="carte" onsubmit={verifier}>
      {#if d.verifie_le}
        <p class="discret">
          Vérifié le {quand(d.verifie_le)}{d.verifie_par ? ` par ${d.verifie_par}` : ''}.
          {#if d.type_corrige || d.societe_corrigee}
            Corrigé : {[d.type_corrige && 'type', d.societe_corrigee && 'structure'].filter(Boolean).join(' et ')}.
          {:else}
            Lecture confirmée sans correction.
          {/if}
        </p>
      {:else}
        <p class="discret">Confirmez ou corrigez le type et la structure. Ces vérifications mesurent la justesse de la lecture.</p>
      {/if}
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
      </div>
      <button disabled={!choix.type}>{d.verifie_le ? 'Enregistrer à nouveau' : 'Valider'}</button>
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
  summary { cursor: pointer; font-weight: 650; margin-bottom: 0.6rem; }
</style>
