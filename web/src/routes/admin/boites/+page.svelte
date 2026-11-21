<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { Boite, Societe } from '#lib/types.ts';

  let boites = $state<Boite[]>([]);
  let societes = $state<Societe[]>([]);
  let nouvelle = $state<{ societe_id: string; libelle: string; serveur: string; port: number; identifiant: string; mot_de_passe: string; reprise_mois: number } | null>(null);
  let erreur = $state('');

  async function charger() {
    [boites, societes] = await Promise.all([api.get<Boite[]>('/api/admin/boites'), api.get<Societe[]>('/api/admin/societes')]);
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
  });
  const nomSociete = (id: number) => societes.find((s) => s.id === id)?.raison_sociale ?? '?';

  async function creer(e: SubmitEvent) {
    e.preventDefault();
    if (!nouvelle) return;
    erreur = '';
    try {
      const b = await api.post<Boite>('/api/admin/boites', { ...nouvelle, societe_id: Number(nouvelle.societe_id), dossiers: ['INBOX'] });
      await goto(`/admin/boites/${b.id}`);
    } catch (err) {
      erreur = message(err);
    }
  }

  function etat(b: Boite): [string, string] {
    if (b.pause_auto) return ['pause automatique', 'ko'];
    if (!b.active) return ['inactive', ''];
    if (b.erreur_depuis) return ['en erreur', 'att'];
    return ['active', 'ok'];
  }
</script>

<h1>Boîtes surveillées</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}

{#if nouvelle}
  <form class="carte" onsubmit={creer}>
    <h2>Nouvelle boîte</h2>
    <p class="discret">IMAPS (port 993). Utilisez un mot de passe d'application quand l'hébergeur en propose.</p>
    <div class="champs">
      <div>
        <label for="s">Structure</label>
        <select id="s" bind:value={nouvelle.societe_id} required>
          <option value="" disabled>Choisir…</option>
          {#each societes.filter((s) => !s.archivee) as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
        </select>
      </div>
      <div><label for="l">Libellé</label><input id="l" bind:value={nouvelle.libelle} required placeholder="Compta Frouard" /></div>
      <div><label for="srv">Serveur IMAP</label><input id="srv" bind:value={nouvelle.serveur} required placeholder="imap.exemple.fr" /></div>
      <div><label for="p">Port</label><input id="p" type="number" bind:value={nouvelle.port} /></div>
      <div><label for="i">Identifiant</label><input id="i" bind:value={nouvelle.identifiant} required autocomplete="off" /></div>
      <div><label for="m">Mot de passe</label><input id="m" type="password" bind:value={nouvelle.mot_de_passe} required autocomplete="new-password" /></div>
      <div><label for="r">Reprise de l'historique (mois)</label><input id="r" type="number" min="0" max="24" bind:value={nouvelle.reprise_mois} /></div>
    </div>
    <div class="actions"><button>Créer puis tester</button><button type="button" class="second" onclick={() => (nouvelle = null)}>Annuler</button></div>
  </form>
{:else}
  <p><button disabled={societes.length === 0} onclick={() => (nouvelle = { societe_id: '', libelle: '', serveur: '', port: 993, identifiant: '', mot_de_passe: '', reprise_mois: 3 })}>Nouvelle boîte</button>
    {#if societes.length === 0}<span class="discret">Créez d'abord une <a href="/admin/structures">structure</a>.</span>{/if}</p>
{/if}

<table>
  <thead><tr><th>Boîte</th><th>Structure</th><th>État</th><th>Dernière relève</th><th>Dossiers</th></tr></thead>
  <tbody>
    {#each boites as b (b.id)}
      {@const [texte, classe] = etat(b)}
      <tr class="cliquable" onclick={() => goto(`/admin/boites/${b.id}`)}>
        <td><a href="/admin/boites/{b.id}">{b.libelle}</a><div class="discret">{b.identifiant}</div></td>
        <td>{nomSociete(b.societe_id)}</td>
        <td><span class="etiquette {classe}">{texte}</span></td>
        <td class="date">{quand(b.derniere_releve)}</td>
        <td class="discret">{b.dossiers.join(', ')}</td>
      </tr>
    {:else}
      <tr><td colspan="5" class="discret">Aucune boîte.</td></tr>
    {/each}
  </tbody>
</table>
