<script lang="ts">
  // Utilisateurs : administrateurs et responsables de magasin, ajout et modification en place.
  import { tick } from 'svelte';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES, libelleRole, nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import type { Role, Societe, Utilisateur } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';

  let {
    utilisateurs,
    societes,
    onchange
  }: { utilisateurs: Utilisateur[]; societes: Societe[]; onchange: () => Promise<void> } = $props();

  type Form = { id: number | null; identifiant: string; mot_de_passe: string; role: string; societe_id: string; actif: boolean };
  let f = $state<Form | null>(null);
  let erreur = $state('');
  let info = $state('');
  let enCours = $state(false);

  async function focaliser() {
    await tick();
    document.querySelector<HTMLInputElement>('#utilisateurs form input')?.focus({ preventScroll: true });
  }
  export function nouveau() {
    erreur = '';
    info = '';
    // Un seul choix possible : la société sans responsable vient en premier.
    const sans = societes.find((s) => !s.archivee && !utilisateurs.some((u) => u.actif && u.role === 'responsable' && u.societe_id === s.id));
    f = { id: null, identifiant: '', mot_de_passe: '', role: 'responsable', societe_id: sans ? String(sans.id) : '', actif: true };
    void focaliser();
  }
  function modifier(u: Utilisateur) {
    erreur = '';
    info = '';
    f = { id: u.id, identifiant: u.identifiant, mot_de_passe: '', role: u.role, societe_id: u.societe_id ? String(u.societe_id) : '', actif: u.actif };
    void focaliser();
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    if (f.role === 'responsable' && !f.societe_id) {
      erreur = 'Choisissez la société du responsable.';
      return;
    }
    enCours = true;
    const corps = {
      identifiant: f.identifiant.trim(),
      mot_de_passe: f.mot_de_passe || null,
      role: f.role as Role,
      societe_id: f.role === 'responsable' ? Number(f.societe_id) : null,
      actif: f.actif
    };
    try {
      if (f.id) await api.put(`/api/admin/utilisateurs/${f.id}`, corps);
      else await api.post('/api/admin/utilisateurs', corps);
      info = f.id ? `${corps.identifiant} : enregistré.` : `${corps.identifiant} ajouté : il peut se connecter.`;
      f = null;
      await onchange();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  const ROLES = [
    { valeur: 'responsable', libelle: libelleRole('responsable') },
    { valeur: 'admin', libelle: libelleRole('admin') }
  ];
  let choixSocietes = $derived([
    { valeur: '', libelle: 'Choisir…' },
    ...societes.filter((s) => !s.archivee || String(s.id) === f?.societe_id).map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))
  ]);
  const nomSociete = (id: number | null) => (id ? nomCourtSociete(societes.find((s) => s.id === id)?.raison_sociale) || 'Société inconnue' : '');
  let tries = $derived(
    [...utilisateurs].sort((a, b) => Number(b.actif) - Number(a.actif) || a.role.localeCompare(b.role) || a.identifiant.localeCompare(b.identifiant))
  );
</script>

{#snippet formulaire(g: Form)}
  <form class="edition" onsubmit={enregistrer}>
    <h3>{g.id ? `Modifier ${g.identifiant}` : 'Nouvel utilisateur'}</h3>
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
    <div class="formulaire">
      <Champ libelle="Identifiant" bind:valeur={g.identifiant} requis autocomplete="off" aide="Une adresse mail ou un nom court." />
      <Champ
        libelle={g.id ? 'Nouveau mot de passe' : 'Mot de passe'}
        type="password"
        bind:valeur={g.mot_de_passe}
        requis={!g.id}
        autocomplete="new-password"
        placeholder={g.id ? 'Laisser vide pour le garder' : ''}
        aide="12 caractères au moins."
      />
      <Champ libelle="Rôle" type="select" options={ROLES} bind:valeur={g.role} aide={g.role === 'admin' ? 'Voit toutes les sociétés et les réglages.' : 'Ne voit que sa société, sans les réglages.'} />
      {#if g.role === 'responsable'}
        <Champ libelle="Société" type="select" options={choixSocietes} bind:valeur={g.societe_id} requis />
      {/if}
      <label class="case plein">
        <input type="checkbox" bind:checked={g.actif} disabled={g.id === session.moi?.id} />
        <span>Compte actif<span class="aide">Un compte désactivé ne peut plus se connecter ; son historique reste.</span></span>
      </label>
    </div>
    <div class="form-f">
      <button class="btn principal" class:en-cours={enCours} disabled={enCours}>{g.id ? 'Enregistrer' : 'Ajouter l’utilisateur'}</button>
      <button type="button" class="btn fantome" onclick={() => (f = null)}>Annuler</button>
    </div>
  </form>
{/snippet}

<section class="sujet" id="utilisateurs">
  <div class="sujet-h">
    <h2>{RUBRIQUES.utilisateurs}</h2>
    <span class="sp"></span>
    {#if !f || f.id !== null}
      <button type="button" class="btn sm" onclick={nouveau}><Icone nom="plus" />Ajouter un utilisateur</button>
    {/if}
  </div>
  <p class="lead">Un responsable de magasin voit les factures et les mails de sa société ; un administrateur voit tout, réglages compris.</p>
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  <div class="filets">
    {#if f && f.id === null}{@render formulaire(f)}{/if}
    {#each tries as u (u.id)}
      {#if f && f.id === u.id}
        {@render formulaire(f)}
      {:else}
        <div class="ligne bloc fixe" class:inactif={!u.actif}>
          <div class="a">{u.identifiant}{#if u.id === session.moi?.id}{' '}<span class="muted">(vous)</span>{/if}</div>
          <div class="b">
            {libelleRole(u.role)}{#if u.role === 'responsable'}{' '}· {nomSociete(u.societe_id)}{/if} · créé le <span class="num">{quand(u.cree_le)}</span>
          </div>
          <div class="r droite">
            {#if !u.actif}<Etiquette ton="pale">Désactivé</Etiquette>{/if}
            <button type="button" class="btn sm fantome" onclick={() => modifier(u)}>Modifier</button>
          </div>
        </div>
      {/if}
    {/each}
  </div>
</section>

<style>
  .droite {
    display: flex;
    align-items: center;
    gap: var(--e3);
  }
  .inactif .a {
    color: var(--text2);
  }
</style>
