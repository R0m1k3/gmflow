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
      groupe_id: x.groupe_id
    };
  }
</script>

<script lang="ts">
  // Fiche d'un fournisseur : identité en lecture (admin : bouton « Modifier » qui ouvre le formulaire
  // complet, confirmer, fusionner), encadré « À rapprocher », puis le fil par mois (factures, avoirs,
  // BL, mails). Sert au volet de /fournisseurs et à /fournisseurs/[id].
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
  let ficheOuverte = $state(true);

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
    ficheOuverte = true;
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
    if (f.code_gestion) m.push(`BO Phénix ${f.code_gestion}`);
    else m.push('Sans code BO Phénix');
    if (f.siret) m.push(`SIRET ${f.siret}`);
    else if (f.tva) m.push(`TVA ${f.tva}`);
    if (f.domaines.length) m.push(f.domaines.length > 1 ? `${f.domaines[0]} et ${f.domaines.length - 1} autre${f.domaines.length > 2 ? 's' : ''}` : f.domaines[0]);
    if (nomSocieteDefaut) m.push(nomCourtSociete(nomSocieteDefaut));
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
    ficheOuverte = true;
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
{:else if f}
  <div class="titre-fiche">
    <h1>{f.nom}</h1>
    {#if aConfirmer}<Etiquette ton="alerte">{libelleStatutFournisseur(f.statut)}</Etiquette>{/if}
    <span class="sp"></span>
    {#if admin}
      {#if !ed}
        <button type="button" class="btn modifier" onclick={ouvrirEdition}><Icone nom="crayon" />Modifier</button>
      {/if}
    {:else}
      <span class="lecture-seule petit">Modifiable par l’administrateur</span>
    {/if}
  </div>
  <div class="meta-fiche"><FournEtat etat={fil.etat} /></div>
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

  {#if ed}
    <div class="cadre-form">
    <form class="form-fiche" onsubmit={enregistrer} aria-label="Modifier la fiche de {f.nom}">
      <h2 class="plein">Modifier la fiche</h2>
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
        {/if}
      {:else}
        <p class="petit t2 groupe-note">Cette fiche est un groupe : ses marques sont listées plus bas.</p>
      {/if}
      <div class="plein boutons">
        <button type="submit" class="btn principal" class:en-cours={enCours} disabled={enCours}>Enregistrer</button>
        <button type="button" class="btn" onclick={fermerEdition} disabled={enCours}>Annuler</button>
      </div>
    </form>
    </div>
  {:else}
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
    <div class="replis">
      <Repli icone="batiment" titre="Fiche du fournisseur" resume={resumeFiche} bind:ouvert={ficheOuverte}>
        <div class="lignes-champs">
          <Champ disposition="ligne" libelle="Code BO Phénix" valeur={f.code_gestion ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.code_gestion ?? '', true)}{/snippet}
          </Champ>
          {#if f.groupe_id !== null}
            <Champ disposition="ligne" libelle="Noms sur les documents" valeur={f.noms_documents.join(', ')}>
              {#snippet affichage()}{@render valeurOuTiret(f?.noms_documents.join(', ') ?? '')}{/snippet}
            </Champ>
          {/if}
          <Champ disposition="ligne" libelle="SIRET" valeur={f.siret ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.siret ?? '', true)}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="N° TVA" valeur={f.tva ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.tva ?? '', true)}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Adresse" valeur={f.adresse ?? ''}>
            {#snippet affichage()}{#if f?.adresse}<span class="multi">{f.adresse}</span>{:else}{@render valeurOuTiret('')}{/if}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Téléphone" valeur={f.telephone ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.telephone ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="E-mail de contact" valeur={f.email ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.email ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Interlocuteur" valeur={f.interlocuteur ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.interlocuteur ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Conditions de paiement" valeur={f.conditions_paiement ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.conditions_paiement ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Domaines mail" valeur={f.domaines.join(', ')}>
            {#snippet affichage()}{@render valeurOuTiret(f?.domaines.join(', ') ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Société par défaut" valeur={nomSocieteDefaut}>
            {#snippet affichage()}{@render valeurOuTiret(nomSocieteDefaut)}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Format des BL" valeur={f.format_bl ?? ''}>
            {#snippet affichage()}{@render valeurOuTiret(f?.format_bl ?? '')}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Notes" valeur={f.notes ?? ''}>
            {#snippet affichage()}{#if f?.notes}<span class="multi">{f.notes}</span>{:else}{@render valeurOuTiret('')}{/if}{/snippet}
          </Champ>
          <Champ disposition="ligne" libelle="Statut" valeur={f.statut === 'valide' ? 'Confirmé' : 'À confirmer'} />
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
  {/if}

  {#if f.groupe_id === null}
    <FournGroupe fiche={f} {admin} onmodifie={apresGroupe} />
  {/if}

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
  .multi {
    white-space: pre-line;
  }
  .titre-fiche .modifier {
    flex: none;
    align-self: center;
  }
  .lecture-seule {
    flex: none;
    color: var(--text2);
  }
  .cadre-form {
    container-type: inline-size;
  }
  .form-fiche {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--e4) var(--e5);
    margin-top: var(--e4);
    padding: var(--e4);
    border: 1px solid var(--line2);
    border-radius: var(--r2);
    background: var(--white);
  }
  .form-fiche > h2 {
    font-size: 14px;
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
  /* Volet étroit ou téléphone : une colonne. */
  @container (max-width: 520px) {
    .form-fiche {
      grid-template-columns: 1fr;
      padding: var(--e3);
    }
  }
</style>
