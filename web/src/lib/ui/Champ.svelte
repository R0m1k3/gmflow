<script lang="ts">
  // Champ de saisie, en deux dispositions :
  // - « bloc » (formulaires, réglages) : libellé au-dessus, saisie, aide dessous ;
  // - « ligne » (fiche document) : clé à gauche, valeur à droite entre deux filets.
  //   Avec `modifiable`, la valeur se lit, et un clic (ou Entrée) ouvre la saisie :
  //   Entrée valide (`onvalider`), Échap annule.
  // `aControler` surligne le champ en ambre (valeur douteuse) ; une chaîne sert d'explication.
  // `erreur` (disposition « bloc ») : valeur refusée, message en rouge sous la saisie.
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';

  type Option = { valeur: string; libelle: string };
  let {
    libelle,
    valeur = $bindable(''),
    disposition = 'bloc',
    type = 'text',
    options = [],
    aide,
    aControler = false,
    erreur,
    modifiable = false,
    mono = false,
    indice,
    source,
    placeholder,
    id: idDonne,
    requis = false,
    desactive = false,
    autocomplete,
    inputmode,
    onvalider,
    affichage,
    apres
  }: {
    libelle: string;
    valeur?: string;
    disposition?: 'bloc' | 'ligne';
    type?: 'text' | 'email' | 'password' | 'date' | 'number' | 'url' | 'select' | 'textarea';
    options?: Option[];
    aide?: string;
    aControler?: boolean | string;
    erreur?: string;
    modifiable?: boolean;
    mono?: boolean;
    indice?: string;
    source?: string;
    placeholder?: string;
    id?: string;
    requis?: boolean;
    desactive?: boolean;
    autocomplete?: HTMLInputAttributes['autocomplete'];
    inputmode?: HTMLInputAttributes['inputmode'];
    onvalider?: (v: string) => void | Promise<void>;
    /** Affichage de la valeur en lecture (par ex. un <Montant>). */
    affichage?: Snippet;
    /** Contenu à droite de la valeur : un bouton .btn.sm « Mettre 2 210,11 € ». */
    apres?: Snippet;
  } = $props();

  const uid = $props.id();
  let id = $derived(idDonne ?? `champ-${uid}`);
  let edition = $state(false);
  let brouillon = $state('');
  let saisie = $state<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>();
  let explication = $derived(typeof aControler === 'string' ? aControler : aide);
  let libelleOption = $derived(options.find((o) => o.valeur === valeur)?.libelle ?? valeur);

  async function ouvrir() {
    if (!modifiable || desactive) return;
    brouillon = valeur;
    edition = true;
    await tick();
    saisie?.focus();
    if (saisie instanceof HTMLInputElement) saisie.select();
  }

  async function valider() {
    if (!edition) return;
    edition = false;
    if (brouillon === valeur) return;
    valeur = brouillon;
    await onvalider?.(brouillon);
  }

  function touche(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      edition = false;
    } else if (e.key === 'Enter' && type !== 'textarea') {
      e.preventDefault();
      e.stopPropagation();
      void valider();
    }
  }
</script>

{#snippet controle(lie: 'valeur' | 'brouillon')}
  {#if type === 'select'}
    {#if lie === 'valeur'}
      <select
        {id}
        bind:value={valeur}
        required={requis}
        disabled={desactive}
        class:num-saisie={mono}
        aria-describedby={erreur ? `${id}-erreur` : undefined}
        aria-invalid={erreur ? 'true' : undefined}
      >
        {#each options as o (o.valeur)}<option value={o.valeur}>{o.libelle}</option>{/each}
      </select>
    {:else}
      <select {id} bind:this={saisie} bind:value={brouillon} onkeydown={touche} onblur={valider} onchange={valider}>
        {#each options as o (o.valeur)}<option value={o.valeur}>{o.libelle}</option>{/each}
      </select>
    {/if}
  {:else if type === 'textarea'}
    {#if lie === 'valeur'}
      <textarea
        {id}
        bind:value={valeur}
        {placeholder}
        required={requis}
        disabled={desactive}
        aria-describedby={erreur ? `${id}-erreur` : undefined}
        aria-invalid={erreur ? 'true' : undefined}
      ></textarea>
    {:else}
      <textarea {id} bind:this={saisie} bind:value={brouillon} onkeydown={touche} onblur={valider}></textarea>
    {/if}
  {:else if lie === 'valeur'}
    <input
      {id}
      {type}
      bind:value={valeur}
      {placeholder}
      {autocomplete}
      {inputmode}
      required={requis}
      disabled={desactive}
      class:num-saisie={mono}
      aria-describedby={erreur ? `${id}-erreur` : explication ? `${id}-aide` : undefined}
      aria-invalid={aControler || erreur ? 'true' : undefined}
    />
  {:else}
    <input
      {id}
      {type}
      bind:this={saisie}
      bind:value={brouillon}
      {placeholder}
      {inputmode}
      class:num-saisie={mono}
      onkeydown={touche}
      onblur={valider}
    />
  {/if}
{/snippet}

{#if disposition === 'bloc'}
  <div class="champ" class:a-controler={!!aControler} class:en-faute={!!erreur}>
    <label for={id}>{libelle}</label>
    {@render controle('valeur')}
    {#if erreur}<div class="erreur-champ" id="{id}-erreur" role="alert">{erreur}</div>{/if}
    {#if explication}<div class="aide" class:alerte={!!aControler} id="{id}-aide">{explication}</div>{/if}
  </div>
{:else}
  <div class="champ-ligne" class:a-controler={!!aControler}>
    {#if edition}
      <label class="k" for={id}>{libelle}</label>
      <div class="v edition">{@render controle('brouillon')}</div>
    {:else}
      <div class="k">{libelle}</div>
      <div class="v" class:modifiable>
        {#if modifiable && !desactive}
          <button type="button" class="val" onclick={ouvrir} aria-label="Modifier {libelle}">
            {@render lecture()}
          </button>
        {:else}
          <span class="val">{@render lecture()}</span>
        {/if}
        {#if apres}{@render apres()}{/if}
      </div>
    {/if}
    {#if explication && !edition}<div class="aide">{explication}</div>{/if}
  </div>
{/if}

{#snippet lecture()}
  {#if affichage}
    {@render affichage()}
  {:else if valeur === '' || valeur === null || valeur === undefined}
    <span class="vide-val">Non lu</span>
  {:else if mono}
    <span class="num">{type === 'select' ? libelleOption : valeur}</span>
  {:else}
    <span>{type === 'select' ? libelleOption : valeur}</span>
  {/if}
  {#if indice}<span class="indice">{indice}</span>{/if}
  {#if source}<span class="source">{source}</span>{/if}
{/snippet}
