<script lang="ts">
  // Corriger les lignes articles d'une pièce lue (audit L16). Fermé : un bouton « Corriger les
  // lignes ». Ouvert sur ordinateur : un tableau aux cellules modifiables, Tab passe à la case
  // suivante, Entrée enregistre la ligne et descend, Échap défait la ligne en cours. Sur téléphone
  // et dans le volet étroit de la fiche : une ligne touchée s'ouvre en petit formulaire. Chaque ligne s'enregistre seule, avec son état
  // (« Enregistrée », ou l'erreur sous la ligne). Le serveur recalcule ce qui en dépend (montant,
  // totaux d'un BL, écarts avec la facture, export Phénix).
  import { ajouterLigne, ErreurApi, lignesDocument, message, modifierLigne, supprimerLigne } from '#lib/api.ts';
  import type { LigneEdition, ResultatLigne, SaisieLigne, TypeDocument } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Confirmation from './Confirmation.svelte';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';

  let {
    documentId,
    typeDoc,
    corrigees = 0,
    onchange
  }: {
    documentId: number;
    typeDoc: TypeDocument | null;
    /** Lignes déjà corrigées (fiche), affiché fermé. */
    corrigees?: number;
    /** Une ligne a changé : relire la fiche et le rapprochement. */
    onchange?: () => void;
  } = $props();

  type Col = { cle: keyof SaisieLigne; libelle: string; num?: boolean; bl?: boolean; large?: boolean };
  const COLONNES: Col[] = [
    { cle: 'reference', libelle: 'Réf.' },
    { cle: 'designation', libelle: 'Désignation', large: true },
    { cle: 'gencod', libelle: 'Gencod', bl: true },
    { cle: 'colis', libelle: 'Colis', num: true, bl: true },
    { cle: 'pcb', libelle: 'PCB', num: true, bl: true },
    { cle: 'quantite', libelle: 'Qté', num: true },
    { cle: 'prix_unitaire_ht', libelle: 'PU HT', num: true },
    { cle: 'montant_ht', libelle: 'Montant HT', num: true }
  ];
  let colonnes = $derived(COLONNES.filter((c) => !c.bl || typeDoc === 'bl'));

  type Etat = 'propre' | 'modifiee' | 'envoi' | 'enregistree' | 'erreur';
  type Rangee = {
    cle: string;
    id: number | null;
    /** Valeurs affichées (texte saisi). */
    v: Record<string, string>;
    /** Valeurs d'origine (pour Échap et pour savoir si la ligne a changé). */
    o: Record<string, string>;
    base: LigneEdition | null;
    etat: Etat;
    erreur: string;
    champ: string;
  };

  let ouvert = $state(false);
  let charge = $state(false);
  let modifiable = $state(false);
  let raison = $state('');
  let erreurChargement = $state('');
  let rangees = $state<Rangee[]>([]);
  let annonce = $state('');
  let aChange = false;
  let tel = $state(false);
  let ouverteTel = $state<string | null>(null);
  let aSupprimer = $state<Rangee | null>(null);
  let dlgSupprimer = $state(false);
  let tableau = $state<HTMLElement>();
  let n = 0;

  /** Largeur disponible : sous 740 px, le tableau ne tient pas, chaque ligne s'ouvre en formulaire. */
  let largeur = $state(0);
  $effect(() => {
    tel = largeur > 0 && largeur < 740;
  });

  /** Montants à 2 décimales, prix jusqu'à 4, quantités telles quelles ; sans espace des milliers (saisie). */
  const DECIMALES: Partial<Record<keyof SaisieLigne, [number, number]>> = { montant_ht: [2, 2], prix_unitaire_ht: [2, 4] };
  function nombreFr(x: number | null, cle: keyof SaisieLigne): string {
    if (x === null) return '';
    const [min, max] = DECIMALES[cle] ?? [0, 3];
    return x.toLocaleString('fr-FR', { minimumFractionDigits: min, maximumFractionDigits: max, useGrouping: false });
  }
  function versTexte(l: LigneEdition | null): Record<string, string> {
    const r: Record<string, string> = {};
    for (const c of COLONNES) {
      const x = l ? l[c.cle] : null;
      r[c.cle] = typeof x === 'number' ? nombreFr(x, c.cle) : (x ?? '');
    }
    return r;
  }
  function rangee(l: LigneEdition | null): Rangee {
    const v = versTexte(l);
    return { cle: l ? `l${l.id}` : `n${++n}`, id: l?.id ?? null, v, o: { ...v }, base: l, etat: 'propre', erreur: '', champ: '' };
  }
  const modifiee = (r: Rangee) => COLONNES.some((c) => r.v[c.cle].trim() !== r.o[c.cle].trim());

  async function ouvrir() {
    ouvert = true;
    charge = false;
    erreurChargement = '';
    aChange = false;
    try {
      const l = await lignesDocument(documentId);
      modifiable = l.modifiable;
      raison = l.raison ?? '';
      rangees = l.lignes.map(rangee);
      charge = true;
    } catch (e) {
      erreurChargement = message(e);
    }
  }

  /** « Terminer » : les lignes encore modifiées s'enregistrent d'abord ; une erreur garde le tableau ouvert. */
  async function fermer() {
    for (const r of rangees) if (r.etat === 'modifiee' && !(await enregistrer(r.cle))) return;
    ouvert = false;
    ouverteTel = null;
    if (aChange) onchange?.();
  }

  /** Texte saisi → valeur envoyée ; `undefined` : nombre illisible. */
  function lireNombre(t: string): number | null | undefined {
    const s = t.trim().replace(/\s/g, '').replace(',', '.');
    if (!s) return null;
    const x = Number(s);
    return Number.isFinite(x) ? x : undefined;
  }

  function saisie(r: Rangee): SaisieLigne | string {
    const b = r.base;
    const s: SaisieLigne = {
      reference: null,
      designation: null,
      quantite: null,
      unite: b?.unite ?? null,
      prix_unitaire_ht: null,
      montant_ht: null,
      gencod: null,
      colis: null,
      pcb: null,
      remise_taux: b?.remise_taux ?? null,
      taux_tva: b?.taux_tva ?? null,
      deee_ht: b?.deee_ht ?? null,
      ecomob_ht: b?.ecomob_ht ?? null
    };
    for (const c of COLONNES) {
      const t = r.v[c.cle];
      if (c.num) {
        const x = lireNombre(t);
        if (x === undefined) return `${c.libelle} : nombre attendu`;
        if (x !== null && x < 0) return `${c.libelle} : nombre positif attendu`;
        (s as unknown as Record<string, number | null>)[c.cle] = x;
      } else (s as unknown as Record<string, string | null>)[c.cle] = t.trim() || null;
    }
    // Montant inchangé : le serveur le recalcule s'il suivait la quantité et le prix.
    if (b && r.v.montant_ht.trim() === r.o.montant_ht.trim()) s.montant_ht = b.montant_ht;
    return s;
  }

  function maj(cle: string, champs: Partial<Rangee>) {
    rangees = rangees.map((r) => (r.cle === cle ? { ...r, ...champs } : r));
  }

  async function enregistrer(cle: string): Promise<boolean> {
    const r = rangees.find((x) => x.cle === cle);
    if (!r || r.etat === 'envoi') return false;
    if (!modifiee(r) && r.id !== null) return true;
    const s = saisie(r);
    if (typeof s === 'string') {
      maj(cle, { etat: 'erreur', erreur: s, champ: '' });
      return false;
    }
    maj(cle, { etat: 'envoi', erreur: '' });
    try {
      const res: ResultatLigne = r.id === null ? await ajouterLigne(documentId, s) : await modifierLigne(documentId, r.id, s);
      const l = res.ligne;
      const v = versTexte(l);
      maj(cle, { id: l?.id ?? r.id, base: l, v, o: { ...v }, etat: 'enregistree', erreur: '', champ: '' });
      aChange = true;
      const nom = l?.designation ?? l?.reference ?? 'Ligne';
      annonce = `${nom} : enregistrée.${res.magasin_relance ? ' Articles cherchés de nouveau au magasin.' : ''}`;
      return true;
    } catch (e) {
      const champ = e instanceof ErreurApi ? (e.champ ?? '') : '';
      maj(cle, { etat: 'erreur', erreur: message(e), champ });
      return false;
    }
  }

  function annulerRangee(cle: string) {
    const r = rangees.find((x) => x.cle === cle);
    if (!r) return;
    if (r.id === null) rangees = rangees.filter((x) => x.cle !== cle);
    else maj(cle, { v: { ...r.o }, etat: 'propre', erreur: '', champ: '' });
  }

  function ajouter() {
    const r = rangee(null);
    rangees = [...rangees, r];
    if (tel) ouverteTel = r.cle;
    else queueMicrotask(() => tableau?.querySelector<HTMLInputElement>(`[data-r="${r.cle}"] input`)?.focus());
  }

  async function supprimer() {
    const r = aSupprimer;
    if (!r) return;
    if (r.id === null) {
      rangees = rangees.filter((x) => x.cle !== r.cle);
      return;
    }
    try {
      await supprimerLigne(documentId, r.id);
      rangees = rangees.filter((x) => x.cle !== r.cle);
      aChange = true;
      annonce = 'Ligne supprimée.';
    } catch (e) {
      maj(r.cle, { etat: 'erreur', erreur: message(e) });
    }
  }

  function saisir(cle: string, col: string, t: string) {
    rangees = rangees.map((r) => {
      if (r.cle !== cle) return r;
      const v = { ...r.v, [col]: t };
      const x = { ...r, v };
      return { ...x, etat: modifiee(x) || r.id === null ? 'modifiee' : 'propre' };
    });
  }

  /** Clavier d'une cellule : Entrée enregistre et descend d'une ligne, Échap défait la ligne. */
  async function touche(e: KeyboardEvent, r: Rangee, col: string) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      annulerRangee(r.cle);
      return;
    }
    if (e.key !== 'Enter') return;
    e.preventDefault();
    e.stopPropagation();
    const i = rangees.findIndex((x) => x.cle === r.cle);
    if (!(await enregistrer(r.cle))) return;
    const suivante = rangees[i + 1];
    if (suivante) tableau?.querySelector<HTMLInputElement>(`[data-r="${suivante.cle}"] [data-c="${col}"]`)?.focus();
  }

  /** Le focus quitte la ligne : elle s'enregistre si elle a changé. */
  function sortie(e: FocusEvent, r: Rangee) {
    const vers = e.relatedTarget as Node | null;
    const tr = e.currentTarget as HTMLElement;
    if (vers && tr.contains(vers)) return;
    const x = rangees.find((y) => y.cle === r.cle);
    if (x && x.etat === 'modifiee') void enregistrer(r.cle);
  }

  const etatTexte = (r: Rangee) => (r.etat === 'envoi' ? 'Enregistrement…' : r.etat === 'enregistree' ? 'Enregistrée' : r.etat === 'modifiee' ? 'Modifiée' : '');
  const corrigeeTitre = (l: LigneEdition | null) =>
    l?.corrigee_le ? `Corrigée à la main le ${new Date(l.corrigee_le).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })}${l.corrigee_par ? ` par ${l.corrigee_par}` : ''}` : '';
  const resumeTel = (r: Rangee) =>
    [r.v.quantite && `${r.v.quantite}${typeDoc === 'bl' && r.v.pcb ? ` (${r.v.colis || '?'} × ${r.v.pcb})` : ''}`, r.v.prix_unitaire_ht && `${r.v.prix_unitaire_ht} €`]
      .filter(Boolean)
      .join(' à ');
</script>

{#if !ouvert}
  <div class="lignes-fermees">
    <button type="button" class="btn sm" onclick={ouvrir}><Icone nom="crayon" />Corriger les lignes</button>
    {#if corrigees > 0}<span class="t2 petit">{corrigees} ligne{corrigees > 1 ? 's' : ''} corrigée{corrigees > 1 ? 's' : ''} à la main</span>{/if}
  </div>
{:else}
  <div class="edition" aria-label="Correction des lignes" bind:clientWidth={largeur}>
    <div class="sr-seul" role="status" aria-live="polite">{annonce}</div>
    {#if erreurChargement}
      <Bandeau ton="erreur" titre="Lignes non chargées.">{erreurChargement}</Bandeau>
    {:else if !charge}
      <p class="t2 petit">Chargement…</p>
    {:else if !modifiable}
      <Bandeau ton="info" titre="Lignes non modifiables.">{raison}</Bandeau>
    {:else if !tel}
      <p class="t2 petit aide-edition">Tab : case suivante. Entrée : enregistrer la ligne. Échap : défaire la ligne.</p>
      <div class="defile-x" bind:this={tableau}>
        <table class="table-edition">
          <thead>
            <tr>
              {#each colonnes as c (c.cle)}<th class:n={c.num}>{c.libelle}</th>{/each}
              <th class="etat-col"><span class="sr-seul">État</span></th>
            </tr>
          </thead>
          <tbody>
            {#each rangees as r, i (r.cle)}
              <tr data-r={r.cle} class:en-faute={r.etat === 'erreur'} onfocusout={(e) => sortie(e, r)}>
                {#each colonnes as c (c.cle)}
                  <td class:n={c.num} class:large={c.large} class:code={c.cle === 'gencod'}>
                    <input
                      type="text"
                      inputmode={c.num ? 'decimal' : undefined}
                      class:num={c.num}
                      class:faute={r.champ === c.cle}
                      data-c={c.cle}
                      aria-label="{c.libelle}, ligne {i + 1}"
                      aria-invalid={r.champ === c.cle ? 'true' : undefined}
                      value={r.v[c.cle]}
                      oninput={(e) => saisir(r.cle, c.cle, e.currentTarget.value)}
                      onkeydown={(e) => touche(e, r, c.cle)}
                    />
                  </td>
                {/each}
                <td class="etat-col">
                  <span class="etat-ligne">
                    {#if r.base?.corrigee_le && r.etat === 'propre'}<Etiquette ton="pale" title={corrigeeTitre(r.base)}>corrigée</Etiquette>{:else}<span
                        class="t2 petit">{etatTexte(r)}</span
                      >{/if}
                    <button
                      type="button"
                      class="btn-icone"
                      aria-label="Supprimer la ligne {i + 1}"
                      title="Supprimer la ligne"
                      onclick={() => {
                        aSupprimer = r;
                        dlgSupprimer = true;
                      }}><Icone nom="fermer" /></button
                    >
                  </span>
                </td>
              </tr>
              {#if r.etat === 'erreur'}
                <tr class="erreur-ligne"><td colspan={colonnes.length + 1}><span role="alert">{r.erreur}</span></td></tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <ul class="liste-tel">
        {#each rangees as r, i (r.cle)}
          <li>
            {#if ouverteTel === r.cle}
              <form
                class="form-tel"
                onsubmit={async (e) => {
                  e.preventDefault();
                  if (await enregistrer(r.cle)) ouverteTel = null;
                }}
              >
                {#each colonnes as c (c.cle)}
                  <label class="champ" class:plein={!c.num && c.cle !== 'gencod'}>
                    <span class="libelle">{c.libelle}</span>
                    <input
                      type="text"
                      inputmode={c.num ? 'decimal' : undefined}
                      class:num={c.num}
                      aria-invalid={r.champ === c.cle ? 'true' : undefined}
                      value={r.v[c.cle]}
                      oninput={(e) => saisir(r.cle, c.cle, e.currentTarget.value)}
                    />
                  </label>
                {/each}
                {#if r.etat === 'erreur'}<p class="erreur-tel" role="alert">{r.erreur}</p>{/if}
                <div class="actions-tel">
                  <button type="submit" class="btn" class:en-cours={r.etat === 'envoi'} disabled={r.etat === 'envoi'}>Enregistrer</button>
                  <button
                    type="button"
                    class="btn"
                    onclick={() => {
                      annulerRangee(r.cle);
                      ouverteTel = null;
                    }}>Annuler</button
                  >
                  <span class="sp"></span>
                  <button
                    type="button"
                    class="btn danger"
                    onclick={() => {
                      aSupprimer = r;
                      dlgSupprimer = true;
                    }}>Supprimer</button
                  >
                </div>
              </form>
            {:else}
              <button type="button" class="ligne-tel" onclick={() => (ouverteTel = r.cle)} aria-label="Corriger la ligne {i + 1} : {r.v.designation || r.v.reference}">
                <span class="a">{r.v.designation || r.v.reference || 'Ligne sans nom'}</span>
                <span class="r num">{r.v.montant_ht}</span>
                <span class="b t2">{r.v.reference && r.v.designation ? `${r.v.reference}, ` : ''}{resumeTel(r)}</span>
                {#if r.etat === 'enregistree'}<span class="t2 petit">Enregistrée</span>{:else if r.base?.corrigee_le}<Etiquette ton="pale">corrigée</Etiquette>{/if}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <div class="pied-edition">
      {#if charge && modifiable}<button type="button" class="btn sm" onclick={ajouter}><Icone nom="plus" />Ajouter une ligne</button>{/if}
      <span class="sp"></span>
      <button type="button" class="btn sm" onclick={fermer}>Terminer</button>
    </div>
  </div>
{/if}

<Confirmation bind:ouvert={dlgSupprimer} titre="Supprimer cette ligne ?" libelle="Supprimer" danger onconfirmer={supprimer}>
  <p>{aSupprimer?.v.designation || aSupprimer?.v.reference || 'La ligne'} disparaît de la pièce. Les écarts et l’export sont recalculés.</p>
</Confirmation>

<style>
  .lignes-fermees {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--e2) var(--e3);
    margin-top: var(--e3);
  }
  .edition {
    margin-top: var(--e3);
  }
  .aide-edition {
    margin: 0 0 var(--e2);
  }
  .table-edition {
    width: 100%;
    border-collapse: collapse;
  }
  .table-edition th {
    text-align: left;
    font-weight: 500;
    font-size: 14px;
    color: var(--text2);
    padding: 4px 4px 6px;
    white-space: nowrap;
  }
  .table-edition th.n {
    text-align: right;
  }
  .table-edition td {
    padding: 2px 1px;
    vertical-align: middle;
  }
  .table-edition input {
    width: 100%;
    min-width: 56px;
    height: 34px;
    padding: 0 8px;
    font-size: 15px;
  }
  .table-edition td.large input {
    min-width: 150px;
  }
  .table-edition td.code input {
    min-width: 124px;
  }
  .table-edition input.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .table-edition input.faute,
  .table-edition input[aria-invalid='true'] {
    border-color: var(--err);
  }
  .etat-col {
    width: 1%;
    white-space: nowrap;
  }
  .etat-ligne {
    display: inline-flex;
    align-items: center;
    gap: var(--e2);
  }
  .erreur-ligne td {
    color: var(--err);
    font-size: 14px;
    padding: 0 4px 6px;
  }
  .pied-edition {
    display: flex;
    align-items: center;
    gap: var(--e3);
    margin-top: var(--e3);
  }
  .sp {
    flex: 1;
  }
  .liste-tel {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--e2);
  }
  .ligne-tel {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px var(--e3);
    width: 100%;
    min-height: 44px;
    padding: 10px 12px;
    text-align: left;
    border: 0;
    border-radius: var(--r2);
    background: var(--pane);
    box-shadow: 0 0 0 1px var(--line);
    color: var(--text);
    font: inherit;
  }
  .ligne-tel .a {
    font-weight: 500;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ligne-tel .r {
    text-align: right;
  }
  .ligne-tel .b {
    grid-column: 1 / -1;
    font-size: 14px;
  }
  .form-tel {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: var(--e3);
    padding: 12px;
    border-radius: var(--r2);
    background: var(--pane);
    box-shadow: 0 0 0 1px var(--line);
  }
  .form-tel input {
    width: 100%;
    height: 36px;
  }
  .form-tel input.num {
    text-align: right;
  }
  .form-tel .plein,
  .form-tel .erreur-tel,
  .form-tel .actions-tel {
    grid-column: 1 / -1;
  }
  .form-tel .champ .libelle {
    margin-bottom: 4px;
  }
  .erreur-tel {
    color: var(--err);
    margin: 0;
  }
  .actions-tel {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
  }
  @media (max-width: 760px) {
    .form-tel input {
      height: 44px;
      font-size: 16px;
    }
    .actions-tel .btn,
    .pied-edition .btn {
      min-height: 44px;
    }
  }
</style>
