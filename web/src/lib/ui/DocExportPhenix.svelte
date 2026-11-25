<script lang="ts" module>
  import type { EtatExportPhenix } from '#lib/types.ts';

  /** Résumé du bloc replié : « 12 articles, 2 lots détaillés, 3 alertes ». */
  export function resumeExport(e: EtatExportPhenix | null): string {
    if (!e) return '';
    if (e.en_cours) return 'recherche au magasin en cours';
    if (e.erreur) return 'recherche au magasin en échec';
    const m = [`${e.lignes} ligne${e.lignes > 1 ? 's' : ''}`];
    if (e.lots) m.push(`${e.lots} lot${e.lots > 1 ? 's' : ''} détaillé${e.lots > 1 ? 's' : ''}`);
    const n = alertes(e).reduce((t, g) => t + g.messages.length, 0);
    if (n) m.push(`${n} alerte${n > 1 ? 's' : ''}`);
    return m.join(', ');
  }

  /** Alertes des deux fichiers, sans doublon, regroupées par ligne du BL (générales d'abord). */
  export function alertes(e: EtatExportPhenix) {
    const groupes = new Map<number, { rang: number | null; article: string | null; messages: string[] }>();
    for (const a of [...e.matrice.alertes, ...e.trame.alertes]) {
      const k = a.rang ?? -1;
      const g = groupes.get(k) ?? { rang: a.rang, article: a.article, messages: [] };
      if (!g.messages.includes(a.message)) g.messages.push(a.message);
      groupes.set(k, g);
    }
    return [...groupes.entries()].sort(([a], [b]) => a - b).map(([, g]) => g);
  }
</script>

<script lang="ts">
  // Bloc « Export Phénix » d'une fiche BL : recherche des articles au magasin (lots détaillés),
  // alertes, et les deux fichiers : matrice d'intégration (commande) et trame BO Phénix
  // (fiches produits). Endpoints /api/documents/{id}/export-phenix.
  import { onDestroy } from 'svelte';
  import { api, message, sonder } from '#lib/api.ts';
  import Bandeau from './Bandeau.svelte';
  import { jourHeure } from './DocOutils.ts';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';

  let {
    id,
    etat = $bindable(null),
    onerreur
  }: { id: number; etat?: EtatExportPhenix | null; onerreur: (m: string) => void } = $props();

  let demande = $state(false);
  let arret: (() => void) | null = null;

  async function charger() {
    etat = await api.get<EtatExportPhenix>(`/api/documents/${id}/export-phenix`);
    return etat.en_cours;
  }

  function suivre() {
    arret?.();
    arret = sonder(charger, 2500);
  }

  $effect(() => {
    void id;
    charger()
      .then((enCours) => enCours && suivre())
      .catch((e) => onerreur(message(e)));
  });
  onDestroy(() => arret?.());

  async function verifier() {
    demande = true;
    try {
      await api.post(`/api/documents/${id}/export-phenix/verifier`);
      await charger();
      suivre();
    } catch (e) {
      onerreur(message(e));
    } finally {
      demande = false;
    }
  }

  let liste = $derived(etat ? alertes(etat) : []);
  /** Les deux fichiers : une ligne chacun, le nombre de lignes seulement s'il diffère de celui du BL (lots détaillés). */
  let fichiers = $derived.by(() => {
    if (!etat) return [];
    const e = etat;
    const n = (r: number) => (r !== e.lignes ? `, ${r} ligne${r > 1 ? 's' : ''}` : '');
    return [
      { cle: 'matrice', nom: 'Matrice d’intégration', detail: `la commande${n(e.matrice.rangees)} · fournisseur choisi dans Phénix` },
      { cle: 'trame', nom: 'Trame BO Phénix', detail: `fiches produits${n(e.trame.rangees)}${e.code_fournisseur ? ` · fournisseur ${e.code_fournisseur}` : ''}` }
    ];
  });
</script>

{#if etat}
  <div class="export">
    {#if etat.en_cours}
      <Bandeau ton="info" role="status">Recherche des articles au magasin en cours…</Bandeau>
    {:else if etat.erreur}
      <Bandeau ton="erreur" titre="Recherche au magasin impossible.">
        {etat.erreur}
        {#snippet action()}<button type="button" class="btn sm" onclick={verifier} disabled={demande}>Relancer</button>{/snippet}
      </Bandeau>
    {:else if etat.serveur && !etat.verifie_le}
      <Bandeau ton="attention" titre="Articles pas encore cherchés au magasin.">
        Les lots ne sont détaillés qu’après cette recherche.
        {#snippet action()}<button type="button" class="btn sm" onclick={verifier} disabled={demande}>Chercher au magasin</button>{/snippet}
      </Bandeau>
    {/if}
    {#if etat.a_relire}
      <Bandeau ton="attention">BL lu avant l’export : ni gencod ni colisage. « Relire en détail » pour les lire, s’ils sont imprimés.</Bandeau>
    {/if}

    <div class="fichiers">
      {#each fichiers as f (f.cle)}
        {#if etat.pret}
          <a class="fichier" href="/api/documents/{id}/export-phenix/{f.cle}" download>
            <Icone nom="fichier" /><span class="txt"><span class="nom">{f.nom}</span><span class="det">{f.detail}</span></span><Icone nom="telecharger" class="dl" />
          </a>
        {:else}
          <span class="fichier desactive" aria-disabled="true">
            <Icone nom="fichier" /><span class="txt"><span class="nom">{f.nom}</span><span class="det">{f.detail}</span></span><Icone nom="telecharger" class="dl" />
          </span>
        {/if}
      {/each}
    </div>
    {#if etat.verifie_le && !etat.erreur}
      <p class="etat-magasin" title="Lot présumé : « lot » ou un multiple (x2, 3X…) dans la désignation. Seules ces lignes sont cherchées au magasin.">
        {#if etat.cherchees}
          <Etiquette ton={etat.trouves < etat.cherchees ? 'alerte' : 'ok'}>{etat.cherchees} lot{etat.cherchees > 1 ? 's' : ''} présumé{etat.cherchees > 1 ? 's' : ''}</Etiquette>
          <span>{etat.trouves} retrouvé{etat.trouves > 1 ? 's' : ''}{etat.lots ? `, ${etat.lots} remplacé${etat.lots > 1 ? 's' : ''} par ${etat.composants} composants` : ''} · {jourHeure(etat.verifie_le)}</span>
        {:else}
          <Etiquette ton="ok">Aucun lot</Etiquette>
          <span>tout vient du BL · {jourHeure(etat.verifie_le)}</span>
        {/if}
        <button type="button" class="btn-lien" onclick={verifier} disabled={demande || etat.en_cours}>Rechercher</button>
      </p>
    {/if}
    {#if liste.length}
      <ul class="alertes">
        {#each liste as g (g.rang ?? -1)}
          <li>
            {#if g.rang !== null}<span class="quelle">Ligne {g.rang + 1}{g.article ? `, ${g.article}` : ''}</span>{' : '}{/if}{g.messages.join(' ; ')}.
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .export :global(.bandeau) {
    margin-bottom: var(--e2);
  }
  .fichiers {
    display: grid;
    gap: var(--e2);
  }
  .fichier {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    padding: var(--e2) 10px;
    color: var(--text);
    text-decoration: none;
  }
  a.fichier:hover {
    background: var(--bg);
  }
  .fichier > :global(svg.i:first-child) {
    color: var(--text2);
  }
  .fichier :global(.dl) {
    margin-left: auto;
    color: var(--accent);
  }
  .fichier.desactive {
    background: var(--desact-bg);
    color: var(--desact);
  }
  .fichier.desactive :global(.dl) {
    color: var(--desact);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nom {
    font-weight: 600;
  }
  .det {
    font-size: 14px;
    color: var(--muted);
  }
  .etat-magasin {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px var(--e2);
    margin: var(--e3) 0 0;
    font-size: 14px;
    color: var(--text2);
  }
  .etat-magasin .btn-lien {
    margin-left: auto;
  }
  .alertes {
    margin: var(--e3) 0 0;
    padding-left: 18px;
    font-size: 15px;
    color: var(--text2);
  }
  .alertes li {
    margin-bottom: 2px;
  }
  .quelle {
    color: var(--text);
    font-weight: 500;
  }
</style>
