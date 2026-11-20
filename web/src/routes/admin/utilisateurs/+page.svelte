<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Societe, Utilisateur } from '#lib/types.ts';

  type Form = { id: number | null; identifiant: string; mot_de_passe: string; role: 'admin' | 'responsable'; societe_id: string; actif: boolean };
  let liste = $state<Utilisateur[]>([]);
  let societes = $state<Societe[]>([]);
  let f = $state<Form | null>(null);
  let erreur = $state('');

  async function charger() {
    [liste, societes] = await Promise.all([api.get<Utilisateur[]>('/api/admin/utilisateurs'), api.get<Societe[]>('/api/admin/societes')]);
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
  });
  const nomSociete = (id: number | null) => (id ? (societes.find((s) => s.id === id)?.raison_sociale ?? '?') : '—');

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    const corps = { identifiant: f.identifiant, mot_de_passe: f.mot_de_passe || null, role: f.role, societe_id: f.role === 'responsable' ? Number(f.societe_id) : null, actif: f.actif };
    try {
      if (f.id) await api.put(`/api/admin/utilisateurs/${f.id}`, corps);
      else await api.post('/api/admin/utilisateurs', corps);
      f = null;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<h1>Utilisateurs</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if f}
  <form class="carte" onsubmit={enregistrer}>
    <h2>{f.id ? 'Modifier' : 'Nouvel utilisateur'}</h2>
    <div class="champs">
      <div><label for="i">Identifiant</label><input id="i" bind:value={f.identifiant} required autocomplete="off" /></div>
      <div><label for="m">{f.id ? 'Nouveau mot de passe' : 'Mot de passe'} (12 caractères au moins)</label><input id="m" type="password" minlength="12" bind:value={f.mot_de_passe} required={!f.id} autocomplete="new-password" placeholder={f.id ? 'inchangé' : ''} /></div>
      <div>
        <label for="r">Rôle</label>
        <select id="r" bind:value={f.role}><option value="responsable">Responsable de structure</option><option value="admin">Administrateur</option></select>
      </div>
      {#if f.role === 'responsable'}
        <div>
          <label for="s">Structure</label>
          <select id="s" bind:value={f.societe_id} required>
            <option value="" disabled>Choisir…</option>
            {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
        </div>
      {/if}
    </div>
    <p><label><input type="checkbox" bind:checked={f.actif} /> Compte actif</label></p>
    <div class="actions"><button>Enregistrer</button><button type="button" class="second" onclick={() => (f = null)}>Annuler</button></div>
  </form>
{:else}
  <p><button onclick={() => (f = { id: null, identifiant: '', mot_de_passe: '', role: 'responsable', societe_id: '', actif: true })}>Nouvel utilisateur</button></p>
{/if}

<table>
  <thead><tr><th>Identifiant</th><th>Rôle</th><th>Structure</th><th>État</th><th></th></tr></thead>
  <tbody>
    {#each liste as u (u.id)}
      <tr>
        <td>{u.identifiant}{#if u.id === session.moi?.id} <span class="discret">(vous)</span>{/if}</td>
        <td>{u.role === 'admin' ? 'Administrateur' : 'Responsable'}</td>
        <td>{nomSociete(u.societe_id)}</td>
        <td><span class="etiquette {u.actif ? 'ok' : ''}">{u.actif ? 'actif' : 'désactivé'}</span></td>
        <td><button class="second" onclick={() => (f = { id: u.id, identifiant: u.identifiant, mot_de_passe: '', role: u.role, societe_id: u.societe_id ? String(u.societe_id) : '', actif: u.actif })}>Modifier</button></td>
      </tr>
    {/each}
  </tbody>
</table>
