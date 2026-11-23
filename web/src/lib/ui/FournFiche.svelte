<script lang="ts" module>
  import type { Fournisseur, StatutFournisseur } from '#lib/types.ts';

  /** Fiche fournisseur en saisie (chaînes, comme les champs). */
  export interface EditionFournisseur {
    nom: string;
    code_gestion: string;
    siret: string;
    tva: string;
    domaines: string;
    /** `a_confirmer` ou `valide` (chaîne : lié à un champ de saisie). */
    statut: string;
    societe_defaut_id: string;
    format_bl: string;
    notes: string;
  }

  export function versEdition(f: Fournisseur): EditionFournisseur {
    return {
      nom: f.nom,
      code_gestion: f.code_gestion ?? '',
      siret: f.siret ?? '',
      tva: f.tva ?? '',
      domaines: f.domaines.join(', '),
      statut: f.statut,
      societe_defaut_id: f.societe_defaut_id === null ? '' : String(f.societe_defaut_id),
      format_bl: f.format_bl ?? '',
      notes: f.notes ?? ''
    };
  }

  /** Corps de `POST /api/admin/fournisseurs` et `PUT /api/admin/fournisseurs/{id}`. */
  export function corpsFournisseur(x: EditionFournisseur, statut: StatutFournisseur = x.statut as StatutFournisseur) {
    return {
      nom: x.nom.trim(),
      code_gestion: x.code_gestion,
      siret: x.siret,
      tva: x.tva,
      domaines: x.domaines
        .split(/[\n,;\s]+/)
        .map((d) => d.trim())
        .filter(Boolean),
      statut,
      societe_defaut_id: x.societe_defaut_id === '' ? null : Number(x.societe_defaut_id),
      format_bl: x.format_bl,
      notes: x.notes
    };
  }
</script>

<script lang="ts">
  // Fiche d'un fournisseur : identité (admin : modifier, confirmer, fusionner), encadré « À rapprocher »,
  // puis le fil par mois (factures, avoirs, BL, mails). Sert au volet de /fournisseurs et à /fournisseurs/[id].
  import type { Snippet } from 'svelte';
  import { api, ErreurApi, filFournisseur, fournisseurs as listerFournisseurs, message, modifierFournisseur } from '#lib/api.ts';
  import { dateSimple } from '#lib/format.ts';
  import {
    BL_EN_ATTENTE,
    FACTURE_SANS_BL,
    libelleCategorie,
    libelleMethode,
    libelleStatutEnvoi,
    libelleStatutFournisseur,
    libelleStatutReponse,
    libelleType,
    nomCourtSociete,
    tonStatutEnvoi,
    tonStatutReponse
  } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { DocRap, ElementFil, FilFournisseur, LigneFournisseur, Rapproche, ChangementsFournisseur } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Champ from './Champ.svelte';
  import Chargement from './Chargement.svelte';
  import Confirmation from './Confirmation.svelte';
  import Etiquette from './Etiquette.svelte';
  import FournEtat from './FournEtat.svelte';
  import Icone from './Icone.svelte';
  import Montant from './Montant.svelte';
  import Repli from './Repli.svelte';
  import Vide from './Vide.svelte';

  let {
    id,
    onmodifie,
    onfusion
  }: {
    id: number;
    /** Après un enregistrement (pour rafraîchir une liste). */
    onmodifie?: () => void;
    /** Après une fusion : la fiche `id` n'existe plus, ouvrir `cible`. */
    onfusion?: (cible: number) => void;
  } = $props();

  let admin = $derived(session.moi?.role === 'admin');
  // Responsable : uniquement sa société.
  let societeChoisie = $derived(admin ? societe.id : (session.moi?.societe_id ?? null));

  let fil = $state<FilFournisseur | null>(null);
  let ed = $state<EditionFournisseur | null>(null);
  let nbMois = $state(6);
  let erreur = $state('');
  let introuvable = $state(false);
  let info = $state('');
  let enCours = $state(false);
  let ficheOuverte = $state(false);

  // Sociétés pour « rangé par défaut dans » (modifiable par un admin seulement, la barre du haut les charge).
  let societes = $derived(admin ? societe.liste : []);

  let jeton = 0;
  async function charger() {
    const moi = ++jeton;
    try {
      const v = await filFournisseur(id, { societe: societeChoisie, mois: nbMois });
      if (moi !== jeton) return;
      fil = v;
      ed = versEdition(v.fournisseur);
      erreur = '';
      introuvable = false;
    } catch (e) {
      if (moi !== jeton) return;
      introuvable = e instanceof ErreurApi && e.code === 404;
      erreur = message(e);
    }
  }

  // Nouveau fournisseur ou autre société : on repart de zéro.
  $effect(() => {
    void id;
    void societeChoisie;
    fil = null;
    ed = null;
    info = '';
    erreur = '';
    nbMois = 6;
    ficheOuverte = false;
  });
  $effect(() => {
    void id;
    void societeChoisie;
    void nbMois;
    void charger();
  });

  let f = $derived(fil?.fournisseur ?? null);
  let aConfirmer = $derived(f?.statut === 'a_confirmer');
  let nomSocieteDefaut = $derived(f?.societe_defaut ?? '');
  let resumeFiche = $derived.by(() => {
    if (!f) return '';
    const m: string[] = [];
    if (f.code_gestion) m.push(`Code ${f.code_gestion}`);
    if (f.siret) m.push(`SIRET ${f.siret}`);
    else if (f.tva) m.push(`TVA ${f.tva}`);
    if (f.domaines.length) m.push(f.domaines.length > 1 ? `${f.domaines[0]} et ${f.domaines.length - 1} autre${f.domaines.length > 2 ? 's' : ''}` : f.domaines[0]);
    if (nomSocieteDefaut) m.push(nomCourtSociete(nomSocieteDefaut));
    return m.join(' · ') || 'Ni SIRET ni domaine';
  });

  async function enregistrer(statut?: 'a_confirmer' | 'valide') {
    if (!ed || !f) return;
    erreur = '';
    info = '';
    enCours = true;
    try {
      // PATCH des seuls champs changés : « Confirmer » n'envoie que le statut.
      const avant = corpsFournisseur(versEdition(f));
      const apres = corpsFournisseur(ed, statut);
      const changements = Object.fromEntries(
        Object.entries(apres).filter(([k, v]) => JSON.stringify(v) !== JSON.stringify(avant[k as keyof typeof avant]))
      ) as ChangementsFournisseur;
      if (Object.keys(changements).length) await modifierFournisseur(f.id, changements);
      info =
        statut === 'valide'
          ? 'Fournisseur confirmé : ses factures ont repassé les contrôles'
          : 'Fiche enregistrée : ses factures ont repassé les contrôles';
      await charger();
      onmodifie?.();
    } catch (e) {
      erreur = message(e);
      await charger();
    } finally {
      enCours = false;
    }
  }

  // ---------- Fusion ----------
  let fusionOuverte = $state(false);
  let fusionQ = $state('');
  let fusionListe = $state<LigneFournisseur[]>([]);
  let cible = $state('');
  let erreurFusion = $state('');
  $effect(() => {
    if (!fusionOuverte) return;
    const q = fusionQ.trim();
    const t = setTimeout(() => {
      listerFournisseurs({ q: q || undefined })
        .then((p) => {
          fusionListe = p.elements.filter((l) => l.id !== id);
          if (cible && !fusionListe.some((l) => String(l.id) === cible)) cible = '';
        })
        .catch((e) => (erreurFusion = message(e)));
    }, 200);
    return () => clearTimeout(t);
  });
  function ouvrirFusion() {
    fusionQ = '';
    cible = '';
    erreurFusion = '';
    fusionOuverte = true;
  }
  async function fusionner() {
    erreurFusion = '';
    if (!cible) {
      erreurFusion = 'Choisissez la fiche à garder';
      throw new Error(erreurFusion);
    }
    try {
      const r = await api.post<{ documents: number }>(`/api/admin/fournisseurs/${id}/fusionner`, { cible_id: Number(cible) });
      info = `Fiches fusionnées : ${r.documents} document${r.documents > 1 ? 's' : ''} déplacé${r.documents > 1 ? 's' : ''}`;
      onmodifie?.();
      onfusion?.(Number(cible));
    } catch (e) {
      erreurFusion = message(e);
      throw e;
    }
  }

  // ---------- Fil ----------
  const moisLong = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric', timeZone: 'UTC' });
  function libelleMois(m: string): string {
    const t = moisLong.format(new Date(`${m}-01T12:00:00Z`));
    return t.charAt(0).toUpperCase() + t.slice(1);
  }
  /** Un mail qui porte un document du fil fait doublon avec lui, sauf s'il attend une réponse. */
  const montrer = (e: ElementFil) => e.genre === 'document' || e.document_ids.length === 0 || e.statut_reponse === 'a_repondre';
  let moisFil = $derived(
    (fil?.mois ?? []).map((m) => ({ mois: m.mois, elements: m.elements.filter(montrer) })).filter((m) => m.elements.length > 0)
  );
  const plusieursSocietes = $derived(admin && societe.id === null && societe.liste.length > 1);
  const expediteurCourt = (s: string) => s.replace(/\s*<[^>]*>\s*$/, '').replace(/^"|"$/g, '') || s;

  // ---------- À rapprocher ----------
  const LIMITE = 5;
  let tout = $state<Record<string, boolean>>({});
  let ar = $derived(fil?.a_rapprocher);
  let nbARapprocher = $derived(ar ? ar.bl_sans_facture.length + ar.factures_sans_bl.length + ar.avec_ecart.length : 0);
  const limiter = <T,>(l: T[], cle: string) => (tout[cle] ? l : l.slice(0, LIMITE));
  const nomDoc = (d: DocRap) => d.numero ?? d.nom_fichier;
  const ecartNet = (e: number | null) => e !== null && Math.abs(e) > 0.05;
</script>

{#snippet docRap(d: DocRap)}
  <a class="ligne" href="/documents/{d.id}">
    <div class="a">{libelleType(d.type)} <span class="num">{nomDoc(d)}</span></div>
    <div class="r"><Montant valeur={d.montant_ht} /> <span class="muted petit">HT</span></div>
    <div class="b">
      {dateSimple(d.date_document)}{#if d.numero_commande}{' · commande '}<span class="num">{d.numero_commande}</span>{/if}{#if plusieursSocietes && d.societe}{' · '}{nomCourtSociete(d.societe)}{/if}
    </div>
  </a>
{/snippet}

{#snippet ecart(r: Rapproche)}
  <a class="ligne" href="/documents/{r.facture.id}">
    <div class="a">{libelleType(r.facture.type)} <span class="num">{nomDoc(r.facture)}</span></div>
    <div class="r">
      {#if r.ecart_montant === null}<span class="muted petit">écart non calculable</span>
      {:else}<span class:flag={ecartNet(r.ecart_montant)} class:e={ecartNet(r.ecart_montant)}><Montant valeur={r.ecart_montant} /></span>{/if}
    </div>
    <div class="b">
      {#each r.bls as b, i (b.lien_id)}{i > 0 ? ', ' : ''}BL <span class="num">{b.numero ?? b.id}</span>{/each}
      {#if r.bls.length} · {libelleMethode(r.bls[0].methode)}{/if}
      {#if r.ecarts.length}
        · <span class="flag w">{r.ecarts.length} écart{r.ecarts.length > 1 ? 's' : ''} de quantité</span>{/if}
    </div>
    {#if r.ecarts.length}
    <div class="ecarts petit t2">
      {#each r.ecarts.slice(0, 3) as x, i (i)}
        <div>
          {x.reference ?? x.designation ?? 'Article'} : livré <span class="num">{x.qte_bl.toLocaleString('fr-FR')}</span>, facturé
          <span class="num">{x.qte_facture.toLocaleString('fr-FR')}</span>
        </div>
      {/each}
      {#if r.ecarts.length > 3}<div class="muted">et {r.ecarts.length - 3} autre{r.ecarts.length > 4 ? 's' : ''}</div>{/if}
    </div>
    {/if}
  </a>
{/snippet}

{#snippet groupe(cle: string, titre: string, n: number, contenu: Snippet)}
  <div class="groupe">
    <h3>{titre} <span class="num muted">{n}</span></h3>
    <div class="filets">{@render contenu()}</div>
    {#if n > LIMITE && !tout[cle]}
      <button type="button" class="plus" onclick={() => (tout[cle] = true)}>{n - LIMITE} autre{n - LIMITE > 1 ? 's' : ''}</button>
    {/if}
  </div>
{/snippet}

{#snippet valeurOuTiret(v: string, mono = false)}
  {#if v}<span class:num={mono}>{v}</span>{:else}<span class="muted">—</span>{/if}
{/snippet}

{#if !fil}
  {#if erreur}
    {#if introuvable}
      <Bandeau ton="erreur" titre="Fiche introuvable.">Ce fournisseur n’existe plus, ou n’a rien dans votre société.</Bandeau>
    {:else}
      <Bandeau ton="erreur" titre="Impossible d’ouvrir la fiche.">
        {erreur}
        {#snippet action()}<button type="button" class="btn sm" onclick={charger}>Réessayer</button>{/snippet}
      </Bandeau>
    {/if}
  {:else}
    <Chargement />
  {/if}
{:else if f && ed}
  <div class="titre-fiche">
    <h1>{f.nom}</h1>
    {#if aConfirmer}<Etiquette ton="alerte">{libelleStatutFournisseur(f.statut)}</Etiquette>{/if}
  </div>
  <div class="meta-fiche"><FournEtat etat={fil.etat} /></div>

  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
  {#if aConfirmer && admin}
    <Bandeau titre="Nouveau fournisseur.">
      Ses factures ne partent pas seules vers Dext ou Yooz tant que la fiche n'est pas confirmée : vérifiez le nom et le SIRET.
      {#snippet action()}
        <button type="button" class="btn sm principal" class:en-cours={enCours} disabled={enCours} onclick={() => enregistrer('valide')}
          >Confirmer</button
        >
      {/snippet}
    </Bandeau>
  {/if}

  <div class="replis">
    <Repli icone="batiment" titre="Fiche du fournisseur" resume={resumeFiche} bind:ouvert={ficheOuverte}>
      <div class="lignes-champs">
        <Champ disposition="ligne" libelle="Nom" bind:valeur={ed.nom} modifiable={admin} onvalider={() => enregistrer()} />
        <Champ
          disposition="ligne"
          libelle="Code fournisseur"
          bind:valeur={ed.code_gestion}
          mono
          modifiable={admin}
          onvalider={() => enregistrer()}
          aide={admin ? 'Le code de ce fournisseur dans le logiciel de gestion du magasin ; repris dans les exports' : undefined}
        >
          {#snippet affichage()}{@render valeurOuTiret(ed?.code_gestion ?? '', true)}{/snippet}
        </Champ>
        <Champ disposition="ligne" libelle="SIRET" bind:valeur={ed.siret} mono modifiable={admin} onvalider={() => enregistrer()}>
          {#snippet affichage()}{@render valeurOuTiret(ed?.siret ?? '', true)}{/snippet}
        </Champ>
        <Champ disposition="ligne" libelle="N° TVA" bind:valeur={ed.tva} mono modifiable={admin} onvalider={() => enregistrer()}>
          {#snippet affichage()}{@render valeurOuTiret(ed?.tva ?? '', true)}{/snippet}
        </Champ>
        <Champ
          disposition="ligne"
          libelle="Domaines mail"
          bind:valeur={ed.domaines}
          modifiable={admin}
          onvalider={() => enregistrer()}
          aide={admin ? 'Séparés par des virgules ; appris tout seul quand une facture est reconnue par son SIRET ou son n° de TVA' : undefined}
        >
          {#snippet affichage()}{@render valeurOuTiret(ed?.domaines ?? '')}{/snippet}
        </Champ>
        <Champ
          disposition="ligne"
          type="select"
          libelle="Société par défaut"
          bind:valeur={ed.societe_defaut_id}
          options={[{ valeur: '', libelle: 'Aucune société' }, ...societes.map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))]}
          modifiable={admin}
          onvalider={() => enregistrer()}
        >
          {#snippet affichage()}{@render valeurOuTiret(nomSocieteDefaut)}{/snippet}
        </Champ>
        <Champ
          disposition="ligne"
          libelle="Format des BL"
          bind:valeur={ed.format_bl}
          modifiable={admin}
          onvalider={() => enregistrer()}
          placeholder="ex. Excel, colonnes réf. / qté"
        >
          {#snippet affichage()}{@render valeurOuTiret(ed?.format_bl ?? '')}{/snippet}
        </Champ>
        <Champ disposition="ligne" type="textarea" libelle="Notes" bind:valeur={ed.notes} modifiable={admin} onvalider={() => enregistrer()}>
          {#snippet affichage()}{@render valeurOuTiret(ed?.notes ?? '')}{/snippet}
        </Champ>
        {#if admin}
          <Champ
            disposition="ligne"
            type="select"
            libelle="Statut"
            bind:valeur={ed.statut}
            options={[
              { valeur: 'valide', libelle: 'Confirmé' },
              { valeur: 'a_confirmer', libelle: 'À confirmer' }
            ]}
            modifiable
            onvalider={() => enregistrer()}
          />
        {/if}
      </div>
      <div class="pied-fiche petit muted">
        <span>Fiche créée le <span class="num">{dateSimple(f.cree_le.slice(0, 10))}</span></span>
        {#if admin}
          <span class="sp"></span>
          <button type="button" class="btn sm fantome" onclick={ouvrirFusion}><Icone nom="lien" />Fusionner avec une autre fiche</button>
        {/if}
      </div>
    </Repli>
  </div>

  <section class="bloc-fil" aria-labelledby="ar-{id}">
    <h2 id="ar-{id}">À rapprocher {#if nbARapprocher}<span class="num muted">{nbARapprocher}</span>{/if}</h2>
    {#if ar && nbARapprocher}
      <div class="encadre-ar">
        {#if ar.bl_sans_facture.length}
          {#snippet bls()}{#each limiter(ar?.bl_sans_facture ?? [], 'bl') as d (d.id)}{@render docRap(d)}{/each}{/snippet}
          {@render groupe('bl', BL_EN_ATTENTE, ar.bl_sans_facture.length, bls)}
        {/if}
        {#if ar.avec_ecart.length}
          {#snippet ecarts()}{#each limiter(ar?.avec_ecart ?? [], 'ecart') as r (r.facture.id)}{@render ecart(r)}{/each}{/snippet}
          {@render groupe('ecart', 'Écarts entre BL et facture', ar.avec_ecart.length, ecarts)}
        {/if}
        {#if ar.factures_sans_bl.length}
          {#snippet fsb()}{#each limiter(ar?.factures_sans_bl ?? [], 'fsb') as d (d.id)}{@render docRap(d)}{/each}{/snippet}
          {@render groupe('fsb', `${FACTURE_SANS_BL}`, ar.factures_sans_bl.length, fsb)}
        {/if}
        {#if ar.tronque}<p class="petit muted">Liste limitée aux 300 plus récents de chaque sorte.</p>{/if}
      </div>
    {:else}
      <p class="muted">Rien à rapprocher : chaque BL a sa facture, sans écart.</p>
    {/if}
  </section>

  <section class="bloc-fil" aria-labelledby="fil-{id}">
    <h2 id="fil-{id}">Factures, BL et mails</h2>
    {#each moisFil as m (m.mois)}
      <div class="mois">
        <h3>{libelleMois(m.mois)} <span class="num muted">{m.elements.length}</span></h3>
        <div class="filets">
          {#each m.elements as e (`${e.genre}-${e.id}`)}
            {#if e.genre === 'document'}
              <a class="ligne" href="/documents/{e.id}">
                <div class="a">
                  {libelleType(e.type)}
                  {#if e.numero}<span class="num">{e.numero}</span>{/if}
                </div>
                <div class="r">
                  {#if e.montant_ttc !== null}<Montant valeur={e.montant_ttc} avoir={e.type === 'avoir'} />
                  {:else if e.montant_ht !== null}<Montant valeur={e.montant_ht} avoir={e.type === 'avoir'} /> <span class="muted petit">HT</span>{/if}
                </div>
                <div class="b">
                  <span class="num">{dateSimple(e.date)}</span>
                  {#if e.type === 'bl'}
                    {#if !e.verifie} · <Etiquette ton="alerte">À valider</Etiquette>{/if}
                    {#if !e.rapproche} · <span class="flag w">{BL_EN_ATTENTE}</span>{/if}
                  {:else if e.statut_depot}
                    · <Etiquette ton={tonStatutEnvoi(e.statut_depot)}>{libelleStatutEnvoi(e.statut_depot, e.outil)}</Etiquette>
                  {/if}
                  {#if e.nb_alertes}
                    · <span class="flag w">{e.nb_alertes} alerte{e.nb_alertes > 1 ? 's' : ''}</span>{/if}
                  {#if plusieursSocietes && e.societe} · {nomCourtSociete(e.societe)}{/if}
                </div>
              </a>
            {:else}
              <a class="ligne" href="/mails/{e.id}">
                <div class="a"><Icone nom="mail" class="ic-mail" />{e.objet || 'Sans objet'}</div>
                <div class="r">
                  {#if e.statut_reponse}<Etiquette ton={tonStatutReponse(e.statut_reponse)}>{libelleStatutReponse(e.statut_reponse)}</Etiquette>{/if}
                </div>
                <div class="b">
                  <span class="num">{dateSimple(e.date)}</span> · {expediteurCourt(e.expediteur)} · {e.resume ?? libelleCategorie(e.categorie)}
                </div>
              </a>
            {/if}
          {/each}
        </div>
      </div>
    {:else}
      <Vide compact titre="Rien sur les {nbMois} derniers mois" texte="Ni facture, ni BL, ni mail de ce fournisseur." />
    {/each}
    {#if nbMois < 24}
      <button type="button" class="btn sm plus-ancien" onclick={() => (nbMois = nbMois < 12 ? 12 : 24)}>
        Voir jusqu'à {nbMois < 12 ? 12 : 24} mois
      </button>
    {/if}
  </section>

  {#if admin}
    <Confirmation bind:ouvert={fusionOuverte} titre="Fusionner « {f.nom} »" libelle="Fusionner" danger onconfirmer={fusionner}>
      <p>Ses documents, identifiants et domaines passent sur la fiche gardée ; celle-ci disparaît.</p>
      <div class="fusion">
        <label class="petit t2" for="fusion-q-{id}">Fiche à garder</label>
        <input id="fusion-q-{id}" type="search" placeholder="Nom, SIRET, TVA ou domaine" bind:value={fusionQ} autocomplete="off" />
        <select bind:value={cible} size="6" aria-label="Fiche à garder">
          {#each fusionListe as l (l.id)}
            <option value={String(l.id)}>{l.nom}{l.siret ? ` · ${l.siret}` : ''}{l.statut === 'a_confirmer' ? ' · à confirmer' : ''}</option>
          {/each}
        </select>
        {#if erreurFusion}<p class="erreur-fusion petit">{erreurFusion}</p>{/if}
      </div>
    </Confirmation>
  {/if}
{/if}

<style>
  .replis {
    margin-top: var(--e4);
  }
  .pied-fiche {
    display: flex;
    align-items: center;
    gap: var(--e2);
    padding-top: var(--e3);
  }
  .bloc-fil {
    margin-top: var(--e8);
  }
  .bloc-fil > h2 {
    font-size: 14px;
    margin-bottom: var(--e2);
  }
  .bloc-fil h3 {
    font-size: 12px;
    font-weight: 500;
    color: var(--text2);
    margin: var(--e4) 0 var(--e1);
  }
  .encadre-ar {
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    padding: 0 var(--e3) var(--e2);
  }
  .encadre-ar .groupe:first-child h3 {
    margin-top: var(--e3);
  }
  .encadre-ar .filets {
    border-top: 0;
  }
  .encadre-ar .filets > :global(:last-child) {
    border-bottom: 0;
  }
  .ecarts {
    grid-column: 1 / -1;
    padding-top: var(--e1);
  }
  .plus-ancien {
    margin-top: var(--e4);
  }
  :global(.ligne .a svg.ic-mail) {
    color: var(--muted);
    margin-right: 6px;
    vertical-align: -3px;
  }
  .fusion {
    display: grid;
    gap: var(--e2);
    margin-top: var(--e3);
  }
  .fusion select {
    height: auto;
    padding: var(--e1);
    background-image: none;
  }
  .erreur-fusion {
    color: var(--err);
  }
</style>
