<script lang="ts">
  // Groupe de fournisseurs : une société (U10) qui facture pour plusieurs marques (L3C, Homea,
  // Decor10), chacune avec sa fiche et son code BO Phénix. Sur la fiche d'une marque : son groupe.
  // Sur les autres : les marques du groupe, en ajouter, rattacher une fiche existante, répartir.
  import { api, fournisseurs as listerFournisseurs, message, modifierFournisseur } from '#lib/api.ts';
  import type { Fournisseur, LigneFournisseur, LigneMarque } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Confirmation from './Confirmation.svelte';
  import Icone from './Icone.svelte';

  let {
    fiche,
    admin,
    onmodifie
  }: {
    fiche: Fournisseur;
    admin: boolean;
    /** Après un changement (recharger la fiche et la liste). */
    onmodifie: () => void;
  } = $props();

  let marques = $state<LigneMarque[]>([]);
  let erreur = $state('');
  let info = $state('');
  let enCours = $state(false);

  async function charger() {
    if (fiche.groupe_id !== null) return;
    try {
      marques = await api.get<LigneMarque[]>(`/api/fournisseurs/${fiche.id}/marques`);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    void fiche.id;
    void fiche.groupe_id;
    void charger();
  });

  const decouper = (t: string) =>
    t
      .split(/[\n,;]+/)
      .map((x) => x.trim())
      .filter(Boolean);

  // ---------- Nouvelle marque ----------
  let ajoutOuvert = $state(false);
  let nom = $state('');
  let code = $state('');
  let noms = $state('');
  async function ajouter(e: SubmitEvent) {
    e.preventDefault();
    erreur = '';
    enCours = true;
    try {
      await api.post('/api/admin/fournisseurs', {
        nom,
        code_gestion: code,
        noms_documents: decouper(noms),
        groupe_id: fiche.id,
        statut: 'valide'
      });
      info = `« ${nom.trim()} » ajouté au groupe`;
      nom = code = noms = '';
      ajoutOuvert = false;
      await charger();
      onmodifie();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  // ---------- Rattacher une fiche existante ----------
  let rattacherOuvert = $state(false);
  let q = $state('');
  let liste = $state<LigneFournisseur[]>([]);
  let choix = $state('');
  $effect(() => {
    if (!rattacherOuvert) return;
    const v = q.trim();
    const t = setTimeout(() => {
      listerFournisseurs({ q: v || undefined })
        .then((p) => (liste = p.elements.filter((l) => l.id !== fiche.id && !marques.some((m) => m.id === l.id))))
        .catch((e) => (erreur = message(e)));
    }, 200);
    return () => clearTimeout(t);
  });
  async function rattacher() {
    if (!choix) {
      erreur = 'Choisissez la fiche à rattacher';
      throw new Error(erreur);
    }
    try {
      await modifierFournisseur(Number(choix), { groupe_id: fiche.id });
      info = 'Fiche rattachée au groupe';
      choix = '';
      await charger();
      onmodifie();
    } catch (e) {
      erreur = message(e);
      throw e;
    }
  }

  async function repartir() {
    erreur = '';
    info = '';
    enCours = true;
    try {
      const r = await api.post<{ deplaces: number; restants: number }>(`/api/admin/fournisseurs/${fiche.id}/repartir`, {});
      info =
        `${r.deplaces} document${r.deplaces > 1 ? 's' : ''} rangé${r.deplaces > 1 ? 's' : ''} chez sa marque` +
        (r.restants ? ` ; ${r.restants} sans marque reconnue restent ici` : '');
      await charger();
      onmodifie();
    } catch (e) {
      erreur = message(e);
    } finally {
      enCours = false;
    }
  }

  async function retirer() {
    erreur = '';
    try {
      await modifierFournisseur(fiche.id, { groupe_id: null });
      onmodifie();
    } catch (e) {
      erreur = message(e);
    }
  }
</script>

{#if fiche.groupe_id !== null}
  <div class="groupe-de petit">
    <Icone nom="batiment" />
    <span>Fait partie du groupe <a href="/fournisseurs?fiche={fiche.groupe_id}" data-sveltekit-reset="false">{fiche.groupe}</a></span>
    {#if admin}<span class="sp"></span><button type="button" class="btn sm fantome" onclick={retirer}>Retirer du groupe</button>{/if}
  </div>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
{:else if marques.length || admin}
  <section class="bloc-groupe" aria-labelledby="groupe-{fiche.id}">
    <h2 id="groupe-{fiche.id}">Fournisseurs du groupe {#if marques.length}<span class="num muted">{marques.length}</span>{/if}</h2>
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
    {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
    {#if marques.length}
      <p class="petit t2">
        Les factures et BL de {fiche.nom} vont à la marque dont le nom figure dans le document ; sinon Claude tranche, et dans le doute ils restent
        ici.
      </p>
      <div class="filets">
        {#each marques as m (m.id)}
          <a class="ligne" href="/fournisseurs?fiche={m.id}" data-sveltekit-reset="false">
            <div class="a">{m.nom}{#if m.code_gestion}{' '}<span class="code num">BO {m.code_gestion}</span>{/if}</div>
            <div class="r num petit muted">{m.nb_documents} doc.</div>
            <div class="b petit muted">
              {m.noms_documents.length ? `Repéré aussi par : ${m.noms_documents.join(', ')}` : 'Repéré par son nom'}
            </div>
          </a>
        {/each}
      </div>
    {:else}
      <p class="petit t2">
        {fiche.nom} facture pour plusieurs marques, chacune avec son code BO Phénix ? Ajoutez-les ici : chaque document ira à la bonne marque.
      </p>
    {/if}
    {#if admin}
      {#if ajoutOuvert}
        <form class="ajout" onsubmit={ajouter}>
          <label>Nom de la marque<input bind:value={nom} required placeholder="ex. Homea" /></label>
          <label>Code BO Phénix<input class="num" bind:value={code} placeholder="ex. 004512" /></label>
          <label class="plein"
            >Autres écritures sur les documents <span class="muted">(facultatif, séparées par des virgules)</span><input
              bind:value={noms}
              placeholder="ex. HOMEA DECO, homea.fr"
            /></label
          >
          <div class="plein actions">
            <button type="submit" class="btn sm principal" class:en-cours={enCours} disabled={enCours}>Ajouter</button>
            <button type="button" class="btn sm" onclick={() => (ajoutOuvert = false)}>Annuler</button>
          </div>
        </form>
      {:else}
        <div class="actions">
          <button type="button" class="btn sm" onclick={() => (ajoutOuvert = true)}><Icone nom="plus" />Ajouter une marque</button>
          <button
            type="button"
            class="btn sm fantome"
            onclick={() => {
              q = '';
              choix = '';
              rattacherOuvert = true;
            }}><Icone nom="lien" />Rattacher une fiche existante</button
          >
          {#if marques.length}
            <button type="button" class="btn sm fantome" class:en-cours={enCours} disabled={enCours} onclick={repartir}
              >Ranger les documents déjà reçus</button
            >
          {/if}
        </div>
      {/if}
    {/if}
  </section>
{/if}

<Confirmation bind:ouvert={rattacherOuvert} titre="Rattacher une fiche à {fiche.nom}" libelle="Rattacher" onconfirmer={rattacher}>
  <p>La fiche devient une marque du groupe : elle garde ses documents et son code BO Phénix.</p>
  <div class="choix">
    <input type="search" placeholder="Nom, code BO, SIRET" bind:value={q} autocomplete="off" aria-label="Chercher une fiche" />
    <select bind:value={choix} size="6" aria-label="Fiche à rattacher">
      {#each liste as l (l.id)}
        <option value={String(l.id)}>{l.nom}{l.code_gestion ? ` · BO ${l.code_gestion}` : ''}{l.groupe ? ` · groupe ${l.groupe}` : ''}</option>
      {/each}
    </select>
  </div>
</Confirmation>

<style>
  .groupe-de {
    display: flex;
    align-items: center;
    gap: var(--e2);
    padding: var(--e2) var(--e3);
    margin-top: var(--e3);
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--pane);
  }
  .sp {
    flex: 1;
  }
  .bloc-groupe {
    display: grid;
    gap: var(--e2);
    margin-top: var(--e8);
  }
  .code {
    color: var(--muted);
    font-size: 0.85em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
  }
  .ajout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--e2) var(--e3);
    padding: var(--e3);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .ajout label {
    display: grid;
    gap: 4px;
    font-size: 13px;
    font-weight: 500;
  }
  .plein {
    grid-column: 1 / -1;
  }
  .choix {
    display: grid;
    gap: var(--e2);
    margin-top: var(--e2);
  }
</style>
