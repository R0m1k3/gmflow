<script lang="ts" module>
  import type { DetailRapprochement, FicheDocument } from '#lib/types.ts';
  import { formatMontant } from './Montant.svelte';
  import { texteRemise } from './DocOutils.ts';

  /** Remise de la facture du lien (la fiche, ou la facture liée à un BL) : elle explique l'écart. */
  export function remiseDuLien(d: FicheDocument, rap: DetailRapprochement | null): number | null {
    const r = d.type === 'facture' ? d.remise_ht : (rap?.liens[0]?.autre.remise_ht ?? null);
    return r && r > 0.005 ? r : null;
  }

  /** Écart de montant HT au-delà duquel le lien est signalé (comme le serveur, SEUIL_MONTANT). */
  export const SEUIL_ECART = 0.05;

  /** Ligne de résumé du bloc replié : « 2 liés, 1 à lier, écart de 24,00 € HT ». */
  export function resumeLiens(d: FicheDocument, rap: DetailRapprochement | null): string {
    if (!rap) return '';
    if (rap.ignore && !rap.liens.length) return 'pas de correspondance attendue';
    const bl = d.type === 'facture';
    const morceaux: string[] = [];
    const n = rap.liens.length;
    if (n) morceaux.push(bl ? `${n} lié${n > 1 ? 's' : ''}` : 'liée');
    else morceaux.push(bl ? 'aucun BL lié' : 'aucune facture liée');
    const c = rap.candidats.length;
    if (c) morceaux.push(`${c} à lier`);
    const e = rap.liens[0]?.ecart_montant;
    const r = remiseDuLien(d, rap);
    if (e !== null && e !== undefined && Math.abs(e) > SEUIL_ECART)
      morceaux.push(`écart de ${formatMontant(Math.abs(e))} HT${r ? (Math.abs(e + r) <= SEUIL_ECART ? ', dû à la remise' : ', remise comprise') : ''}`);
    else if (r) morceaux.push('remise sur la facture');
    else if (rap.liens.some((l) => l.ecarts.length)) morceaux.push('écart de quantités');
    return morceaux.join(', ');
  }
</script>

<script lang="ts">
  // Contenu du bloc « BL liés » (facture) ou « Facture liée » (BL) de la fiche document :
  // liens (Délier), candidats du même fournisseur (Lier, L pour le premier), écarts,
  // « Pas de correspondance attendue ». Endpoints de rapprochement existants.
  import { api, message } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { libelleMethode } from '#lib/libelles.ts';
  import type { DocRap } from '#lib/types.ts';
  import { dateFr } from './DocOutils.ts';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import Montant from './Montant.svelte';

  let {
    d,
    rap,
    actif = true,
    onchange,
    onerreur
  }: {
    d: FicheDocument;
    rap: DetailRapprochement;
    /** Raccourci L posé (fiche affichée). */
    actif?: boolean;
    onchange: (info: string) => Promise<void>;
    onerreur: (m: string) => void;
  } = $props();

  let facture = $derived(d.type === 'facture');
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
    const ignore = !rap.ignore;
    void faire('ignore', () => api.post(`/api/documents/${d.id}/rapprochement-ignore`, { ignore }), ignore ? 'Document sorti du rapprochement' : 'Document remis dans le rapprochement');
  }

  /** Pourquoi ce candidat est proposé. */
  function raison(c: DocRap): string {
    if (c.numero_commande && d.numero_commande && c.numero_commande === d.numero_commande) return `même commande ${c.numero_commande}`;
    if (facture && c.numero && rap.numeros_bl.includes(c.numero)) return 'cité dans la facture';
    return 'même fournisseur';
  }

  let ecart = $derived(rap.liens[0]?.ecart_montant ?? null);
  let remise = $derived(remiseDuLien(d, rap));
  let texteR = $derived(facture ? texteRemise(d.remise_ht, d.remise_taux) : texteRemise(remise));
  /** Écart sans la remise : facture HT + remise − BL. */
  let horsRemise = $derived(ecart !== null && remise !== null ? ecart + remise : null);
  let expliquee = $derived(horsRemise !== null && Math.abs(horsRemise) <= SEUIL_ECART);
  let ecartsLignes = $derived(rap.liens.flatMap((l) => l.ecarts));
  const qte = (v: number) => v.toLocaleString('fr-FR');

  $effect(() => {
    if (!actif || !rap.candidats.length) return;
    return raccourcis({ l: () => lier(rap.candidats[0]) });
  });
</script>

{#if facture && rap.numeros_bl.length}
  <p class="t2 petit doc-cites">BL cités par la facture : <span class="num">{rap.numeros_bl.join(', ')}</span></p>
{/if}

{#if rap.liens.length || rap.candidats.length}
  <table>
    <thead>
      <tr><th>{facture ? 'BL' : 'Facture'}</th><th>{facture ? 'Livré le' : 'Date'}</th><th>Lien</th><th class="n">HT</th><th></th></tr>
    </thead>
    <tbody>
      {#each rap.liens as l (l.id)}
        <tr>
          <td><a class="num" href="/documents/{l.autre.id}">{l.autre.numero ?? l.autre.nom_fichier}</a></td>
          <td class="num">{dateFr(l.autre.date_document, true) || '—'}</td>
          <td class="t2">{libelleMethode(l.methode)}{l.auteur ? ` (${l.auteur})` : ''}</td>
          <td class="n"><Montant valeur={l.autre.montant_ht} /></td>
          <td class="n">
            <button type="button" class="btn sm fantome" class:en-cours={occupe === l.id} disabled={occupe !== null} onclick={() => delier(l.id, l.autre)}>Délier</button>
          </td>
        </tr>
      {/each}
      {#each rap.candidats as c, i (c.id)}
        <tr class="cand">
          <td><a class="num" href="/documents/{c.id}">{c.numero ?? c.nom_fichier}</a></td>
          <td class="num">{dateFr(c.date_document, true) || '—'}</td>
          <td class="t2">{raison(c)}</td>
          <td class="n"><Montant valeur={c.montant_ht} /></td>
          <td class="n">
            <button type="button" class="btn sm" class:en-cours={occupe === c.id} disabled={occupe !== null} onclick={() => lier(c)}>
              Lier{#if i === 0}<Kbd t="L" />{/if}
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="t2">
    {#if rap.ignore}
      Pas de correspondance attendue : ce document n’est pas rapproché automatiquement.
    {:else}
      Aucun{facture ? ' BL' : 'e facture'} du même fournisseur à lier pour l’instant.
    {/if}
  </p>
{/if}

{#if (ecart !== null && Math.abs(ecart) > SEUIL_ECART) || ecartsLignes.length}
  <div class="encadre doc-ecart">
    <Icone nom={expliquee && !ecartsLignes.length ? 'info' : 'alerte'} class={expliquee && !ecartsLignes.length ? 'doc-bleu' : 'doc-ambre'} />
    <div>
      {#if ecart !== null && Math.abs(ecart) > SEUIL_ECART}
        <div>
          <b>Écart de <Montant valeur={Math.abs(ecart)} /> HT</b> :
          {ecart > 0 ? `la facture dépasse ${facture ? 'les BL liés' : 'ses BL'}.` : `${facture ? 'les BL liés dépassent' : 'les BL dépassent'} la facture.`}
        </div>
        {#if remise !== null && horsRemise !== null}
          <div class="doc-remise">
            <b>Remise de {texteR}</b> déduite sur la facture.
            {#if expliquee}L’écart vient de la remise : hors remise, les montants concordent.
            {:else}Hors remise, {horsRemise > 0 ? 'la facture dépasse' : 'les BL dépassent'} de <Montant valeur={Math.abs(horsRemise)} /> HT.{/if}
          </div>
        {/if}
      {/if}
      {#each ecartsLignes as e, i (i)}
        <div><b>{e.designation ?? e.reference ?? 'Article'}</b> : <span class="num">{qte(e.qte_facture)}</span> facturé{e.qte_facture > 1 ? 's' : ''}, <span class="num">{qte(e.qte_bl)}</span> livré{e.qte_bl > 1 ? 's' : ''}</div>
      {/each}
      {#if rap.candidats.length}<div class="petit">Lier un candidat peut régler l’écart.</div>{/if}
    </div>
  </div>
{/if}

{#if d.type === 'bl' && !rap.lignes_lues}
  <p class="t2 petit">BL lu sans ses lignes : un administrateur peut le relire avec Claude.</p>
{/if}

<div class="doc-ignore">
  <button type="button" class="btn sm fantome" class:en-cours={occupe === 'ignore'} disabled={occupe !== null} onclick={basculer}>
    {rap.ignore ? 'Remettre dans le rapprochement' : 'Pas de correspondance attendue'}
  </button>
</div>

<style>
  .doc-cites {
    margin: 0 0 var(--e2);
  }
  .doc-ecart {
    margin-top: var(--e3);
  }
  .doc-ecart > div {
    display: grid;
    gap: 2px;
  }
  :global(.doc-bleu) {
    color: var(--accent-ink);
    margin-top: 2px;
  }
  .doc-remise {
    margin-top: var(--e1);
  }
  :global(.doc-ambre) {
    color: var(--warn);
    margin-top: 2px;
  }
  .doc-ignore {
    margin-top: var(--e2);
  }
  td a.num {
    color: inherit;
  }
</style>
