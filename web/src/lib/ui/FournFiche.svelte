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
    adresse: string;
    telephone: string;
    email: string;
    interlocuteur: string;
    conditions_paiement: string;
    /** Séparés par des virgules (un nom peut contenir des espaces). */
    noms_documents: string;
    /** Séparés par des virgules ou des espaces. */
    prefixes_codes: string;
    groupe_id: number | null;
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
      notes: f.notes ?? '',
      adresse: f.adresse ?? '',
      telephone: f.telephone ?? '',
      email: f.email ?? '',
      interlocuteur: f.interlocuteur ?? '',
      conditions_paiement: f.conditions_paiement ?? '',
      noms_documents: f.noms_documents.join(', '),
      prefixes_codes: f.prefixes_codes.join(', '),
      groupe_id: f.groupe_id
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
      notes: x.notes,
      adresse: x.adresse,
      telephone: x.telephone,
      email: x.email,
      interlocuteur: x.interlocuteur,
      conditions_paiement: x.conditions_paiement,
      noms_documents: x.noms_documents
        .split(/[\n,;]+/)
        .map((n) => n.trim())
        .filter(Boolean),
      prefixes_codes: x.prefixes_codes
        .split(/[\n,;\s]+/)
        .map((p) => p.trim())
        .filter(Boolean),
      groupe_id: x.groupe_id
    };
  }
</script>

<script lang="ts">
  // Fiche d'un fournisseur (maquette Sauge S3) : titre et état, « Fiche du fournisseur » (champs remplis,
  // encadré des champs à compléter ; admin : « Modifier », confirmer, fusionner, codes du magasin),
  // groupe et marques, « À rapprocher » en cartes, puis le fil par mois (factures, avoirs, BL, mails).
  // Sert au volet de /fournisseurs et à /fournisseurs/[id].
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
  import type { DocRap, ElementFil, FilFournisseur, LigneFournisseur, LigneMarque, Rapproche, ChangementsFournisseur } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Champ from './Champ.svelte';
  import Chargement from './Chargement.svelte';
  import CodesMagasin from './CodesMagasin.svelte';
  import Confirmation from './Confirmation.svelte';
  import Etiquette from './Etiquette.svelte';
  import FournEtat from './FournEtat.svelte';
  import FournGroupe from './FournGroupe.svelte';
  import Icone from './Icone.svelte';
  import { phraseEcartLigne } from './DocOutils.ts';
  import Montant, { formatMontant } from './Montant.svelte';
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

  // Sociétés pour « rangé par défaut dans » (modifiable par un admin seulement, la barre du haut les charge).
  let societes = $derived(admin ? societe.liste : []);

  let jeton = 0;
  async function charger() {
    const moi = ++jeton;
    try {
      const v = await filFournisseur(id, { societe: societeChoisie, mois: nbMois });
      if (moi !== jeton) return;
      fil = v;
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
    fautes = {};
    info = '';
    erreur = '';
    nbMois = 6;
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

  // Lignes de la fiche : celles remplies sont affichées, les vides listées dans « N champs à compléter ».
  // Le code BO Phénix et le statut sont toujours affichés.
  let champs = $derived.by(() => {
    if (!f) return [];
    const c: { libelle: string; valeur: string; mono?: boolean; multi?: boolean; toujours?: boolean }[] = [
      { libelle: 'Code BO Phénix', valeur: f.code_gestion ?? '', mono: true, toujours: true }
    ];
    if (f.groupe_id !== null) {
      c.push({ libelle: 'Noms sur les documents', valeur: f.noms_documents.join(', ') });
      c.push({ libelle: 'Préfixes des codes', valeur: f.prefixes_codes.join(', '), mono: true });
    }
    c.push(
      { libelle: 'Domaines mail', valeur: f.domaines.join(', ') },
      { libelle: 'SIRET', valeur: f.siret ?? '', mono: true },
      { libelle: 'N° TVA', valeur: f.tva ?? '', mono: true },
      { libelle: 'Adresse', valeur: f.adresse ?? '', multi: true },
      { libelle: 'Téléphone', valeur: f.telephone ?? '' },
      { libelle: 'E-mail de contact', valeur: f.email ?? '' },
      { libelle: 'Interlocuteur', valeur: f.interlocuteur ?? '' },
      { libelle: 'Conditions de paiement', valeur: f.conditions_paiement ?? '' },
      { libelle: 'Société par défaut', valeur: nomSocieteDefaut },
      { libelle: 'Format des BL', valeur: f.format_bl ?? '' },
      { libelle: 'Notes', valeur: f.notes ?? '', multi: true },
      { libelle: 'Statut', valeur: f.statut === 'valide' ? 'Confirmé' : 'À confirmer', toujours: true }
    );
    return c;
  });
  let champsRemplis = $derived(champs.filter((c) => c.toujours || c.valeur.trim()));
  let champsVides = $derived(champs.filter((c) => !c.toujours && !c.valeur.trim()).map((c) => c.libelle));
  let metaFiche = $derived.by(() => {
    if (!f) return '';
    const m: string[] = [f.code_gestion ? `Code BO Phénix ${f.code_gestion}` : 'Sans code BO Phénix'];
    if (f.domaines.length) m.push(f.domaines.length > 1 ? `${f.domaines[0]} et ${f.domaines.length - 1} autre${f.domaines.length > 2 ? 's' : ''}` : f.domaines[0]);
    if (f.statut === 'valide') m.push('Confirmé');
    return m.join(' · ');
  });

  // ---------- Modification (admin) ----------
  /** Message d'erreur de l'API sous le champ en faute (`siret`, `email`…). */
  let fautes = $state<Record<string, string>>({});
  /** La fiche a des marques : c'est un groupe, elle n'entre pas dans un autre. */
  let estGroupe = $state(false);
  let groupeQ = $state('');
  let groupesPossibles = $state<LigneFournisseur[]>([]);

  async function ouvrirEdition() {
    if (!f) return;
    ed = versEdition(f);
    fautes = {};
    erreur = '';
    info = '';
    groupeQ = '';
    estGroupe = false;
    if (f.groupe_id === null) {
      try {
        estGroupe = (await api.get<LigneMarque[]>(`/api/fournisseurs/${f.id}/marques`)).length > 0;
      } catch {
        /* sans la liste, le choix du groupe reste proposé ; l'API refuse un groupe dans un groupe */
      }
    }
  }
  function fermerEdition() {
    ed = null;
    fautes = {};
  }
  // Groupes proposés : fiches qui ne sont pas elles-mêmes dans un groupe.
  $effect(() => {
    if (!ed || estGroupe) return;
    const q = groupeQ.trim();
    const t = setTimeout(() => {
      listerFournisseurs({ q: q || undefined })
        .then((p) => (groupesPossibles = p.elements.filter((l) => l.id !== id && l.groupe === null)))
        .catch(() => (groupesPossibles = []));
    }, 200);
    return () => clearTimeout(t);
  });
  let optionsGroupe = $derived.by(() => {
    const o = [{ valeur: '', libelle: 'Aucun groupe' }];
    if (f?.groupe_id != null && f.groupe && !groupesPossibles.some((g) => g.id === f?.groupe_id)) {
      o.push({ valeur: String(f.groupe_id), libelle: f.groupe });
    }
    for (const g of groupesPossibles) o.push({ valeur: String(g.id), libelle: g.nom });
    return o;
  });

  /** PATCH des seuls champs changés : une ancienne valeur non touchée ne bloque pas l'enregistrement. */
  async function enregistrer(e?: SubmitEvent) {
    e?.preventDefault();
    if (!ed || !f) return;
    erreur = '';
    info = '';
    fautes = {};
    enCours = true;
    try {
      const avant = corpsFournisseur(versEdition(f));
      const apres = corpsFournisseur(ed);
      const changements = Object.fromEntries(
        Object.entries(apres).filter(([k, v]) => JSON.stringify(v) !== JSON.stringify(avant[k as keyof typeof avant]))
      ) as ChangementsFournisseur;
      if (Object.keys(changements).length) {
        await modifierFournisseur(f.id, changements);
        info = 'Fiche enregistrée : ses factures ont repassé les contrôles';
      } else {
        info = 'Rien n’a changé';
      }
      ed = null;
      await charger();
      onmodifie?.();
    } catch (err) {
      if (err instanceof ErreurApi && err.champ) fautes = { [err.champ]: err.message };
      else erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  async function confirmer() {
    if (!f) return;
    erreur = '';
    info = '';
    enCours = true;
    try {
      await modifierFournisseur(f.id, { statut: 'valide' });
      info = 'Fournisseur confirmé : ses factures ont repassé les contrôles';
      await charger();
      onmodifie?.();
    } catch (err) {
      erreur = message(err);
    } finally {
      enCours = false;
    }
  }

  function apresGroupe() {
    void charger();
    onmodifie?.();
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
  let sortesAR = $derived(ar ? [ar.bl_sans_facture.length, ar.avec_ecart.length, ar.factures_sans_bl.length].filter((n) => n > 0).length : 0);
  const limiter = <T,>(l: T[], cle: string) => (tout[cle] ? l : l.slice(0, LIMITE));
  const nomDoc = (d: DocRap) => d.numero ?? d.nom_fichier;
  const ecartNet = (e: number | null) => e !== null && Math.abs(e) > 0.05;
</script>


{#snippet carteDoc(d: DocRap, etat: string, montantEcart: number | null | undefined = undefined)}
  <a class="carte-ar" href="/documents/{d.id}">
    <span class="ic"><Icone nom={d.type === 'bl' ? 'camion' : 'fichier'} /></span>
    <span class="corps">
      <span class="titre">{libelleType(d.type)} <span class="num">{nomDoc(d)}</span></span>
      <span class="sous">
        {etat}{#if d.date_document}{' · '}<span class="num">{dateSimple(d.date_document)}</span>{/if}{#if d.numero_commande}{' · commande '}<span class="num">{d.numero_commande}</span>{/if}{#if plusieursSocietes && d.societe}{' · '}{nomCourtSociete(d.societe)}{/if}
      </span>
    </span>
    <span class="montant">
      {#if montantEcart !== undefined}
        {#if montantEcart === null}<span class="muted petit">écart non calculable</span>
        {:else}<span class:etiquette={ecartNet(montantEcart)} class:att={ecartNet(montantEcart)}>Écart <Montant valeur={montantEcart} /></span>{/if}
      {:else}
        <Montant valeur={d.montant_ht} /> HT
      {/if}
    </span>
    <Icone nom="droite" class="chev" />
  </a>
{/snippet}

{#snippet ecart(r: Rapproche)}
  <div class="ecart-bloc">
    {@render carteDoc(
      r.facture,
      r.bls.length ? `${r.bls.map((b) => `BL ${b.numero ?? b.id}`).join(', ')} · ${libelleMethode(r.bls[0].methode)}` : 'Écart avec les BL',
      r.ecart_montant
    )}
    {#if (r.facture.remise_ht && r.facture.remise_ht > 0.005) || r.ecarts.length}
      <div class="ecarts petit t2">
        {#if r.facture.remise_ht && r.facture.remise_ht > 0.005}<div><span class="etiquette info">Remise {formatMontant(r.facture.remise_ht)}</span></div>{/if}
        {#if r.ecarts.length}
          <div class="ecarts-t"><span class="etiquette att">{r.ecarts.length} article{r.ecarts.length > 1 ? 's' : ''} en écart</span></div>
          {#each r.ecarts.slice(0, 3) as x, i (i)}
            {@const p = phraseEcartLigne(x)}
            <div>{p.titre} : {p.article}, {p.detail}</div>
          {/each}
          {#if r.ecarts.length > 3}<div class="muted">et {r.ecarts.length - 3} autre{r.ecarts.length > 4 ? 's' : ''}</div>{/if}
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet groupe(cle: string, titre: string, n: number, contenu: Snippet)}
  <div class="groupe-ar">
    {#if sortesAR > 1}<h4>{titre} <span class="num muted">{n}</span></h4>{/if}
    {@render contenu()}
    {#if n > LIMITE && !tout[cle]}
      <button type="button" class="plus" onclick={() => (tout[cle] = true)}>{n - LIMITE} autre{n - LIMITE > 1 ? 's' : ''}</button>
    {/if}
  </div>
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
{:else if f}
  <div class="fiche-f">
    <header class="tete">
      <div class="tete-g">
        <h2 class="nom">{f.nom}</h2>
        <div class="meta">
          <span>{metaFiche}</span>
          {#if aConfirmer}<span class="etiquette att">{libelleStatutFournisseur(f.statut)}</span>{/if}
          <FournEtat etat={fil.etat} quoi="etiquettes" />
        </div>
      </div>
      {#if admin}
        {#if !ed}
          <button type="button" class="btn modifier" onclick={ouvrirEdition}>Modifier</button>
        {/if}
      {:else}
        <span class="lecture-seule petit">Modifiable par l’administrateur</span>
      {/if}
    </header>

    {#if f.groupe_id !== null || erreur || info || (aConfirmer && admin && !ed)}
      <div class="annonces">
        {#if f.groupe_id !== null}
          <FournGroupe fiche={f} {admin} onmodifie={apresGroupe} />
        {/if}
        {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
        {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
        {#if aConfirmer && admin && !ed}
          <Bandeau titre="Nouveau fournisseur.">
            Ses factures ne partent pas seules vers Dext ou Yooz tant que la fiche n'est pas confirmée : vérifiez le nom et le SIRET.
            {#snippet action()}
              <button type="button" class="btn sm principal" class:en-cours={enCours} disabled={enCours} onclick={confirmer}>Confirmer</button>
            {/snippet}
          </Bandeau>
        {/if}
      </div>
    {/if}

    {#if ed}
      <section class="sec-f cadre-form" aria-labelledby="ed-{id}">
        <h3 id="ed-{id}">Modifier la fiche</h3>
        <form class="form-fiche" onsubmit={enregistrer} aria-label="Modifier la fiche de {f.nom}">
          <div class="plein">
            <Champ
              libelle="Code BO Phénix"
              bind:valeur={ed.code_gestion}
              mono
              placeholder="ex. 004512"
              erreur={fautes.code_gestion}
              aide="Notre code interne de ce fournisseur dans Phénix ; repris dans les exports"
            />
          </div>
          <div class="plein"><Champ libelle="Nom" bind:valeur={ed.nom} requis erreur={fautes.nom} /></div>
          <Champ libelle="SIRET" bind:valeur={ed.siret} mono inputmode="numeric" placeholder="14 chiffres" erreur={fautes.siret} />
          <Champ libelle="N° TVA intracommunautaire" bind:valeur={ed.tva} mono placeholder="FR..." erreur={fautes.tva} />
          <div class="plein">
            <Champ
              libelle="Domaines mail"
              bind:valeur={ed.domaines}
              placeholder="durand.fr"
              erreur={fautes.domaines}
              aide="Séparés par des virgules ; appris tout seul quand une facture est reconnue par son SIRET ou son n° de TVA"
            />
          </div>
          <div class="plein">
            <Champ libelle="Adresse" type="textarea" bind:valeur={ed.adresse} placeholder="N°, rue, code postal, ville" erreur={fautes.adresse} />
          </div>
          <Champ libelle="Téléphone" bind:valeur={ed.telephone} inputmode="tel" placeholder="03 83 00 00 00" erreur={fautes.telephone} />
          <Champ libelle="E-mail de contact" bind:valeur={ed.email} inputmode="email" placeholder="commandes@fournisseur.fr" erreur={fautes.email} />
          <Champ libelle="Interlocuteur" bind:valeur={ed.interlocuteur} placeholder="Nom du commercial ou du contact" erreur={fautes.interlocuteur} />
          <Champ
            libelle="Conditions de paiement"
            bind:valeur={ed.conditions_paiement}
            placeholder="ex. 30 jours fin de mois, LCR"
            erreur={fautes.conditions_paiement}
          />
          <Champ
            libelle="Société par défaut"
            type="select"
            bind:valeur={ed.societe_defaut_id}
            options={[{ valeur: '', libelle: 'Aucune société' }, ...societes.map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))]}
            erreur={fautes.societe_defaut_id}
          />
          <Champ libelle="Format des BL" bind:valeur={ed.format_bl} placeholder="ex. Excel, colonnes réf. / qté" erreur={fautes.format_bl} />
          <div class="plein"><Champ libelle="Notes" type="textarea" bind:valeur={ed.notes} erreur={fautes.notes} /></div>
          <Champ
            libelle="Statut"
            type="select"
            bind:valeur={ed.statut}
            options={[
              { valeur: 'valide', libelle: 'Confirmé' },
              { valeur: 'a_confirmer', libelle: 'À confirmer' }
            ]}
            erreur={fautes.statut}
            aide="Une fiche à confirmer bloque l'envoi automatique de ses factures"
          />
          {#if !estGroupe}
            <div class="champ" class:en-faute={!!fautes.groupe_id}>
              <label for="groupe-{id}">Fait partie du groupe</label>
              <input type="search" class="groupe-q" placeholder="Chercher un groupe (ex. U10)" bind:value={groupeQ} autocomplete="off" aria-label="Chercher un groupe" />
              <select
                id="groupe-{id}"
                bind:value={() => (ed?.groupe_id == null ? '' : String(ed.groupe_id)), (v) => ed && (ed.groupe_id = v === '' ? null : Number(v))}
                aria-invalid={fautes.groupe_id ? 'true' : undefined}
              >
                {#each optionsGroupe as o (o.valeur)}<option value={o.valeur}>{o.libelle}</option>{/each}
              </select>
              {#if fautes.groupe_id}<div class="erreur-champ" role="alert">{fautes.groupe_id}</div>{/if}
              <div class="aide">Pour une marque d'un groupe qui facture pour plusieurs marques (U10 pour L3C, Homea, Decor10)</div>
            </div>
            {#if ed.groupe_id !== null}
              <div class="plein">
                <Champ
                  libelle="Noms sur les documents"
                  bind:valeur={ed.noms_documents}
                  placeholder="ex. HOMEA DECO, homea.fr"
                  erreur={fautes.noms_documents}
                  aide="Autres écritures de cette marque dans les factures et BL du groupe, séparées par des virgules ; son nom est toujours cherché"
                />
              </div>
              <div class="plein">
                <Champ
                  libelle="Préfixes des codes"
                  bind:valeur={ed.prefixes_codes}
                  placeholder="ex. LC"
                  erreur={fautes.prefixes_codes}
                  aide="Début des n° de BL ou codes client de cette marque quand le document ne la nomme pas (LC pour LC54023001), séparés par des virgules"
                />
              </div>
            {/if}
          {:else}
            <p class="petit t2 groupe-note">Cette fiche est un groupe : ses marques sont listées plus bas.</p>
          {/if}
          <div class="plein boutons">
            <button type="submit" class="btn principal" class:en-cours={enCours} disabled={enCours}>Enregistrer</button>
            <button type="button" class="btn" onclick={fermerEdition} disabled={enCours}>Annuler</button>
          </div>
        </form>
      </section>
    {:else}
      <section class="sec-f" aria-labelledby="fiche-{id}">
        <h3 id="fiche-{id}">Fiche du fournisseur</h3>
        <dl class="champs">
          {#each champsRemplis as c (c.libelle)}
            <div>
              <dt>{c.libelle}</dt>
              <dd class:num={c.mono} class:multi={c.multi}>{#if c.valeur}{c.valeur}{:else}<span class="muted">—</span>{/if}</dd>
            </div>
          {/each}
        </dl>
        {#if admin && !f.code_gestion}
          <CodesMagasin
            {id}
            onassocie={async () => {
              info = 'Code BO Phénix associé';
              await charger();
              onmodifie?.();
            }}
          />
        {/if}
        {#if champsVides.length}
          <div class="a-completer">
            <div class="ac-t">
              <span class="ac-titre"><span class="num">{champsVides.length}</span> champ{champsVides.length > 1 ? 's' : ''} à compléter</span>
              <span class="ac-liste">{champsVides.join(', ')}</span>
            </div>
            {#if admin}
              <button type="button" class="btn" onclick={ouvrirEdition}>Compléter la fiche</button>
            {:else}
              <span class="petit t2">L’administrateur peut les compléter</span>
            {/if}
          </div>
        {/if}
        <div class="pied-fiche">
          <span>Fiche créée le <span class="num">{dateSimple(f.cree_le.slice(0, 10))}</span></span>
          {#if admin}
            <button type="button" class="lien-discret" onclick={ouvrirFusion}><Icone nom="lien" />Fusionner avec une autre fiche</button>
          {/if}
        </div>
      </section>
    {/if}

    {#if f.groupe_id === null}
      <FournGroupe fiche={f} {admin} onmodifie={apresGroupe} />
    {/if}

    <section class="sec-f" aria-labelledby="ar-{id}">
      <h3 id="ar-{id}">À rapprocher {#if nbARapprocher}<span class="num muted">{nbARapprocher}</span>{/if}</h3>
      {#if ar && nbARapprocher}
        {#if ar.bl_sans_facture.length}
          {#snippet bls()}{#each limiter(ar?.bl_sans_facture ?? [], 'bl') as d (d.id)}{@render carteDoc(d, 'En attente de facture')}{/each}{/snippet}
          {@render groupe('bl', BL_EN_ATTENTE, ar.bl_sans_facture.length, bls)}
        {/if}
        {#if ar.avec_ecart.length}
          {#snippet ecarts()}{#each limiter(ar?.avec_ecart ?? [], 'ecart') as r (r.facture.id)}{@render ecart(r)}{/each}{/snippet}
          {@render groupe('ecart', 'Écarts entre BL et facture', ar.avec_ecart.length, ecarts)}
        {/if}
        {#if ar.factures_sans_bl.length}
          {#snippet fsb()}{#each limiter(ar?.factures_sans_bl ?? [], 'fsb') as d (d.id)}{@render carteDoc(d, 'Sans BL')}{/each}{/snippet}
          {@render groupe('fsb', FACTURE_SANS_BL, ar.factures_sans_bl.length, fsb)}
        {/if}
        {#if ar.tronque}<p class="petit muted">Liste limitée aux 300 plus récents de chaque sorte.</p>{/if}
      {:else}
        <p class="t2">Rien à rapprocher : chaque BL a sa facture, sans écart.</p>
      {/if}
    </section>

    <section class="sec-f" aria-labelledby="fil-{id}">
      <h3 id="fil-{id}">Factures, BL et mails</h3>
      {#each moisFil as m (m.mois)}
        <div class="mois">
          <h4>{libelleMois(m.mois)} <span class="num muted">{m.elements.length}</span></h4>
          <div class="fil">
            {#each m.elements as e (`${e.genre}-${e.id}`)}
              {#if e.genre === 'document'}
                <a class="el" href="/documents/{e.id}">
                  <span class="el-nom">{libelleType(e.type)}{#if e.numero}{' '}<span class="num">{e.numero}</span>{/if}</span>
                  <span class="t2 num">{dateSimple(e.date)}</span>
                  {#if e.type === 'bl'}
                    {#if !e.verifie}<span class="etiquette att">À valider</span>{/if}
                    {#if !e.rapproche}<span class="el-etat">{BL_EN_ATTENTE}</span>{/if}
                  {:else if e.statut_depot}
                    <Etiquette ton={tonStatutEnvoi(e.statut_depot)}>{libelleStatutEnvoi(e.statut_depot, e.outil)}</Etiquette>
                  {/if}
                  {#if e.remise_ht && e.remise_ht > 0.005}<span class="etiquette info">Remise {formatMontant(e.remise_ht)}</span>{/if}
                  {#if e.nb_alertes}<span class="etiquette att">{e.nb_alertes} alerte{e.nb_alertes > 1 ? 's' : ''}</span>{/if}
                  {#if plusieursSocietes && e.societe}<span class="el-etat">{nomCourtSociete(e.societe)}</span>{/if}
                  <span class="el-montant">
                    {#if e.montant_ttc !== null}<Montant valeur={e.montant_ttc} avoir={e.type === 'avoir'} />
                    {:else if e.montant_ht !== null}<Montant valeur={e.montant_ht} avoir={e.type === 'avoir'} /> <span class="muted">HT</span>{/if}
                  </span>
                </a>
              {:else}
                <a class="el" href="/mails/{e.id}">
                  <span class="el-nom el-mail"><Icone nom="mail" />{e.objet || 'Sans objet'}</span>
                  <span class="t2 num">{dateSimple(e.date)}</span>
                  {#if e.statut_reponse}<Etiquette ton={tonStatutReponse(e.statut_reponse)}>{libelleStatutReponse(e.statut_reponse)}</Etiquette>{/if}
                  <span class="el-etat el-resume">{expediteurCourt(e.expediteur)} · {e.resume ?? libelleCategorie(e.categorie)}</span>
                </a>
              {/if}
            {/each}
          </div>
        </div>
      {:else}
        <Vide compact titre="Rien sur les {nbMois} derniers mois" texte="Ni facture, ni BL, ni mail de ce fournisseur." />
      {/each}
      {#if nbMois < 24}
        <div>
          <button type="button" class="btn" onclick={() => (nbMois = nbMois < 12 ? 12 : 24)}>
            Voir jusqu’à {nbMois < 12 ? 12 : 24} mois
          </button>
        </div>
      {/if}
    </section>
  </div>

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
  /* Fiche Sauge : blocs espacés de 40 px, titres 20 px, lignes clé / valeur à filets. */
  .fiche-f {
    max-width: 816px;
    display: flex;
    flex-direction: column;
    gap: 40px;
    container-type: inline-size;
  }
  .tete {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
  }
  .tete-g {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    flex: 1 1 320px;
  }
  .nom {
    font-size: 28px;
    font-weight: 500;
    line-height: 1.15;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 16px;
    color: var(--text2);
  }
  .modifier {
    height: 44px;
    flex: none;
  }
  .lecture-seule {
    flex: none;
    color: var(--text2);
    padding-top: 8px;
  }
  .annonces {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: -16px;
  }
  .sec-f {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .sec-f h3,
  .sec-f h4 {
    margin: 0;
  }
  .sec-f > h3 {
    font-size: 20px;
    font-weight: 500;
    line-height: 1.3;
  }
  .sec-f h4 {
    font-size: 15px;
    font-weight: 500;
    color: var(--text2);
  }
  .champs {
    margin: 0;
    display: flex;
    flex-direction: column;
    font-size: 16px;
  }
  .champs > div {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    gap: 16px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line-faint);
  }
  .champs dt {
    color: var(--text2);
  }
  .champs dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .multi {
    white-space: pre-line;
  }
  .a-completer {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    padding: 16px 20px;
    border-radius: var(--r3);
    background: var(--pane);
    box-shadow: 0 0 0 1px var(--line);
  }
  .ac-t {
    flex: 1 1 360px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .ac-titre {
    font-weight: 500;
  }
  .ac-liste {
    font-size: 15px;
    color: var(--text2);
    text-wrap: pretty;
  }
  .pied-fiche {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 15px;
    color: var(--muted);
  }
  .lien-discret {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text2);
    font-size: 15px;
    cursor: pointer;
  }
  .lien-discret:hover {
    color: var(--text);
    text-decoration: underline;
  }
  .lien-discret :global(svg.i) {
    width: 16px;
    height: 16px;
  }
  /* À rapprocher : une carte par pièce */
  .groupe-ar {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .groupe-ar + .groupe-ar {
    margin-top: 8px;
  }
  .carte-ar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 20px;
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
    color: var(--text);
    text-decoration: none;
    background: var(--white);
  }
  .carte-ar:hover {
    background: var(--pane);
  }
  .carte-ar:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .carte-ar .ic {
    color: var(--muted);
    display: flex;
    flex: none;
  }
  .carte-ar .corps {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .carte-ar .titre {
    font-weight: 500;
  }
  .carte-ar .sous {
    font-size: 15px;
    color: var(--text2);
    overflow-wrap: anywhere;
  }
  .carte-ar .montant {
    flex: none;
    color: var(--text2);
    white-space: nowrap;
  }
  .carte-ar :global(svg.chev) {
    flex: none;
    color: var(--muted);
    width: 16px;
    height: 16px;
  }
  .ecart-bloc {
    display: flex;
    flex-direction: column;
  }
  .ecarts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 20px 4px 56px;
  }
  .ecarts-t {
    margin-bottom: 2px;
  }
  /* Fil par mois */
  .mois {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .mois + .mois {
    margin-top: 12px;
  }
  .fil {
    border-top: 1px solid var(--line-faint);
  }
  .el {
    display: flex;
    align-items: center;
    gap: 6px 12px;
    flex-wrap: wrap;
    padding: 12px 0;
    border-bottom: 1px solid var(--line-faint);
    color: var(--text);
    text-decoration: none;
  }
  .el:hover {
    background: var(--pane);
    box-shadow: -8px 0 0 var(--pane), 8px 0 0 var(--pane);
  }
  .el-nom {
    font-weight: 500;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .el-mail {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .el-mail :global(svg.i) {
    color: var(--muted);
    flex: none;
  }
  .el-etat {
    font-size: 15px;
    color: var(--text2);
  }
  .el-resume {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 200px;
  }
  .el-montant {
    margin-left: auto;
    font-weight: 500;
    white-space: nowrap;
  }
  .el-montant:empty {
    display: none;
  }
  /* Formulaire de modification */
  .form-fiche {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--e4) var(--e5);
    padding: 20px;
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
    background: var(--white);
  }
  .form-fiche .plein {
    grid-column: 1 / -1;
  }
  .form-fiche .boutons {
    display: flex;
    gap: var(--e2);
  }
  .groupe-q {
    margin-bottom: var(--e1);
  }
  .groupe-note {
    align-self: end;
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
  /* Volet étroit ou téléphone */
  @container (max-width: 520px) {
    .nom {
      font-size: 22px;
    }
    .champs > div {
      grid-template-columns: 120px minmax(0, 1fr);
      gap: 12px;
    }
    .form-fiche {
      grid-template-columns: 1fr;
      padding: var(--e3);
    }
    .carte-ar {
      padding: 14px 16px;
      gap: 12px;
    }
    .ecarts {
      padding-left: 16px;
    }
    .modifier {
      height: 40px;
    }
  }
</style>
