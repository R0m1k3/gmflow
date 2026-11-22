<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Societe, Utilisateur } from '#lib/types.ts';

  type Form = { id: number | null; identifiant: string; mot_de_passe: string; role: 'admin' | 'responsable'; societe_id: string; actif: boolean };
  let liste = $state<Utilisateur[]>([]);
  let societes = $state<Societe[]>([]);
  let f = $state<Form | null>(null);
  let erreur = $state('');
  let info = $state('');
  let chargement = $state(true);

  async function charger() {
    try {
      [liste, societes] = await Promise.all([api.get<Utilisateur[]>('/api/admin/utilisateurs'), api.get<Societe[]>('/api/admin/societes')]);
    } finally {
      chargement = false;
    }
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
  });
  const nouveau = (): Form => ({ id: null, identifiant: '', mot_de_passe: '', role: 'responsable', societe_id: '', actif: true });
  const nomSociete = (id: number | null) => (id ? (societes.find((s) => s.id === id)?.raison_sociale ?? 'Non renseignée') : 'Toutes les sociétés');

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    info = '';
    if (f.id && !f.actif && !confirm(`Désactiver le compte « ${f.identifiant} » ? Cette personne ne pourra plus se connecter.`)) return;
    const corps = { identifiant: f.identifiant, mot_de_passe: f.mot_de_passe || null, role: f.role, societe_id: f.role === 'responsable' ? Number(f.societe_id) : null, actif: f.actif };
    try {
      if (f.id) await api.put(`/api/admin/utilisateurs/${f.id}`, corps);
      else await api.post('/api/admin/utilisateurs', corps);
      info = `Compte « ${f.identifiant} » enregistré.`;
      f = null;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<div class="entete">
  <div>
    <h1>Utilisateurs</h1>
    <p class="sous">Les personnes qui peuvent se connecter à GMFlow, et ce qu'elles peuvent voir.</p>
  </div>
  {#if !f}<div class="actions"><button onclick={() => (f = nouveau())}>Ajouter un utilisateur</button></div>{/if}
</div>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if f}
  <form class="pile" onsubmit={enregistrer}>
    <h2 class="sans-marge">{f.id ? `Modifier « ${f.identifiant} »` : 'Nouvel utilisateur'}</h2>
    <fieldset>
      <legend>Connexion</legend>
      <div class="champs">
        <div>
          <label for="i">Identifiant</label>
          <input id="i" bind:value={f.identifiant} required autocomplete="off" />
          <p class="aide">Ce que la personne tape pour se connecter, par exemple « marie ».</p>
        </div>
        <div>
          <label for="m">{f.id ? 'Nouveau mot de passe' : 'Mot de passe'}</label>
          <input id="m" type="password" minlength="12" bind:value={f.mot_de_passe} required={!f.id} autocomplete="new-password" placeholder={f.id ? 'inchangé' : ''} />
          <p class="aide">12 caractères au moins.{f.id ? ' Laissez vide pour garder l\'actuel.' : ''}</p>
        </div>
      </div>
    </fieldset>
    <fieldset>
      <legend>Droits</legend>
      <div class="champs">
        <div>
          <label for="r">Rôle</label>
          <select id="r" bind:value={f.role}><option value="responsable">Responsable de société</option><option value="admin">Administrateur</option></select>
          <p class="aide">Le responsable voit les mails et factures de sa société. L'administrateur voit tout et gère les réglages.</p>
        </div>
        {#if f.role === 'responsable'}
          <div>
            <label for="s">Société</label>
            <select id="s" bind:value={f.societe_id} required>
              <option value="" disabled>Choisir…</option>
              {#each societes as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
            </select>
            <p class="aide">La seule société que cette personne peut consulter.</p>
          </div>
        {/if}
      </div>
      <label class="case"><input type="checkbox" bind:checked={f.actif} /> Compte actif</label>
      <p class="aide">Décochez pour empêcher la connexion sans supprimer le compte.</p>
    </fieldset>
    <div class="actions"><button>Enregistrer</button><button type="button" class="second" onclick={() => (f = null)}>Annuler</button></div>
  </form>
{:else if chargement}
  <div class="charge" aria-busy="true"><span></span></div>
{:else if liste.length === 0}
  <div class="vide">
    <strong>Aucun utilisateur</strong>
    <p>Ajoutez les personnes qui doivent consulter les mails et factures d'une société.</p>
    <div class="actions"><button onclick={() => (f = nouveau())}>Ajouter un utilisateur</button></div>
  </div>
{:else}
  <ul class="liste">
    {#each liste as u (u.id)}
      <li>
        <div class="principal">
          <span>{u.identifiant}{#if u.id === session.moi?.id} <span class="gris">(vous)</span>{/if}</span>
          <span class="discret">{u.role === 'admin' ? 'Administrateur' : 'Responsable'} · {nomSociete(u.societe_id)}</span>
        </div>
        <div class="droite">
          <span class="etiquette {u.actif ? 'ok' : ''}">{u.actif ? 'Actif' : 'Désactivé'}</span>
          <button class="second" onclick={() => (f = { id: u.id, identifiant: u.identifiant, mot_de_passe: '', role: u.role, societe_id: u.societe_id ? String(u.societe_id) : '', actif: u.actif })}>Modifier</button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .sans-marge { margin: 0; }
</style>
