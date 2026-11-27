<script lang="ts">
  // Sociétés : liste à filets, ajout et modification en place.
  import { tick } from 'svelte';
  import { api, message } from '#lib/api.ts';
  import { RUBRIQUES, nomOutil } from '#lib/libelles.ts';
  import type { EntreeSociete, OutilDepot, Societe } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  let { societes, onchange }: { societes: Societe[]; onchange: () => Promise<void> } = $props();

  type Form = {
    id: number | null;
    raison_sociale: string;
    autres_noms: string;
    siret: string;
    tva: string;
    adresses: string;
    outil_depot: OutilDepot | '';
    adresse_depot: string;
    depot_auto: boolean;
    archivee: boolean;
  };
  const vide = (): Form => ({
    id: null,
    raison_sociale: '',
    autres_noms: '',
    siret: '',
    tva: '',
    adresses: '',
    outil_depot: '',
    adresse_depot: '',
    depot_auto: false,
    archivee: false
  });

  let f = $state<Form | null>(null);
  let erreur = $state('');
  let enCours = $state(false);
  let info = $state('');

  async function focaliser() {
    await tick();
    document.querySelector<HTMLInputElement>('#societes form input')?.focus({ preventScroll: true });
  }
  export function nouvelle() {
    erreur = '';
    info = '';
    f = vide();
    void focaliser();
  }
  export function modifier(id: number) {
    const s = societes.find((x) => x.id === id);
    if (!s) return;
    erreur = '';
    info = '';
    f = {
      id: s.id,
      raison_sociale: s.raison_sociale,
      autres_noms: (s.autres_noms ?? []).join('\n'),
      siret: s.siret ?? '',
      tva: s.tva ?? '',
      adresses: (s.adresses ?? []).join('\n'),
      outil_depot: s.outil_depot ?? '',
      adresse_depot: s.adresse_depot ?? '',
      depot_auto: s.depot_auto,
      archivee: s.archivee
    };
    void focaliser();
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    enCours = true;
    const corps: EntreeSociete = {
      raison_sociale: f.raison_sociale.trim(),
      autres_noms: f.autres_noms
        .split('\n')
        .map((l) => l.trim())
        .filter(Boolean),
      siret: f.siret.trim(),
      tva: f.tva.trim(),
      adresses: f.adresses
        .split('\n')
        .map((l) => l.trim())
        .filter(Boolean),
      outil_depot: f.outil_depot || null,
      adresse_depot: f.adresse_depot.trim(),
      depot_auto: f.depot_auto,
      archivee: f.archivee
    };
    try {
      if (f.id) await api.put(`/api/admin/societes/${f.id}`, corps);
      else await api.post('/api/admin/societes', corps);
      info = f.id ? `${corps.raison_sociale} : enregistrée.` : `${corps.raison_sociale} ajoutée.`;
      f = null;
      await onchange();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  const OUTILS = [
    { valeur: '', libelle: 'Aucun pour l’instant' },
    { valeur: 'dext', libelle: 'Dext' },
    { valeur: 'yooz', libelle: 'Yooz' }
  ];
  let triees = $derived([...societes].sort((a, b) => Number(a.archivee) - Number(b.archivee) || a.raison_sociale.localeCompare(b.raison_sociale)));
</script>

{#snippet formulaire(g: Form)}
  <form class="edition" onsubmit={enregistrer}>
    <h3>{g.id ? `Modifier ${g.raison_sociale || 'la société'}` : 'Nouvelle société'}</h3>
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
    <div class="formulaire">
      <div class="plein"><Champ libelle="Raison sociale" bind:valeur={g.raison_sociale} requis placeholder="SAS Frouard Distribution" /></div>
      <div class="plein">
        <Champ
          libelle="Autres noms"
          type="textarea"
          bind:valeur={g.autres_noms}
          placeholder={'Frouard Distribution\nLa Foir’Fouille Frouard'}
          aide="Un par ligne : les noms que les fournisseurs mettent sur leurs factures (nom complet, enseigne et ville). Écrivez l’enseigne avec la ville : l’enseigne seule ne distingue pas deux magasins."
        />
      </div>
      <Champ libelle="SIRET" bind:valeur={g.siret} mono inputmode="numeric" />
      <Champ libelle="N° de TVA" bind:valeur={g.tva} mono />
      <div class="plein">
        <Champ
          libelle="Adresses"
          type="textarea"
          bind:valeur={g.adresses}
          aide="Une par ligne : siège, magasins. Elles aident à reconnaître la société sur une facture."
        />
      </div>
      <Champ libelle="Outil de collecte" type="select" options={OUTILS} bind:valeur={g.outil_depot} />
      <Champ
        libelle="Adresse de collecte {nomOutil(g.outil_depot)}"
        type="email"
        bind:valeur={g.adresse_depot}
        aide="L’adresse donnée par {nomOutil(g.outil_depot)} pour recevoir les factures de cette société."
      />
      <label class="case plein">
        <input type="checkbox" bind:checked={g.depot_auto} />
        <span>
          {RUBRIQUES.envoiAuto}
          <span class="aide">Les factures sans alerte partent seules vers {nomOutil(g.outil_depot)} ; les autres attendent « Valider ».</span>
        </span>
      </label>
      {#if g.id}
        <label class="case plein">
          <input type="checkbox" bind:checked={g.archivee} />
          <span>
            Archivée
            <span class="aide">Elle n’est plus proposée pour une boîte ou un compte ; ses documents restent.</span>
          </span>
        </label>
      {/if}
    </div>
    <div class="form-f">
      <button class="btn principal" class:en-cours={enCours} disabled={enCours}>{g.id ? 'Enregistrer' : 'Ajouter la société'}</button>
      <button type="button" class="btn fantome" onclick={() => (f = null)}>Annuler</button>
    </div>
  </form>
{/snippet}

<section class="sujet" id="societes">
  <div class="sujet-h">
    <h2>{RUBRIQUES.societes}</h2>
    <span class="sp"></span>
    {#if f?.id !== null || !f}
      <button type="button" class="btn sm" onclick={nouvelle}><Icone nom="plus" />Ajouter une société</button>
    {/if}
  </div>
  <p class="lead">Chaque société reçoit ses factures dans ses boîtes mail et les envoie à son outil de collecte, Dext ou Yooz.</p>
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}

  {#if triees.length || f}
    <div class="carte-r">
      {#if f && f.id === null}{@render formulaire(f)}{/if}
      {#each triees as s (s.id)}
        {#if f && f.id === s.id}
          {@render formulaire(f)}
        {:else}
          <div class="rang" class:inactive={s.archivee}>
            <div class="g">
              <span class="t">
                {s.raison_sociale}
                {#if s.archivee}<Etiquette ton="pale">Archivée</Etiquette>{/if}
              </span>
              {#if s.autres_noms?.length || s.siret}
                <span class="s">
                  {#if s.autres_noms?.length}Aussi : {s.autres_noms.join(', ')}{/if}{#if s.autres_noms?.length && s.siret}{' '}·{' '}{/if}{#if s.siret}SIRET <span class="num">{s.siret}</span>{/if}
                </span>
              {/if}
              {#if s.adresse_depot}
                <span class="s">{nomOutil(s.outil_depot)} : {s.adresse_depot}{#if s.depot_auto}{' '}· envoi automatique{/if}</span>
              {:else}
                <span class="alerte-l"><Icone nom="alerte" />Pas d’adresse de collecte Dext / Yooz</span>
              {/if}
            </div>
            <div class="d"><button type="button" class="lien-a" onclick={() => modifier(s.id)}>Modifier</button></div>
          </div>
        {/if}
      {/each}
    </div>
  {:else}
    <Vide compact icone="batiment" titre="Aucune société" texte="Commencez par la société qui reçoit les factures.">
      <button type="button" class="btn sm" onclick={nouvelle}>Ajouter une société</button>
    </Vide>
  {/if}
</section>

<style>
  .inactive .t {
    color: var(--text2);
  }
  .t :global(.etiquette) {
    margin-left: var(--e2);
  }
</style>
