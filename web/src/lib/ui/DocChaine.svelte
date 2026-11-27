<script lang="ts" module>
  import type { DetailRapprochement, DocRap, FicheDocument } from '#lib/types.ts';
  /** Point d'état d'un maillon : vert, ambre ou creux. */
  export type PointChaine = 'ok' | 'alerte' | 'creux';
  /** Facture liée à un BL, lue par ses deux routes (null : lecture en échec). */
  export type FactureLiee = { fiche: FicheDocument | null; rap: DetailRapprochement | null };
</script>

<script lang="ts">
  // Chaîne de la fiche document (proposition C « Récit ») : commande → BL → facture → Phénix /
  // Dext → Logiflow, un point d'état par maillon et les montants dans une même colonne à droite.
  // Un clic sur un maillon le déplie juste dessous (rien ne se referme : `ouverts` est un Set de
  // clés) ; le maillon de la pièce affichée est toujours déplié et porte ses champs de montants
  // (snippet `montantsPiece` de DocFiche) et, sur une facture, l'addition face à ses BL.
  // Lier (L pour le premier candidat), délier, « Pas de correspondance attendue » : endpoints de
  // rapprochement existants.
  import type { Snippet } from 'svelte';
  import { api, message } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { libelleMethode, titreEnvoi } from '#lib/libelles.ts';
  import { additionRapprochement, type Addition } from './addition.ts';
  import { dateFr, phraseEcartLigne, texteRemise, typeNumero } from './DocOutils.ts';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import { formatMontant } from './Montant.svelte';

  let {
    d,
    rap,
    factures,
    ouverts = $bindable(),
    actif = true,
    onchange,
    onerreur,
    lu,
    resumes,
    points,
    onphenix,
    montantsPiece,
    envoi,
    logiflow
  }: {
    d: FicheDocument;
    /** Null si le rapprochement n'a pas pu être lu : ni BL, ni candidats. */
    rap: DetailRapprochement | null;
    /** BL : chacune de ses factures liées (absente : en cours de lecture) ; vide sur une facture. */
    factures: Map<number, FactureLiee>;
    /** Maillons dépliés : `bl-<id>`, `facture-<id>`, `envoi`, `logiflow`, `candidats`. */
    ouverts: Set<string>;
    /** Raccourci L posé (fiche affichée). */
    actif?: boolean;
    onchange: (info: string) => Promise<void>;
    onerreur: (m: string) => void;
    /** HT lu (celui du serveur) ; `corrige` : l'utilisateur l'a changé dans la fiche. */
    lu: { ht: number | null; corrige: boolean };
    resumes: { phenix: string; envoi: string; logiflow: string };
    points: { envoi: PointChaine; logiflow: PointChaine };
    /** Clic sur Phénix : la fiche défile jusqu'à la carte Export Phénix. */
    onphenix: () => void;
    montantsPiece: Snippet;
    /** Contenu du maillon Dext/Yooz (absent : pas de maillon). */
    envoi?: Snippet;
    /** Contenu du maillon Logiflow (absent : pas de maillon). */
    logiflow?: Snippet;
  } = $props();

  let facture = $derived(d.type === 'facture');
  let bl = $derived(d.type === 'bl');
  let courantValide = $derived(!!(d.verifie_le || d.valide_le));

  // ---------- Lier, délier, ignorer (repris de l'ancien DocRapprochement) ----------
  let occupe = $state<number | 'ignore' | null>(null);

  async function faire(cle: number | 'ignore', f: () => Promise<unknown>, info: string) {
    if (occupe !== null) return;
    occupe = cle;
    try {
      await f();
      await onchange(info);
    } catch (e) {
      onerreur(message(e));
    } finally {
      occupe = null;
    }
  }

  function lier(c: DocRap) {
    const [facture_id, bl_id] = facture ? [d.id, c.id] : [c.id, d.id];
    void faire(c.id, () => api.post('/api/rapprochement', { facture_id, bl_id }), `${facture ? 'BL' : 'Facture'} ${c.numero ?? c.nom_fichier} lié${facture ? '' : 'e'}`);
  }
  function delier(lienId: number, autre: DocRap) {
    void faire(lienId, () => api.del(`/api/rapprochement/${lienId}`), `Lien avec ${autre.numero ?? autre.nom_fichier} défait`);
  }
  function basculer() {
    if (!rap) return;
    const ignore = !rap.ignore;
    void faire('ignore', () => api.post(`/api/documents/${d.id}/rapprochement-ignore`, { ignore }), ignore ? 'Document sorti du rapprochement' : 'Document remis dans le rapprochement');
  }

  /** Pourquoi ce candidat est proposé. */
  function raison(c: DocRap): string {
    if (c.numero_commande && d.numero_commande && c.numero_commande === d.numero_commande) return `même commande ${c.numero_commande}`;
    if (facture && c.numero && rap?.numeros_bl.includes(c.numero)) return 'cité dans la facture';
    return 'même fournisseur';
  }

  $effect(() => {
    const r = rap;
    if (!actif || !r?.candidats.length) return;
    return raccourcis({ l: () => lier(r.candidats[0]) });
  });

  // ---------- Dépliage ----------
  function plier(k: string) {
    if (ouverts.has(k)) ouverts.delete(k);
    else ouverts.add(k);
  }

  let candidats = $derived(rap ? (ouverts.has('candidats') ? rap.candidats : rap.candidats.slice(0, 3)) : []);

  // ---------- Addition de la fiche (facture) ----------
  let additionPiece = $derived.by((): Addition | null => {
    if (!facture || !rap?.liens.length || rap.liens[0].ecart_montant === null) return null;
    return additionRapprochement(
      { ht: lu.ht, remise_ht: d.remise_ht, remise_taux: d.remise_taux, annexes_ht: d.annexes_ht },
      rap.liens.map((l) => ({ id: l.autre.id, numero: l.autre.numero, ht: l.autre.montant_ht })),
      rap.liens[0].ecart_montant,
      rap.liens[0].ecarts,
      rap.liens[0].eco_bl
    );
  });
  let remiseTexte = $derived(d.type !== 'bl' ? texteRemise(d.remise_ht, d.remise_taux) : '');
  let annexesTexte = $derived(d.type !== 'bl' && d.annexes_ht && d.annexes_ht > 0.005 ? `${formatMontant(d.annexes_ht)} HT` : '');

  // ---------- BL : ses factures liées et les autres BL de chacune ----------
  /** Addition de la facture liée, à partir de sa fiche et de son rapprochement. */
  function additionFacture(f: FactureLiee): Addition | null {
    if (!f.fiche || !f.rap) return null;
    const l0 = f.rap.liens[0];
    return additionRapprochement(
      { ht: f.fiche.montant_ht, remise_ht: f.fiche.remise_ht, remise_taux: f.fiche.remise_taux, annexes_ht: f.fiche.annexes_ht },
      f.rap.liens.map((l) => ({ id: l.autre.id, numero: l.autre.numero, ht: l.autre.montant_ht })),
      l0?.ecart_montant ?? null,
      l0?.ecarts ?? [],
      l0?.eco_bl ?? null
    );
  }
  /** Vert : déposée et concorde ; ambre : écart de montant ou articles en écart ; creux : sinon (sans comparaison, pas encore déposée). */
  function pointFacture(f: FactureLiee | undefined, a: Addition | null): PointChaine {
    if (!f?.fiche || !a) return 'creux';
    if (a.causes.length || a.verdict === 'article' || a.verdict === 'reste') return 'alerte';
    return f.fiche.statut_depot === 'depose' ? 'ok' : 'creux';
  }
  function etatFacture(f: FicheDocument): string {
    if (f.statut_depot === 'depose') return 'envoyée';
    if (f.valide_le || f.verifie_le) return 'validée';
    return 'à contrôler';
  }
  function verdictCourt(a: Addition): string {
    if (a.verdict === 'concorde') return a.causes.length ? `${a.causes.length} article${a.causes.length > 1 ? 's' : ''} en écart` : 'concorde';
    if (a.verdict === 'article') {
      const p = phraseEcartLigne(a.causes[0]);
      return p.montant !== null && Math.abs(p.montant) > 0.005 ? `${p.article} ${signe('ecart', p.montant)}` : p.article;
    }
    if (a.verdict === 'reste' && a.reste !== null) return `écart ${signe('ecart', a.reste)}`;
    return '';
  }
  /** Les autres BL des factures de ce BL (« même facture »), une fois chacun. */
  let memeFacture = $derived.by(() => {
    const vus = new Set<number>([d.id]);
    const v: DocRap[] = [];
    if (!bl || !rap) return v;
    for (const l of rap.liens) {
      for (const x of factures.get(l.autre.id)?.rap?.liens ?? []) {
        if (vus.has(x.autre.id)) continue;
        vus.add(x.autre.id);
        v.push(x.autre);
      }
    }
    return v;
  });

  const signe = (genre: string, v: number | null) =>
    v === null ? '—' : (genre === 'annexes' || genre === 'autres' || genre === 'ecart') && v > 0.005 ? `+${formatMontant(v, '')}` : formatMontant(v, '');
</script>

{#snippet point(p: PointChaine)}
  <span class="pt {p}" aria-hidden="true"></span>
{/snippet}

{#snippet montantCol(v: number | null | undefined)}
  <span class="mt num">{v === null || v === undefined ? '' : formatMontant(v, '')}</span>
{/snippet}

<!-- Addition d'une facture face à ses BL ; `r` : rapprochement de cette facture (indices). -->
{#snippet additionVue(a: Addition, r: DetailRapprochement, corrige: boolean)}
  <div class="addition">
    {#each a.lignes as l, i (i)}
      <div class="l g-{l.genre}" data-genre={l.genre} data-montant={l.montant ?? ''}>
        <span>{l.libelle}{#if l.note}<span class="t2 petit">{' · '}{l.note}</span>{/if}</span>
        <span class="num">{signe(l.genre, l.montant)}</span>
      </div>
    {/each}
  </div>
  {#if corrige}<p class="t2 petit note">HT corrigé : écart recalculé à la validation.</p>{/if}
  {#if a.verdict === 'concorde'}
    <p class="petit note verdict-ok"><Icone nom="coche" />Les montants concordent.</p>
  {:else if a.verdict === 'sans'}
    <p class="t2 petit note">Pas de comparaison : un montant manque, sur la facture ou sur un BL.</p>
  {:else if a.verdict === 'article'}
    {@const p = phraseEcartLigne(a.causes[0])}
    <p class="petit note verdict-w">Écart : {p.article}, {p.titre.toLowerCase()}.</p>
  {:else}
    <p class="petit note verdict-w">
      {#if !r.lignes.length}Les articles de la facture n’ont pas été lus : « Relire en détail » les lit pour trouver l’article manquant ou le prix changé.
      {:else}Aucun article ne diffère entre les lignes lues : l’écart vient peut-être d’un article non lu, de frais ou d’une remise ligne par ligne.{/if}
    </p>
  {/if}
  {#if (a.verdict === 'article' || a.verdict === 'reste') && r.candidats.length}<p class="t2 petit note">Lier un candidat peut régler l’écart.</p>{/if}
  {#if a.causes.length}
    <ul class="articles">
      {#each a.causes.map(phraseEcartLigne) as x, i (i)}
        <li>
          <span class="flag {x.titre === 'Prix changé' ? 'e' : 'w'}">{x.titre}</span>
          <b>{x.article}</b> : {x.detail}{#if x.montant !== null && Math.abs(x.montant) > 0.005}, <span class="num">{x.montant > 0 ? '+' : ''}{formatMontant(x.montant)}</span> HT{/if}
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

{#snippet maillonCourant()}
  <li class="maillon courant" data-maillon="courant">
    <div class="tete-m">
      {@render point(courantValide ? 'ok' : 'alerte')}
      <span class="quoi">
        <b>{typeNumero(d.type, d.numero)}</b>
        <span class="etiq-m">{bl ? 'ce BL' : 'cette pièce'}</span>
        <span class="sous t2">
          {[d.date_document ? dateFr(d.date_document, true) : '', bl && rap?.lignes.length ? `${rap.lignes.length} article${rap.lignes.length > 1 ? 's' : ''}` : '', d.date_echeance ? `échéance ${dateFr(d.date_echeance, true)}` : '']
            .filter(Boolean)
            .join(' · ')}
        </span>
      </span>
      <span class="mt"></span>
    </div>
    <div class="depli courant-depli">
      {@render montantsPiece()}
      {#if additionPiece && rap}
        {@render additionVue(additionPiece, rap, lu.corrige)}
      {:else if remiseTexte || annexesTexte}
        <p class="annexes-montants">
          {#if remiseTexte}Remise <span class="num">−{remiseTexte}</span>{/if}{#if remiseTexte && annexesTexte}{' · '}{/if}{#if annexesTexte}Éco-part., port <span class="num">{annexesTexte}</span>{/if}
        </p>
      {/if}
    </div>
  </li>
{/snippet}

<!-- BL lié à la facture affichée (`lien`), ou autre BL de la facture d'un BL (« même facture », sans Délier) -->
{#snippet maillonBl(b: DocRap, lien: { id: number; methode: string; auteur: string | null } | null)}
  {@const k = `bl-${b.id}`}
  <li class="maillon" data-maillon={k}>
    <button type="button" class="tete-m clic" aria-expanded={ouverts.has(k)} onclick={() => plier(k)}>
      {@render point('ok')}
      <span class="quoi">
        <b class="lib">{typeNumero('bl', b.numero)}</b>
        {#if !lien}<span class="etiq-m">même facture</span>{/if}
        <span class="sous t2">{[b.date_document ? dateFr(b.date_document, true) : '', lien ? libelleMethode(lien.methode) + (lien.methode === 'manuel' && lien.auteur ? ` (${lien.auteur})` : '') : ''].filter(Boolean).join(' · ')}</span>
      </span>
      {@render montantCol(b.montant_ht)}
    </button>
    {#if ouverts.has(k)}
      <div class="depli">
        <dl class="paires">
          <dt>N°</dt><dd class="num">{b.numero ?? b.nom_fichier}</dd>
          <dt>Date</dt><dd class="num">{b.date_document ? dateFr(b.date_document) : '—'}</dd>
          <dt>Total HT</dt><dd class="num">{formatMontant(b.montant_ht)}</dd>
        </dl>
        <div class="actions-m">
          <a class="btn sm" href="/bl?doc={b.id}"><Icone nom="ouvrir" />Ouvrir le BL</a>
          {#if lien}
            <button type="button" class="btn sm fantome" class:en-cours={occupe === lien.id} disabled={occupe !== null} onclick={() => delier(lien.id, b)}>Délier</button>
          {/if}
        </div>
      </div>
    {/if}
  </li>
{/snippet}

{#snippet groupeBl()}
  {#if rap}
    {#each candidats as c, i (c.id)}
      <li class="maillon cand">
        <div class="tete-m">
          {@render point('creux')}
          <span class="quoi">
            <b>{typeNumero(c.type, c.numero ?? c.nom_fichier)}</b>
            <span class="etiq-m">à lier ?</span>
            <span class="sous t2">{[c.date_document ? dateFr(c.date_document, true) : '', raison(c)].filter(Boolean).join(' · ')}</span>
          </span>
          {@render montantCol(c.montant_ht)}
        </div>
        <div class="actions-m cand-actions">
          <button type="button" class="btn sm" class:en-cours={occupe === c.id} disabled={occupe !== null} onclick={() => lier(c)}>
            Lier{#if i === 0}<Kbd t="L" />{/if}
          </button>
        </div>
      </li>
    {/each}
    {#if rap.candidats.length > 3}
      <li class="pied">
        <button type="button" class="btn-lien" aria-expanded={ouverts.has('candidats')} onclick={() => plier('candidats')}>
          {ouverts.has('candidats') ? 'Montrer les 3 premiers' : `${rap.candidats.length - 3} autre${rap.candidats.length - 3 > 1 ? 's' : ''}`}
        </button>
      </li>
    {/if}
    <li class="pied">
      <button type="button" class="btn sm fantome" class:en-cours={occupe === 'ignore'} disabled={occupe !== null} onclick={basculer}>
        {rap.ignore ? 'Remettre dans le rapprochement' : 'Pas de correspondance attendue'}
      </button>
      {#if facture && rap.numeros_bl.length}
        <p class="t2 petit">BL cités par la facture : <span class="num">{rap.numeros_bl.join(', ')}</span></p>
      {/if}
      {#if bl && !rap.lignes_lues}
        <p class="t2 petit">BL lu sans ses lignes : un administrateur peut le relire avec Claude.</p>
      {/if}
      {#if facture && !rap.liens.length && !rap.candidats.length}
        <p class="t2 petit">
          {rap.ignore ? 'Pas de correspondance attendue : ce document n’est pas rapproché automatiquement.' : 'Aucun BL du même fournisseur à lier pour l’instant.'}
        </p>
      {/if}
    </li>
  {/if}
{/snippet}

<ol class="chaine" aria-label="Chaîne de la pièce">
  {#if d.numero_commande}
    <li class="maillon">
      <div class="tete-m">
        {@render point('creux')}
        <span class="quoi"><b>Commande {d.numero_commande}</b></span>
        <span class="mt"></span>
      </div>
    </li>
  {/if}

  {#if bl}
    {@render maillonCourant()}
    {#each memeFacture as b (b.id)}{@render maillonBl(b, null)}{/each}
    {@render groupeBl()}
    {#if rap}
      {#each rap.liens as l (l.id)}
        {@const k = `facture-${l.autre.id}`}
        {@const f = factures.get(l.autre.id)}
        {@const a = f ? additionFacture(f) : null}
        <li class="maillon" data-maillon={k}>
          <button type="button" class="tete-m clic" aria-expanded={ouverts.has(k)} onclick={() => plier(k)}>
            {@render point(pointFacture(f, a))}
            <span class="quoi">
              <b class="lib">{typeNumero(l.autre.type, l.autre.numero)}</b>
              <span class="sous t2">
                {#if !f}…{:else}{[l.autre.date_document ? dateFr(l.autre.date_document, true) : '', f.fiche ? etatFacture(f.fiche) : '', a ? verdictCourt(a) : ''].filter(Boolean).join(' · ')}{/if}
              </span>
            </span>
            {@render montantCol(f?.fiche ? f.fiche.montant_ht : l.autre.montant_ht)}
          </button>
          {#if ouverts.has(k)}
            <div class="depli">
              {#if !f}
                <p class="t2 petit note">Lecture de la facture…</p>
              {:else if a && f.fiche && f.rap}
                {@render additionVue(a, f.rap, false)}
                <p class="t2 petit note">
                  {[f.fiche.montant_tva !== null ? `TVA ${formatMontant(f.fiche.montant_tva)}` : '', f.fiche.montant_ttc !== null ? `TTC ${formatMontant(f.fiche.montant_ttc)}` : '', f.fiche.date_echeance ? `échéance ${dateFr(f.fiche.date_echeance)}` : '', etatFacture(f.fiche)]
                    .filter(Boolean)
                    .join(' · ')}
                </p>
              {:else}
                <p class="t2 petit note">Détail indisponible.</p>
                <dl class="paires">
                  <dt>N°</dt><dd class="num">{l.autre.numero ?? l.autre.nom_fichier}</dd>
                  <dt>Date</dt><dd class="num">{l.autre.date_document ? dateFr(l.autre.date_document) : '—'}</dd>
                  <dt>Total HT</dt><dd class="num">{formatMontant(l.autre.montant_ht)}</dd>
                </dl>
              {/if}
              <div class="actions-m">
                <a class="btn sm" href="/factures?doc={l.autre.id}"><Icone nom="ouvrir" />Ouvrir la facture</a>
                <button type="button" class="btn sm fantome" class:en-cours={occupe === l.id} disabled={occupe !== null} onclick={() => delier(l.id, l.autre)}>Délier</button>
              </div>
            </div>
          {/if}
        </li>
      {:else}
        <li class="maillon sans-facture">
          <div class="tete-m">
            {@render point('creux')}
            <span class="quoi t2">{rap.ignore ? 'Pas de facture attendue' : 'Pas encore de facture'}</span>
            <span class="mt"></span>
          </div>
        </li>
      {/each}
    {/if}
    <li class="maillon">
      <button type="button" class="tete-m clic" onclick={onphenix}>
        {@render point('creux')}
        <span class="quoi"><b class="lib">Phénix</b>{#if resumes.phenix}<span class="sous t2">{resumes.phenix}</span>{/if}</span>
        <span class="mt"></span>
      </button>
    </li>
  {:else}
    {#if facture && rap}
      {#each rap.liens as l (l.id)}{@render maillonBl(l.autre, l)}{/each}
      {@render groupeBl()}
    {/if}
    {@render maillonCourant()}
  {/if}

  {#if envoi}
    <li class="maillon" data-maillon="envoi">
      <button type="button" class="tete-m clic" aria-expanded={ouverts.has('envoi')} onclick={() => plier('envoi')}>
        {@render point(points.envoi)}
        <span class="quoi"><b class="lib">{titreEnvoi(d.societe_outil_depot)}</b><span class="sous t2">{resumes.envoi}</span></span>
        <span class="mt"></span>
      </button>
      {#if ouverts.has('envoi')}<div class="depli">{@render envoi()}</div>{/if}
    </li>
  {/if}
  {#if logiflow}
    <li class="maillon" data-maillon="logiflow">
      <button type="button" class="tete-m clic" aria-expanded={ouverts.has('logiflow')} onclick={() => plier('logiflow')}>
        {@render point(points.logiflow)}
        <span class="quoi"><b class="lib">Logiflow</b><span class="sous t2">{resumes.logiflow}</span></span>
        <span class="mt"></span>
      </button>
      {#if ouverts.has('logiflow')}<div class="depli">{@render logiflow()}</div>{/if}
    </li>
  {/if}
</ol>

<style>
  .chaine {
    container: chaine / inline-size;
    position: relative;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* filet vertical qui relie les points */
  .chaine::before {
    content: '';
    position: absolute;
    left: 8px;
    top: 14px;
    bottom: 14px;
    width: 2px;
    background: #d5dbd7;
  }
  .maillon {
    position: relative;
  }
  .tete-m {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) 7rem;
    column-gap: 10px;
    align-items: baseline;
    width: 100%;
    padding: 6px 0;
    margin: 0;
    border: 0;
    background: none;
    font: inherit;
    font-size: 15px;
    color: inherit;
    text-align: left;
  }
  button.tete-m {
    cursor: pointer;
    border-radius: 8px;
  }
  button.tete-m:hover .lib {
    text-decoration: underline;
  }
  button.tete-m:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .lib {
    color: var(--accent-ink);
    font-weight: 600;
  }
  .pt {
    align-self: start;
    position: relative;
    z-index: 1;
    width: 10px;
    height: 10px;
    margin: 5px 0 0 4px;
    border-radius: 50%;
    box-sizing: border-box;
    box-shadow: 0 0 0 3px var(--white);
  }
  .pt.ok {
    background: var(--accent);
  }
  .pt.alerte {
    background: var(--warn-line);
  }
  .pt.creux {
    background: var(--white);
    border: 2px solid var(--muted);
  }
  .quoi {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 6px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .quoi b {
    font-weight: 600;
  }
  .sous {
    font-size: 14px;
  }
  @container chaine (max-width: 360px) {
    .sous {
      flex-basis: 100%;
    }
  }
  .etiq-m {
    font-size: 13px;
    color: var(--text2);
    background: var(--sunk);
    border-radius: var(--r-etiq);
    padding: 0 6px;
    white-space: nowrap;
  }
  .mt {
    text-align: right;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  /* pièce affichée : toujours dépliée, sur fond vert clair */
  .courant {
    background: var(--accent-clair);
    border-radius: var(--r2);
    margin: 2px -8px;
    padding: 0 8px 8px;
  }
  .courant .pt {
    box-shadow: 0 0 0 3px var(--accent-clair);
  }
  .courant .etiq-m {
    background: var(--white);
  }
  .depli {
    margin: 0 0 var(--e2) 28px;
    font-size: 15px;
  }
  .depli:not(.courant-depli) {
    border: 1.5px solid var(--accent);
    border-radius: var(--r3);
    padding: 10px 12px;
    background: var(--white);
  }
  .cand > .tete-m {
    border: 1px dashed var(--champ);
    border-radius: var(--r2);
    margin: 2px -8px 0;
    padding: 6px 8px;
    width: calc(100% + 16px);
  }
  .cand-actions {
    margin: 4px 0 var(--e1) 28px;
  }
  .sans-facture .quoi {
    font-size: 15px;
  }
  .pied {
    margin-left: 28px;
    padding: 4px 0 6px;
    display: grid;
    gap: var(--e1);
    justify-items: start;
  }
  .pied p {
    margin: 0;
  }
  .paires {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px var(--e3);
    margin: 0;
  }
  .paires dt {
    color: var(--text2);
  }
  .paires dd {
    margin: 0;
    text-align: right;
  }
  .actions-m {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
    margin-top: var(--e2);
  }
  .addition {
    display: grid;
    margin-top: var(--e2);
    font-size: 15px;
  }
  .addition .l {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 7rem;
    column-gap: 10px;
    padding: 2px 0;
  }
  .addition .l > :last-child {
    text-align: right;
    white-space: nowrap;
  }
  .addition .g-bl,
  .addition .g-autres,
  .addition .g-remise,
  .addition .g-annexes {
    color: var(--text2);
  }
  .addition .g-somme,
  .addition .g-attendu {
    border-top: 1px solid #d5dbd7;
    font-weight: 600;
  }
  .addition .g-facture {
    font-weight: 600;
  }
  .addition .g-ecart {
    border-top: 1px solid #d5dbd7;
    font-weight: 700;
  }
  .note {
    margin: var(--e1) 0 0;
  }
  .verdict-ok {
    color: var(--ok);
    display: flex;
    align-items: center;
    gap: var(--e1);
  }
  .verdict-w {
    color: var(--warn);
  }
  .articles {
    list-style: none;
    margin: var(--e2) 0 0;
    padding: 0;
    display: grid;
    gap: var(--e1);
    font-size: 15px;
  }
  .articles .flag {
    margin-right: var(--e1);
  }
</style>
