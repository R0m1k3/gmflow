<script lang="ts">
  import { page } from '$app/state';
  import { api, message } from '#lib/api.ts';
  import { quand, taille } from '#lib/format.ts';
  import type { FicheMail } from '#lib/types.ts';

  let m = $state<FicheMail | null>(null);
  let erreur = $state('');
  $effect(() => {
    api
      .get<FicheMail>(`/api/mails/${page.params.id}`)
      .then((v) => (m = v))
      .catch((e) => (erreur = (e as { code?: number }).code === 404 ? 'Mail introuvable.' : message(e)));
  });
</script>

<p><a href="/mails">← Mails</a></p>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if m}
  <h1>{m.objet || '(sans objet)'}</h1>
  <div class="carte entetes">
    <div><span class="discret">De</span> {m.expediteur || '—'}</div>
    <div><span class="discret">À</span> {m.destinataires.join(', ') || '—'}</div>
    <div><span class="discret">Date</span> <span class="date">{quand(m.date_mail)}</span></div>
    <div><span class="discret">Reçu dans</span> {m.boites.map((b) => `${b.libelle} (${b.dossier})`).join(', ')}</div>
    <div class="discret mid">Message-ID : {m.message_id}</div>
  </div>

  {#if m.pieces.length}
    <h2>Pièces jointes</h2>
    <table>
      <tbody>
        {#each m.pieces as p (p.id)}
          <tr>
            <td><a href="/api/pieces/{p.id}" download>{p.nom}</a></td>
            <td class="discret">{p.type_mime}</td>
            <td class="nombre">{taille(p.taille)}</td>
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
  .entetes .discret:first-child { display: inline-block; min-width: 5.5rem; }
  .entetes span.discret { display: inline-block; min-width: 5.5rem; }
  .mid { word-break: break-all; font-size: 0.8rem; }
</style>
