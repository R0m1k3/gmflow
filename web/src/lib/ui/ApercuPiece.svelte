<script lang="ts">
  // Aperçu d'une pièce jointe : PDF dans une iframe, image en <img>, tableur (xlsx, xls, ods,
  // csv) en tableau lu par le serveur (/api/pieces/{id}/tableau), sinon une phrase et le lien de
  // téléchargement. Source : /api/pieces/{id}?inline=1.
  // `carte` (fiche document) : la page posée en carte blanche ombrée sur le fond sauge, sans la barre
  // du lecteur PDF du navigateur ; `zoom` agrandit la carte, `pages` (lu dans le PDF quand c'est
  // possible) et `zoomable` renseignent la barre de la fiche.
  import { api, message } from '#lib/api.ts';
  import Chargement from './Chargement.svelte';
  import Icone from './Icone.svelte';

  interface Feuille {
    nom: string;
    lignes: string[][];
    tronque: boolean;
  }

  let {
    id,
    typeMime,
    nom,
    outils = true,
    carte = false,
    zoom = 1,
    pages = $bindable(null),
    zoomable = $bindable(false)
  }: {
    id: number;
    typeMime: string | null | undefined;
    nom?: string | null;
    outils?: boolean;
    carte?: boolean;
    zoom?: number;
    pages?: number | null;
    zoomable?: boolean;
  } = $props();

  let src = $derived(`/api/pieces/${id}?inline=1`);
  /** Lecteur PDF du navigateur sans sa barre, page à la largeur du cadre (Chrome, Edge ; ignoré ailleurs). */
  let srcCarte = $derived(`${src}#toolbar=0&navpanes=0&view=FitH`);
  let telecharger = $derived(`/api/pieces/${id}`);
  let genre = $derived.by(() => {
    const t = (typeMime ?? '').toLowerCase();
    const ext = (nom ?? '').toLowerCase().split('.').pop() ?? '';
    if (t === 'application/pdf' || t === 'application/x-pdf' || ext === 'pdf') return 'pdf';
    if (/^image\/(png|jpe?g|gif|webp)$/.test(t)) return 'image';
    if (['xlsx', 'xlsm', 'xls', 'ods', 'csv'].includes(ext) || /spreadsheet|excel|csv/.test(t)) return 'tableur';
    return 'autre';
  });
  $effect(() => {
    zoomable = carte && (genre === 'pdf' || genre === 'image');
  });
  // Nombre de pages : le /Count de l'arbre des pages, quand il est lisible en clair (sinon rien).
  $effect(() => {
    const moi = id;
    pages = null;
    if (!carte || genre !== 'pdf') return;
    fetch(src, { credentials: 'same-origin' })
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject()))
      .then((b) => {
        if (moi !== id) return;
        const t = new TextDecoder('latin1').decode(b);
        let n = 0;
        for (const m of t.matchAll(/\/Type\s*\/Pages\b[^>]*?\/Count\s+(\d+)|\/Count\s+(\d+)[^>]*?\/Type\s*\/Pages\b/g)) {
          n = Math.max(n, Number(m[1] ?? m[2]));
        }
        pages = n > 0 ? n : null;
      })
      .catch(() => {});
  });
  let echec = $state(false);
  let feuilles = $state<Feuille[] | null>(null);
  let feuille = $state(0);
  let erreurTableur = $state('');
  $effect(() => {
    void id;
    echec = false;
    feuilles = null;
    feuille = 0;
    erreurTableur = '';
    if (genre !== 'tableur') return;
    const moi = id;
    api
      .get<Feuille[]>(`/api/pieces/${id}/tableau`)
      .then((v) => {
        if (moi === id) feuilles = v;
      })
      .catch((e) => {
        if (moi === id) erreurTableur = message(e);
      });
  });
</script>

{#snippet noteBlanc()}
  <p class="note-blanc petit muted">
    Aperçu blanc ? <a href={src} target="_blank" rel="noopener">Ouvrez le fichier</a> ou <a href={telecharger} download={nom ?? true}>téléchargez-le</a>.
  </p>
{/snippet}

<div class="apercu">
  {#if outils}
    <div class="apercu-outils">
      <Icone nom="fichier" />
      <span class="nom" title={nom ?? undefined}>{nom ?? 'Pièce jointe'}</span>
      {#if genre !== 'autre'}
        <a class="btn sm fantome" href={src} target="_blank" rel="noopener"><Icone nom="ouvrir" />Ouvrir</a>
      {/if}
      <a class="btn sm fantome" href={telecharger} download={nom ?? true}><Icone nom="telecharger" />Télécharger</a>
    </div>
  {/if}
  {#if genre === 'pdf' && !echec && carte}
    <div class="apercu-cadre en-carte">
      <iframe class="page-carte" src={srcCarte} title="Aperçu de {nom ?? 'la pièce'}" style:--zoom={zoom} onerror={() => (echec = true)}></iframe>
    </div>
    {@render noteBlanc()}
  {:else if genre === 'pdf' && !echec}
    <div class="apercu-cadre sans-marge">
      <iframe {src} title="Aperçu de {nom ?? 'la pièce'}" onerror={() => (echec = true)}></iframe>
    </div>
    {@render noteBlanc()}
  {:else if genre === 'tableur' && !erreurTableur}
    {#if !feuilles}
      <Chargement />
    {:else}
      {#if feuilles.length > 1}
        <div class="feuilles" role="tablist" aria-label="Feuilles">
          {#each feuilles as f, i (i)}
            <button type="button" role="tab" aria-selected={i === feuille} onclick={() => (feuille = i)}>{f.nom}</button>
          {/each}
        </div>
      {/if}
      {@const f = feuilles[feuille]}
      {#if f && f.lignes.length}
        <div class="tableur">
          <table>
            <tbody>
              {#each f.lignes as l, i (i)}
                <tr class:entete={i === 0}>
                  <th class="num">{i + 1}</th>
                  {#each l as c, j (j)}<td class:num={/^-?[\d\s.,]+$/.test(c)}>{c}</td>{/each}
                </tr>
              {/each}
            </tbody>
          </table>
          {#if f.tronque}<p class="petit muted marge">Aperçu limité aux 500 premières lignes et 40 colonnes : téléchargez le fichier pour tout voir.</p>{/if}
        </div>
      {:else}
        <p class="muted marge">Feuille vide.</p>
      {/if}
    {/if}
  {:else if genre === 'image' && !echec}
    <div class="apercu-cadre" class:en-carte={carte}>
      <img class:page-carte={carte} style:--zoom={carte ? zoom : undefined} {src} alt="Aperçu de {nom ?? 'la pièce'}" onerror={() => (echec = true)} />
    </div>
  {:else}
    <div class="vide">
      <Icone nom="fichier" />
      <div class="vide-titre">{echec || erreurTableur ? 'Aperçu indisponible' : 'Pas d’aperçu pour ce fichier'}</div>
      <p class="vide-texte">{nom ?? 'La pièce'} s’ouvre dans le logiciel de l’ordinateur.</p>
      <div class="vide-actions"><a class="btn sm" href={telecharger} download={nom ?? true}><Icone nom="telecharger" />Télécharger</a></div>
    </div>
  {/if}
</div>

<style>
  .feuilles {
    flex: none;
    display: flex;
    gap: var(--e1);
    padding: var(--e2) var(--e3) 0;
    border-bottom: 1px solid var(--line);
    overflow-x: auto;
  }
  .feuilles button {
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-bottom: 0;
    border-radius: 4px 4px 0 0;
    background: var(--pane);
    color: var(--text2);
    font-size: 12px;
    white-space: nowrap;
  }
  .feuilles button[aria-selected='true'] {
    background: var(--bg);
    color: var(--text);
    font-weight: 600;
  }
  .tableur {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg);
  }
  table {
    border-collapse: collapse;
    font-size: 12px;
  }
  td,
  th {
    padding: 3px 8px;
    border: 1px solid var(--line);
    white-space: nowrap;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
  }
  td.num {
    text-align: right;
  }
  th {
    position: sticky;
    left: 0;
    background: var(--pane);
    color: var(--muted);
    font-weight: 400;
  }
  tr.entete td {
    position: sticky;
    top: 0;
    background: var(--pane);
    font-weight: 600;
  }
  .marge {
    padding: var(--e3);
  }
  .note-blanc {
    flex: none;
    margin: 0;
    padding: var(--e2) var(--e3);
    text-align: center;
    font-size: 14px;
  }
  .note-blanc a {
    color: var(--accent);
  }
</style>
