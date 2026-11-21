<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import type { Societe } from '#lib/types.ts';

  type Form = { id: number | null; raison_sociale: string; siret: string; tva: string; adresses: string; outil_depot: string; adresse_depot: string; archivee: boolean };
  const vide = (): Form => ({ id: null, raison_sociale: '', siret: '', tva: '', adresses: '', outil_depot: '', adresse_depot: '', archivee: false });

  let liste = $state<Societe[]>([]);
  let f = $state<Form | null>(null);
  let erreur = $state('');

  async function charger() {
    liste = await api.get<Societe[]>('/api/admin/societes');
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
  });

  function editer(s: Societe) {
    f = { id: s.id, raison_sociale: s.raison_sociale, siret: s.siret ?? '', tva: s.tva ?? '', adresses: (s.adresses ?? []).join('\n'), outil_depot: s.outil_depot ?? '', adresse_depot: s.adresse_depot ?? '', archivee: s.archivee };
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    const corps = {
      raison_sociale: f.raison_sociale,
      siret: f.siret,
      tva: f.tva,
      adresses: f.adresses.split('\n').map((l) => l.trim()).filter(Boolean),
      outil_depot: f.outil_depot || null,
      adresse_depot: f.adresse_depot,
      archivee: f.archivee
    };
    try {
      if (f.id) await api.put(`/api/admin/societes/${f.id}`, corps);
      else await api.post('/api/admin/societes', corps);
      f = null;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<h1>Structures</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}

{#if f}
  <form class="carte" onsubmit={enregistrer}>
    <h2>{f.id ? 'Modifier la structure' : 'Nouvelle structure'}</h2>
    <div class="champs">
      <div><label for="rs">Raison sociale</label><input id="rs" bind:value={f.raison_sociale} required /></div>
      <div><label for="siret">SIRET</label><input id="siret" bind:value={f.siret} /></div>
      <div><label for="tva">N° TVA</label><input id="tva" bind:value={f.tva} /></div>
      <div>
        <label for="outil">Outil de dépôt</label>
        <select id="outil" bind:value={f.outil_depot}><option value="">Aucun</option><option value="dext">Dext</option><option value="yooz">Yooz</option></select>
      </div>
      <div><label for="ad">Adresse de dépôt</label><input id="ad" type="email" bind:value={f.adresse_depot} /></div>
    </div>
    <p><label for="adr">Adresses (une par ligne)</label><textarea id="adr" rows="3" bind:value={f.adresses}></textarea></p>
    <p><label><input type="checkbox" bind:checked={f.archivee} /> Archivée</label></p>
    <div class="actions"><button>Enregistrer</button><button type="button" class="second" onclick={() => (f = null)}>Annuler</button></div>
  </form>
{:else}
  <p><button onclick={() => (f = vide())}>Nouvelle structure</button></p>
{/if}

<table>
  <thead><tr><th>Raison sociale</th><th>SIRET</th><th>Dépôt</th><th></th></tr></thead>
  <tbody>
    {#each liste as s (s.id)}
      <tr>
        <td>{s.raison_sociale} {#if s.archivee}<span class="etiquette">archivée</span>{/if}</td>
        <td class="nombre">{s.siret ?? '—'}</td>
        <td>{s.outil_depot ? `${s.outil_depot} · ${s.adresse_depot ?? ''}` : '—'}</td>
        <td><button class="second" onclick={() => editer(s)}>Modifier</button></td>
      </tr>
    {:else}
      <tr><td colspan="4" class="discret">Aucune structure.</td></tr>
    {/each}
  </tbody>
</table>
