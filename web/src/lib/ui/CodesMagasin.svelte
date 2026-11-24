<script lang="ts">
  // Fiche fournisseur sans code BO Phénix (admin) : codes du serveur du magasin (Apiflow) les plus
  // ressemblants, et recherche par code ou par nom. « Associer » remplit le code de la fiche.
  // Rien ne s'affiche tant que les fournisseurs du magasin ne sont pas copiés.
  import { api, message, modifierFournisseur } from '#lib/api.ts';
  import type { CodesMagasin } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';

  let { id, onassocie }: { id: number; onassocie: () => Promise<void> | void } = $props();

  let codes = $state<CodesMagasin | null>(null);
  let q = $state('');
  let erreur = $state('');
  let enCours = $state<string | null>(null);

  $effect(() => {
    const cherche = q.trim();
    const fiche = id;
    const t = setTimeout(
      () => {
        api
          .get<CodesMagasin>(`/api/admin/fournisseurs/${fiche}/codes-magasin${cherche ? `?q=${encodeURIComponent(cherche)}` : ''}`)
          .then((c) => {
            if (fiche === id) codes = c;
          })
          .catch((e) => (erreur = message(e)));
      },
      cherche ? 200 : 0
    );
    return () => clearTimeout(t);
  });

  async function associer(code: string) {
    erreur = '';
    enCours = code;
    try {
      await modifierFournisseur(id, { code_gestion: code });
      await onassocie();
    } catch (e) {
      erreur = message(e);
    } finally {
      enCours = null;
    }
  }
</script>

{#if codes && codes.nombre > 0}
  <section class="codes" aria-label="Code BO Phénix à associer">
    <div class="tete">
      <h3>Code BO Phénix à associer</h3>
      <span class="petit muted">{codes.nombre} fournisseurs au magasin</span>
    </div>
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
    <input type="search" bind:value={q} placeholder="Chercher un code ou un nom au magasin" aria-label="Chercher un fournisseur du magasin" />
    {#if codes.suggestions.length}
      <ul>
        {#each codes.suggestions as s (s.code)}
          <li>
            <span class="code num">{s.code}</span>
            <span class="nom">
              {s.nom ?? '—'}
              <span class="petit t2">
                {#if s.raison}{s.raison}{/if}{#if s.raison && (s.email || s.nb_articles)}{' · '}{/if}{#if s.email}{s.email}{:else if s.nb_articles}<span class="num">{s.nb_articles}</span> articles{/if}
              </span>
              {#if s.deja_pris_par}<span class="petit flag w">déjà le code de {s.deja_pris_par}</span>{/if}
            </span>
            <button
              type="button"
              class="btn sm"
              class:principal={!s.deja_pris_par && s.score >= 0.95}
              class:en-cours={enCours === s.code}
              disabled={enCours !== null || !!s.deja_pris_par}
              onclick={() => associer(s.code)}>Associer</button
            >
          </li>
        {/each}
      </ul>
    {:else}
      <p class="petit t2">{q.trim() ? 'Aucun fournisseur du magasin ne correspond.' : 'Aucun fournisseur du magasin ne ressemble à cette fiche : cherchez-le par son code ou son nom.'}</p>
    {/if}
  </section>
{/if}

<style>
  .codes {
    margin: var(--e3) 0;
    border: 1px solid var(--line);
    border-radius: var(--r2, 8px);
    padding: var(--e3);
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .tete {
    display: flex;
    align-items: baseline;
    gap: var(--e2);
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  input {
    height: 32px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--e3);
    padding: var(--e2) 0;
    border-top: 1px solid var(--line);
  }
  .code {
    min-width: 64px;
    font-weight: 600;
  }
  .nom .flag {
    align-self: flex-start;
  }
  .nom {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
</style>
