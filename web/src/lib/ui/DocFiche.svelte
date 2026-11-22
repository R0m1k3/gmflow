<script lang="ts" module>
  /** Ce qu'une action de la fiche a fait : la page ouvre `suivant` (ou revient à la file). */
  export interface ResultatFiche {
    id: number;
    suivant: number | null;
    info: string;
  }
  /** Position dans la file : « À valider 1 sur 12 ». */
  export interface PositionFiche {
    section: string;
    n: number;
    total: number;
  }
</script>

<script lang="ts">
  // Fiche document (maquette B, écrans 1 et 2), dans le volet de À traiter ou en plein écran :
  // aperçu à gauche ; à droite alerte en une phrase, champs modifiables au clic (douteux en ambre,
  // correction proposée en un clic), blocs repliés BL liés / Envoi / Mail d'origine / Lecture ;
  // barre d'action « Valider et envoyer à Dext » (POST /valider avec les corrections), refus 409
  // par code, « Écarter », « Relire avec Claude » (admins). Suit la lecture et l'envoi en cours.
  // Raccourcis : Entrée (action principale), E (écarter), F (plein écran, volet), Échap (plein écran).
  import { goto } from '$app/navigation';
  import { tick, untrack } from 'svelte';
  import { api, ecarterDocument, ErreurApi, message, modifierFournisseur, sonder, urlApercu, validerDocument } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import {
    libelleEtat,
    libelleStatutEnvoi,
    libelleStatutFournisseur,
    libelleType,
    libelleValider,
    libelleVoie,
    nomOutil,
    detailAlerte,
    titreAlerte,
    titreEnvoi,
    titreLiens,
    tonStatutEnvoi
  } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type {
    CodeRefus,
    Controle,
    CorrectionDocument,
    DetailRapprochement,
    FicheDocument,
    ChangementsFournisseur,
    LigneFournisseur,
    Page,
    TypeDocument
  } from '#lib/types.ts';
  import ApercuPiece from './ApercuPiece.svelte';
  import Bandeau from './Bandeau.svelte';
  import BarreAction, { type Action } from './BarreAction.svelte';
  import Champ from './Champ.svelte';
  import Chargement from './Chargement.svelte';
  import Confirmation from './Confirmation.svelte';
  import DocRapprochement, { resumeLiens } from './DocRapprochement.svelte';
  import {
    champsDeLAlerte,
    champsPeuSurs,
    dateFr,
    ecrireMontant,
    jourHeure,
    lireMontant,
    nomCourtDocument,
    nomExpediteur,
    phrase,
    tauxTva,
    type CleChamp
  } from './DocOutils.ts';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import Montant, { formatMontant } from './Montant.svelte';
  import Repli from './Repli.svelte';

  let {
    id,
    pleinEcran = false,
    retour = '/',
    position = null,
    ensuite,
    annonce = '',
    onfait,
    onchange,
    onprecedent,
    onsuivant
  }: {
    id: number;
    pleinEcran?: boolean;
    /** Lien « À traiter » (et Échap en plein écran). */
    retour?: string;
    position?: PositionFiche | null;
    /** « Sodistra, facture 2026/11/0457 » : ce qui s'ouvrira après. */
    ensuite?: string;
    /** Phrase laissée par l'action précédente (« Envoi à Dext demandé : … »). */
    annonce?: string;
    /** Validé, écarté : la page ouvre le suivant. */
    onfait: (r: ResultatFiche) => void;
    /** Autre changement (lien, relecture, fournisseur) : la page relit sa file. */
    onchange?: () => void;
    onprecedent?: () => void;
    onsuivant?: () => void;
  } = $props();

  const uid = $props.id();
  let admin = $derived(session.moi?.role === 'admin');

  // ---------- Chargement ----------
  type Choix = {
    type: string;
    societe: string;
    fournisseur: string;
    fournisseurNom: string;
    numero: string;
    date: string;
    commande: string;
    ht: string;
    tva: string;
    ttc: string;
  };
  const VIDE: Choix = { type: '', societe: '', fournisseur: '', fournisseurNom: '', numero: '', date: '', commande: '', ht: '', tva: '', ttc: '' };

  let d = $state<FicheDocument | null>(null);
  let rap = $state<DetailRapprochement | null>(null);
  let erreurChargement = $state('');
  /** 404 : document absent ou hors du périmètre ; rien à réessayer ni à valider. */
  let introuvable = $state(false);
  let erreur = $state('');
  let info = $state('');
  let refus = $state<{ code: CodeRefus | undefined; message: string } | null>(null);
  let occupe = $state<'valider' | 'forcer' | 'ecarter' | 'relire' | 'fournisseur' | null>(null);
  let choix = $state<Choix>({ ...VIDE });
  let lu = $state<Choix>({ ...VIDE });
  let ouvrirLiens = $state(false);
  let arretSuivi: (() => void) | null = null;

  function depuis(v: FicheDocument): Choix {
    return {
      type: v.type ?? '',
      societe: v.societe_id === null ? '' : String(v.societe_id),
      fournisseur: v.fournisseur_id === null ? '' : String(v.fournisseur_id),
      fournisseurNom: v.fournisseur ?? '',
      numero: v.numero ?? '',
      date: v.date_document ?? '',
      commande: v.numero_commande ?? '',
      ht: ecrireMontant(v.montant_ht),
      tva: ecrireMontant(v.montant_tva),
      ttc: ecrireMontant(v.montant_ttc)
    };
  }

  /** Validée, envoi demandé mais pas encore pris par le worker. */
  const envoiDemande = (v: FicheDocument) => !!v.valide_le && (v.statut_depot === 'pret' || v.statut_depot === 'a_verifier');
  const enCours = (v: FicheDocument) =>
    v.etat === 'a_lire' || v.etat === 'en_cours' || v.statut_depot === 'en_envoi' || envoiDemande(v) || v.envoi_relance;

  /** Relit la fiche ; garde les champs saisis si `garder`. Rend vrai s'il faut continuer à suivre. */
  async function charger(ident: number, garder: boolean): Promise<boolean> {
    const v = await api.get<FicheDocument>(`/api/documents/${ident}`);
    if (ident !== id) return false;
    const r = v.etat === 'lu' ? await api.get<DetailRapprochement>(`/api/documents/${ident}/rapprochement`).catch(() => null) : null;
    if (ident !== id) return false;
    const neuf = depuis(v);
    if (!garder || !modifie) choix = neuf;
    lu = neuf;
    d = v;
    rap = r;
    return enCours(v);
  }

  function suivre() {
    arretSuivi?.();
    const ident = id;
    arretSuivi = sonder(() => charger(ident, true));
  }

  $effect(() => {
    const ident = id;
    untrack(() => {
      d = null;
      rap = null;
      erreur = '';
      info = '';
      refus = null;
      erreurChargement = '';
      introuvable = false;
      choix = { ...VIDE };
      lu = { ...VIDE };
      editionF = false;
      charger(ident, false)
        .then((attendre) => {
          if (ident !== id) return;
          if (attendre) suivre();
          ouvrirLiens = pleinEcran && !!rap && (rap.candidats.length > 0 || rap.liens.some((l) => l.ecarts.length > 0));
        })
        .catch((e) => {
          if (ident !== id) return;
          introuvable = e instanceof ErreurApi && e.code === 404;
          erreurChargement = introuvable ? 'Document introuvable, ou hors de votre société.' : message(e);
        });
    });
    return () => {
      arretSuivi?.();
      arretSuivi = null;
    };
  });

  $effect(() => {
    if (!societe.charge) void societe.charger();
  });

  function reessayer() {
    erreurChargement = '';
    charger(id, false)
      .then((attendre) => attendre && suivre())
      .catch((e) => (erreurChargement = message(e)));
  }

  // ---------- Champs ----------
  const CLES: Record<CleChamp, keyof Choix> = {
    type: 'type',
    societe: 'societe',
    fournisseur: 'fournisseur',
    numero: 'numero',
    date: 'date',
    ht: 'ht',
    tva: 'tva',
    ttc: 'ttc'
  };
  const change = (k: CleChamp) => choix[CLES[k]] !== lu[CLES[k]];
  let modifie = $derived(
    (Object.keys(VIDE) as (keyof Choix)[]).some((k) => k !== 'fournisseurNom' && choix[k] !== lu[k])
  );

  let ht = $derived(lireMontant(choix.ht));
  let tva = $derived(lireMontant(choix.tva));
  let ttc = $derived(lireMontant(choix.ttc));
  let montantsCoherents = $derived(ht !== null && tva !== null && ttc !== null && Math.abs(ht + tva - ttc) <= 0.02);

  /** Une alerte est levée quand on a corrigé un de ses champs (et, pour les montants, qu'ils tombent juste). */
  function levee(c: Controle): boolean {
    const champs = champsDeLAlerte(c);
    if (!champs.some(change)) return false;
    return c.code !== 'montants' || montantsCoherents;
  }
  let alertes = $derived(d ? d.controles.filter((c) => c.code !== 'doublon' && !levee(c)) : []);
  let doublon = $derived(d?.controles.find((c) => c.code === 'doublon') ?? null);
  let peuSurs = $derived(d && d.etat === 'lu' && !d.verifie_le ? champsPeuSurs(d) : new Set<CleChamp>());

  function aControler(k: CleChamp): boolean | string {
    if (change(k)) return false;
    const c = alertes.find((a) => champsDeLAlerte(a).includes(k));
    if (c) return c === alertes[0] ? true : c.message;
    // Montants : la confiance ne surligne rien de plus quand une alerte les vise déjà.
    if ((k === 'ht' || k === 'tva' || k === 'ttc') && alertes.some((a) => a.code === 'montants')) return false;
    return peuSurs.has(k);
  }

  let fige = $derived(!d || d.etat !== 'lu' || d.statut_depot === 'en_envoi' || d.statut_depot === 'depose');
  let editable = $derived(!fige && occupe === null);
  let outil = $derived(d?.societe_outil_depot ?? null);
  let deposable = $derived(choix.type === 'facture' || choix.type === 'avoir');

  const TYPES: TypeDocument[] = ['facture', 'avoir', 'bl', 'relance', 'devis', 'autre'];
  let optionsType = $derived(TYPES.map((t) => ({ valeur: t, libelle: libelleType(t, true) })));
  let optionsSociete = $derived.by(() => {
    const o = [{ valeur: '', libelle: 'Aucune' }, ...societe.liste.map((s) => ({ valeur: String(s.id), libelle: s.raison_sociale }))];
    // La société du document reste choisie même si la liste ne la contient pas.
    if (d?.societe_id && !o.some((x) => x.valeur === String(d!.societe_id))) o.push({ valeur: String(d.societe_id), libelle: d.societe ?? `Société ${d.societe_id}` });
    return o;
  });
  let nomSocieteChoisie = $derived(optionsSociete.find((o) => o.valeur === choix.societe)?.libelle ?? '');

  /** Correction proposée en un clic sur un montant (calcul HT + TVA = TTC du serveur), tant que les montants lus sont là. */
  const MONTANTS: Partial<Record<string, 'ht' | 'tva' | 'ttc'>> = { montant_ht: 'ht', montant_tva: 'tva', montant_ttc: 'ttc' };
  let propositions = $derived.by(() => {
    const p: Partial<Record<'ht' | 'tva' | 'ttc', number>> = {};
    if (change('ht') || change('tva') || change('ttc')) return p;
    for (const c of alertes) {
      const k = c.proposition ? MONTANTS[c.proposition.champ] : undefined;
      if (k && c.proposition) p[k] = c.proposition.valeur;
    }
    return p;
  });

  function correctionComplete(type?: TypeDocument): CorrectionDocument {
    const lire = (s: string, nom: string) => {
      const v = lireMontant(s);
      if (v !== null && Number.isNaN(v)) throw new Error(`${nom} illisible : écrivez par exemple 1 250,40`);
      return v;
    };
    const ou = (s: string) => (s.trim() === '' ? null : s.trim());
    return {
      type: type ?? (choix.type as TypeDocument),
      societe_id: choix.societe === '' ? null : Number(choix.societe),
      fournisseur_id: choix.fournisseur === '' ? null : Number(choix.fournisseur),
      numero: ou(choix.numero),
      date_document: ou(choix.date),
      numero_commande: ou(choix.commande),
      montant_ht: lire(choix.ht, 'Total HT'),
      montant_tva: lire(choix.tva, 'TVA'),
      montant_ttc: lire(choix.ttc, 'Total TTC')
    };
  }

  // ---------- Fournisseur : recherche d'une fiche ----------
  let editionF = $state(false);
  let qF = $state('');
  let resultatsF = $state<LigneFournisseur[]>([]);
  let saisieF = $state<HTMLInputElement>();
  let tourF = 0;

  async function chercherF() {
    const moi = ++tourF;
    const q = qF.trim();
    try {
      const r = await api.get<Page<LigneFournisseur>>(`/api/fournisseurs${q ? `?q=${encodeURIComponent(q)}` : ''}`);
      if (moi === tourF) resultatsF = r.elements.slice(0, 8);
    } catch {
      /* la liste reste celle d'avant */
    }
  }
  async function ouvrirF() {
    if (!editable) return;
    qF = choix.fournisseurNom || d?.fournisseur_nom || '';
    editionF = true;
    void chercherF();
    await tick();
    saisieF?.focus();
    saisieF?.select();
  }
  function choisirF(f: { id: number; nom: string } | null) {
    choix.fournisseur = f ? String(f.id) : '';
    choix.fournisseurNom = f ? f.nom : '';
    editionF = false;
  }
  function toucheF(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      editionF = false;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      if (resultatsF[0]) choisirF(resultatsF[0]);
    }
  }

  /** Modifie la fiche fournisseur (admins) : PATCH des seuls champs changés. */
  async function majFournisseur(fid: number, changements: ChangementsFournisseur, phrase: string) {
    if (occupe) return;
    occupe = 'fournisseur';
    erreur = '';
    try {
      await modifierFournisseur(fid, changements);
      info = phrase;
      rangementFait = true;
      await charger(id, true);
      onchange?.();
      void rafraichirCompteurs();
    } catch (e) {
      erreur = message(e);
    } finally {
      occupe = null;
    }
  }
  let rangementFait = $state(false);
  $effect(() => {
    void id;
    rangementFait = false;
  });
  let proposerRangement = $derived(
    admin && !!d && change('societe') && choix.societe !== '' && choix.fournisseur !== '' && !rangementFait
  );

  // ---------- Actions ----------
  let titreDoc = $derived(
    nomCourtDocument({ fournisseur: choix.fournisseurNom || d?.fournisseur_nom || null, type: (choix.type || null) as TypeDocument | null, numero: choix.numero || null })
  );

  async function valider(forcer = false) {
    if (!d || occupe) return;
    let corrections: CorrectionDocument | null = null;
    try {
      if (modifie) corrections = correctionComplete();
    } catch (e) {
      erreur = message(e);
      return;
    }
    occupe = forcer ? 'forcer' : 'valider';
    erreur = '';
    info = '';
    refus = null;
    const quoi = titreDoc;
    try {
      const r = await validerDocument(d.id, { corrections, forcer: forcer || undefined }, societe.id);
      void rafraichirCompteurs();
      onfait({
        id: d.id,
        suivant: r.suivant,
        info: r.envoi ? `Envoi à ${nomOutil(outil)} demandé : ${quoi}` : `Validé : ${quoi}`
      });
    } catch (e) {
      if (e instanceof ErreurApi && e.code === 409) {
        refus = { code: e.raison, message: e.message };
        if (e.raison === 'deja_envoye' || e.raison === 'etat') await charger(d.id, true).catch(() => {});
      } else erreur = message(e);
    } finally {
      occupe = null;
    }
  }

  async function ecarter() {
    if (!d || occupe) return;
    occupe = 'ecarter';
    erreur = '';
    const quoi = titreDoc;
    try {
      const r = await ecarterDocument(d.id, societe.id);
      void rafraichirCompteurs();
      onfait({ id: d.id, suivant: r.suivant, info: `Écarté : ${quoi}` });
    } catch (e) {
      erreur = message(e);
    } finally {
      occupe = null;
    }
  }

  async function relire() {
    if (!d) return;
    erreur = '';
    try {
      await api.post(`/api/admin/documents/${d.id}/relire`);
      info = 'Relecture demandée : la fiche se met à jour toute seule.';
      await charger(d.id, false);
      suivre();
      onchange?.();
      void rafraichirCompteurs();
    } catch (e) {
      erreur = message(e);
    }
  }

  async function apresLien(phrase: string) {
    info = phrase;
    erreur = '';
    await charger(id, true);
    onchange?.();
    void rafraichirCompteurs();
  }

  let dlgEcarter = $state(false);
  let dlgForcer = $state(false);
  let dlgRelire = $state(false);

  // Plan de la barre d'action selon l'état du document.
  type Plan = { principal: Action; secondaires: Action[]; ecarter?: () => void };
  let plan = $derived.by((): Plan => {
    const occ = occupe !== null;
    if (!d) return { principal: { libelle: libelleValider(null, null), desactive: true }, secondaires: [] };
    if (d.etat === 'a_lire' || d.etat === 'en_cours') return { principal: { libelle: libelleEtat(d.etat), enCours: true }, secondaires: [] };
    if (d.etat === 'erreur') {
      const e = { libelle: 'Écarter', touche: 'Enter', onclick: ecarter, enCours: occupe === 'ecarter', desactive: occ };
      return { principal: e, secondaires: [], ecarter };
    }
    if (d.etat === 'ignore') {
      return admin
        ? { principal: { libelle: 'Relire avec Claude', onclick: () => (dlgRelire = true) }, secondaires: [] }
        : { principal: { libelle: 'Retour à À traiter', href: retour }, secondaires: [] };
    }
    if (d.statut_depot === 'en_envoi') return { principal: { libelle: libelleStatutEnvoi('en_envoi'), enCours: true }, secondaires: [] };
    if (d.statut_depot === 'depose' || envoiDemande(d) || d.envoi_relance) return { principal: { libelle: 'Retour à À traiter', href: retour }, secondaires: [] };
    if (refus?.code === 'doublon' || (d.statut_depot === 'doublon' && !d.verifie_le)) {
      const sec: Action[] = admin
        ? [{ libelle: 'Envoyer quand même', danger: true, onclick: () => (dlgForcer = true), enCours: occupe === 'forcer', desactive: occ }]
        : [];
      return { principal: { libelle: 'Écarter le doublon', touche: 'Enter', onclick: ecarter, enCours: occupe === 'ecarter', desactive: occ }, secondaires: sec, ecarter };
    }
    if (d.statut_depot === 'erreur') {
      return {
        principal: admin
          ? { libelle: `Renvoyer à ${nomOutil(outil)}`, touche: 'Enter', onclick: () => valider(), enCours: occupe === 'valider', desactive: occ }
          : { libelle: `Renvoyer à ${nomOutil(outil)}`, desactive: true },
        secondaires: []
      };
    }
    return {
      principal: {
        libelle: libelleValider((choix.type || null) as TypeDocument | null, outil),
        touche: 'Enter',
        onclick: () => valider(),
        enCours: occupe === 'valider',
        desactive: occ || !choix.type || editionF
      },
      secondaires: [{ libelle: 'Écarter', touche: 'E', onclick: () => (dlgEcarter = true), desactive: occ }],
      ecarter: () => (dlgEcarter = true)
    };
  });

  $effect(() => {
    const carte: Record<string, () => void | boolean> = {
      Enter: () => {
        const p = plan.principal;
        if (p.desactive || p.enCours) return;
        if (p.href) void goto(p.href);
        else p.onclick?.();
      },
      e: () => (plan.ecarter ? plan.ecarter() : false)
    };
    if (pleinEcran) carte.Escape = () => void goto(retour);
    else carte.f = () => void goto(`/documents/${id}`);
    return raccourcis(carte);
  });

  // ---------- Affichage ----------
  let titre = $derived.by(() => {
    if (!d) return '';
    const f = choix.fournisseurNom || d.fournisseur_nom || '';
    return `${libelleType((choix.type || null) as TypeDocument | null)}${f ? ` ${f}` : ''}`;
  });
  let dernierEnvoi = $derived(d?.envois[0] ?? null);
  let resumeEnvoi = $derived.by(() => {
    if (!d) return '';
    const fem = choix.type === 'facture' ? 'e' : '';
    const dest = d.societe_adresse_depot ? `vers ${d.societe_adresse_depot}` : 'pas d’adresse Dext / Yooz';
    const s = d.statut_depot;
    const etat =
      s === 'depose' && dernierEnvoi
        ? `envoyé${fem} le ${jourHeure(dernierEnvoi.quand)}`
        : s === 'erreur' || s === 'en_envoi' || s === 'doublon'
          ? libelleStatutEnvoi(s, outil).toLowerCase()
          : d.valide_le
            ? 'envoi demandé'
            : `pas encore envoyé${fem}`;
    return `${dest}, ${etat}`;
  });
  let corrections = $derived(d ? Object.entries(d.corrections ?? {}) : []);
  const NOMS_CHAMPS: Record<string, string> = {
    type: 'type',
    societe_id: 'société',
    fournisseur_id: 'fournisseur',
    numero: 'n°',
    date_document: 'date',
    numero_commande: 'n° de commande',
    montant_ht: 'HT',
    montant_tva: 'TVA',
    montant_ttc: 'TTC'
  };
  const brut = (v: unknown) => (v === null || v === undefined || v === '' ? 'vide' : String(v));
  const quantite = (v: number | null) => (v === null ? '—' : v.toLocaleString('fr-FR'));
  let estBl = $derived(choix.type === 'bl');
</script>

{#snippet montantLu(s: string)}
  {#if s === ''}
    <span class="vide-val">Non lu</span>
  {:else if Number.isNaN(lireMontant(s))}
    <span class="num">{s}</span><span class="indice">montant illisible</span>
  {:else}
    <Montant valeur={lireMontant(s)} />
  {/if}
{/snippet}

{#snippet proposer(k: 'ht' | 'tva' | 'ttc')}
  {@const v = propositions[k]}
  {#if v !== undefined && editable}
    <button type="button" class="btn sm doc-proposer" onclick={() => (choix[k] = ecrireMontant(v))}>Mettre {formatMontant(v)}</button>
  {/if}
{/snippet}

{#snippet lectureF()}
  {#if choix.fournisseurNom}<span>{choix.fournisseurNom}</span>{:else}<span class="vide-val">Pas de fiche fournisseur</span>{/if}
  {#if d?.fournisseur_nom && d.fournisseur_nom.toLowerCase() !== choix.fournisseurNom.toLowerCase()}
    <span class="indice">lu « {d.fournisseur_nom} »</span>
  {/if}
  {#if d?.fournisseur_statut === 'a_confirmer' && !change('fournisseur')}
    <span class="source">{libelleStatutFournisseur('a_confirmer')}</span>
  {/if}
{/snippet}

<div class="volet-h">
  {#if pleinEcran}
    <a class="retour" href={retour}><Icone nom="gauche" />À traiter <Kbd t="Escape" /></a>
    <span class="vsep"></span>
  {:else}
    <a class="retour tel-seul" href={retour}><Icone nom="gauche" />À traiter</a>
  {/if}
  {#if position}
    <span class="pos">{position.section} <span class="num">{position.n}</span> sur <span class="num">{position.total}</span></span>
  {/if}
  <span class="sp"></span>
  {#if admin && d && d.etat !== 'a_lire' && d.etat !== 'en_cours' && d.etat !== 'ignore' && d.statut_depot !== 'en_envoi'}
    <button type="button" class="btn sm fantome large-seul" onclick={() => (dlgRelire = true)}><Icone nom="rafraichir" />Relire avec Claude</button>
    <span class="vsep"></span>
  {/if}
  {#if pleinEcran && d}
    {#if /^(application\/pdf|image\/)/.test(d.type_mime)}
      <a class="btn sm fantome large-seul" href={urlApercu(d.piece_id)} target="_blank" rel="noopener"><Icone nom="zoom" />Zoom</a>
    {/if}
    <a class="btn sm fantome" href="/api/pieces/{d.piece_id}" download={d.nom_fichier}>
      <Icone nom="telecharger" />{d.type_mime === 'application/pdf' ? 'PDF' : 'Fichier'}
    </a>
    {#if onprecedent || onsuivant}<span class="vsep"></span>{/if}
  {/if}
  {#if onprecedent}
    <button type="button" class="btn sm fantome" aria-label="Précédent" onclick={onprecedent}><Icone nom="haut" /><Kbd t="K" /></button>
  {/if}
  {#if onsuivant}
    <button type="button" class="btn sm fantome" aria-label="Suivant" onclick={onsuivant}><Icone nom="bas" /><Kbd t="J" /></button>
  {/if}
  {#if !pleinEcran}
    <span class="vsep"></span>
    <a class="btn sm fantome large-seul" href="/documents/{id}"><Icone nom="agrandir" />Plein écran <Kbd t="F" /></a>
  {/if}
</div>

<div class="volet-b">
  <div class="apercu-zone" class:sr={erreurChargement !== ''}>
    {#if d}<ApercuPiece id={d.piece_id} typeMime={d.type_mime} nom={d.nom_fichier} outils={false} />{/if}
  </div>
  <div class="contenu-fiche avec-barre">
    {#if annonce}<Bandeau ton="ok" role="status">{annonce}</Bandeau>{/if}
    {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}

    {#if erreurChargement}
      <Bandeau ton="erreur">
        {erreurChargement}
        {#snippet action()}
          {#if introuvable}<a class="btn sm" href={retour}>Retour à À traiter</a>{:else}<button type="button" class="btn sm" onclick={reessayer}>Réessayer</button>{/if}
        {/snippet}
      </Bandeau>
    {:else if !d}
      <Chargement texte="Chargement du document" />
    {:else}
      <div class="titre-fiche"><h1>{titre}</h1>{#if choix.numero}<span class="num">{choix.numero}</span>{/if}</div>
      <div class="meta-fiche">
        Reçue le {jourHeure(d.recu_le)}{d.mail_expediteur ? ` de ${nomExpediteur(d.mail_expediteur)}` : ''}{#if admin && d.voie}, {libelleVoie(d.voie)}{/if}{#if d.valide_le}, validation le {jourHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}{:else if d.verifie_le}, validation le {jourHeure(d.verifie_le)}{d.verifie_par ? ` par ${d.verifie_par}` : ''}{/if}
        {#if d.copie_de}<span class="petit">· même fichier que le <a href="/documents/{d.copie_de}">document {d.copie_de}</a></span>{/if}
      </div>

      <!-- Une phrase en tête : refus, état, ou première alerte -->
      {#if refus}
        {#if refus.code === 'doublon'}
          <Bandeau ton="erreur" titre="Doublon.">{phrase(refus.message)}</Bandeau>
        {:else if refus.code === 'expedition'}
          <Bandeau ton="erreur" titre="Envoi impossible.">
            L’adresse d’envoi vers {nomOutil(outil)} n’est pas encore configurée.
            {#snippet action()}{#if admin}<a class="btn sm" href="/reglages">Ouvrir les réglages</a>{/if}{/snippet}
          </Bandeau>
        {:else if refus.code === 'sans_adresse'}
          <Bandeau ton="erreur" titre="Envoi impossible.">
            {d.societe ?? 'La société'} n’a pas d’adresse {nomOutil(outil)}.{admin ? '' : ' Un administrateur doit la renseigner.'}
            {#snippet action()}{#if admin}<a class="btn sm" href="/reglages">Ouvrir les réglages</a>{/if}{/snippet}
          </Bandeau>
        {:else}
          <Bandeau ton="erreur">{phrase(refus.message)}</Bandeau>
        {/if}
      {/if}
      {#if d.etat === 'a_lire' || d.etat === 'en_cours'}
        <Bandeau ton="info" titre="{libelleEtat(d.etat)}.">La fiche se met à jour toute seule.</Bandeau>
      {:else if d.etat === 'erreur'}
        <Bandeau ton="erreur" titre="Illisible.">{d.message ? phrase(d.message) : 'La lecture a échoué.'}{admin ? ' Relisez-le avec Claude, ou écartez-le.' : ''}</Bandeau>
      {:else if d.etat === 'ignore'}
        <Bandeau ton="neutre" titre="{libelleEtat('ignore')}.">Ce document n’est ni une facture, ni un avoir, ni un BL.{d.message ? ` ${d.message}` : ''}</Bandeau>
      {:else if d.statut_depot === 'en_envoi'}
        <Bandeau ton="info" titre="{libelleStatutEnvoi('en_envoi')}.">La fiche se met à jour toute seule.</Bandeau>
      {:else if envoiDemande(d)}
        <Bandeau ton="info" titre="Envoi à {nomOutil(outil)} demandé.">Validée le {jourHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''} : elle part dans quelques instants.</Bandeau>
      {:else if d.statut_depot === 'depose'}
        <Bandeau ton="ok" titre="{libelleStatutEnvoi('depose', outil)}.">{dernierEnvoi ? `Le ${jourHeure(dernierEnvoi.quand)}, à ${dernierEnvoi.adresse}.` : ''}</Bandeau>
      {:else if d.envoi_relance}
        <Bandeau ton="info" titre="Envoi à {nomOutil(outil)} relancé.">Relancé le {jourHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''} : il part dans quelques instants.</Bandeau>
      {:else if d.statut_depot === 'erreur'}
        <Bandeau ton="erreur" titre="Envoi à {nomOutil(outil)} échoué.">
          {dernierEnvoi?.message ? phrase(dernierEnvoi.message) : ''}{admin ? '' : ' Un administrateur peut la renvoyer.'}
        </Bandeau>
      {:else if doublon && !refus && !d.verifie_le}
        <Bandeau ton="erreur" titre="Doublon.">{phrase(doublon.message)}</Bandeau>
      {:else if alertes.length && !refus}
        <Bandeau titre="{titreAlerte(alertes[0])}.">
          {#if detailAlerte(alertes[0])}{phrase(detailAlerte(alertes[0]))}{/if}{#if alertes.length > 1}
            {' '}{alertes.length - 1 === 1 ? 'Une autre alerte' : `${alertes.length - 1} autres alertes`}, surlignée{alertes.length > 2 ? 's' : ''} dans les champs.{/if}
        </Bandeau>
      {/if}

      {#if d.etat === 'lu'}
        {@const ctrlF = aControler('fournisseur')}
        <div class="lignes-champs">
          <Champ
            disposition="ligne"
            libelle="Type"
            type="select"
            bind:valeur={choix.type}
            options={optionsType}
            modifiable={editable}
            aControler={aControler('type')}
          />
          <Champ
            disposition="ligne"
            libelle="Société"
            type="select"
            bind:valeur={choix.societe}
            options={optionsSociete}
            modifiable={editable}
            aControler={aControler('societe')}
          />
          <div class="champ-ligne" class:a-controler={!!ctrlF && !editionF}>
            {#if editionF}
              <label class="k" for="{uid}-f">Fournisseur</label>
              <div class="v edition">
                <input
                  id="{uid}-f"
                  type="search"
                  bind:this={saisieF}
                  bind:value={qF}
                  oninput={chercherF}
                  onkeydown={toucheF}
                  onblur={() => (editionF = false)}
                  placeholder="Nom, SIRET ou domaine"
                  autocomplete="off"
                />
              </div>
              <div class="doc-choix" role="listbox" aria-label="Fiches fournisseur">
                {#each resultatsF as f, i (f.id)}
                  <button type="button" role="option" aria-selected={i === 0} class="btn sm fantome" onpointerdown={(e) => e.preventDefault()} onclick={() => choisirF(f)}>
                    {f.nom}{#if f.statut === 'a_confirmer'}<span class="muted">nouveau</span>{/if}
                    {#if i === 0}<Kbd t="Enter" />{/if}
                  </button>
                {:else}
                  <span class="muted petit">Aucune fiche trouvée</span>
                {/each}
                {#if choix.fournisseur}
                  <button type="button" role="option" aria-selected="false" class="btn sm fantome" onpointerdown={(e) => e.preventDefault()} onclick={() => choisirF(null)}>Aucune fiche</button>
                {/if}
              </div>
            {:else}
              <div class="k">Fournisseur</div>
              <div class="v" class:modifiable={editable}>
                {#if editable}
                  <button type="button" class="val" onclick={ouvrirF} aria-label="Modifier Fournisseur">{@render lectureF()}</button>
                {:else}
                  <span class="val">{@render lectureF()}</span>
                {/if}
                {#if admin && d.fournisseur_id && d.fournisseur_statut === 'a_confirmer' && !change('fournisseur')}
                  <button type="button" class="btn sm" class:en-cours={occupe === 'fournisseur'} disabled={occupe !== null} onclick={() => majFournisseur(d!.fournisseur_id!, { statut: 'valide' }, `Fiche ${choix.fournisseurNom} confirmée`)}>
                    Confirmer la fiche
                  </button>
                {/if}
              </div>
              <!-- « Nouveau fournisseur (à confirmer) » est déjà écrit à côté du nom -->
              {#if typeof ctrlF === 'string' && !(d.fournisseur_statut === 'a_confirmer' && ctrlF.includes('confirmer'))}<div class="aide">{ctrlF}</div>{/if}
            {/if}
          </div>
          <Champ disposition="ligne" libelle="N°" bind:valeur={choix.numero} mono modifiable={editable} aControler={aControler('numero')} />
          <Champ disposition="ligne" libelle="Date" type="date" bind:valeur={choix.date} mono modifiable={editable} aControler={aControler('date')}>
            {#snippet affichage()}
              {#if choix.date}<span class="num">{dateFr(choix.date)}</span>{:else}<span class="vide-val">Non lue</span>{/if}
            {/snippet}
          </Champ>
          {#if choix.commande || lu.commande}
            <Champ disposition="ligne" libelle="Commande" bind:valeur={choix.commande} mono modifiable={editable} />
          {/if}
          {#if !estBl || choix.ht || lu.ht}
            <Champ disposition="ligne" libelle="Total HT" bind:valeur={choix.ht} mono inputmode="decimal" modifiable={editable} aControler={aControler('ht')}>
              {#snippet affichage()}{@render montantLu(choix.ht)}{/snippet}
              {#snippet apres()}{@render proposer('ht')}{/snippet}
            </Champ>
          {/if}
          {#if !estBl || choix.tva || lu.tva}
            <Champ disposition="ligne" libelle="TVA" bind:valeur={choix.tva} mono inputmode="decimal" modifiable={editable} aControler={aControler('tva')} indice={tauxTva(ht, tva) || undefined}>
              {#snippet affichage()}{@render montantLu(choix.tva)}{/snippet}
              {#snippet apres()}{@render proposer('tva')}{/snippet}
            </Champ>
          {/if}
          {#if !estBl || choix.ttc || lu.ttc}
            <Champ disposition="ligne" libelle="Total TTC" bind:valeur={choix.ttc} mono inputmode="decimal" modifiable={editable} aControler={aControler('ttc')}>
              {#snippet affichage()}{@render montantLu(choix.ttc)}{/snippet}
              {#snippet apres()}{@render proposer('ttc')}{/snippet}
            </Champ>
          {/if}
        </div>

        {#if proposerRangement}
          <div class="doc-question">
            <Bandeau ton="info">
              Toujours ranger {choix.fournisseurNom} dans {nomSocieteChoisie} ?
              {#snippet action()}
                <button type="button" class="btn sm" class:en-cours={occupe === 'fournisseur'} disabled={occupe !== null} onclick={() => majFournisseur(Number(choix.fournisseur), { societe_defaut_id: Number(choix.societe) }, `${choix.fournisseurNom} sera rangé dans ${nomSocieteChoisie}`)}>Oui</button>
              {/snippet}
            </Bandeau>
          </div>
        {/if}
        {#if modifie && !fige}
          <p class="petit t2 doc-note">Corrections gardées à la validation.
            <button type="button" class="btn-lien" onclick={() => (choix = { ...lu })}>Revenir à la lecture</button>
          </p>
        {/if}
      {/if}

      <div class="replis">
        {#if d.etat === 'lu' && rap && (d.type === 'facture' || d.type === 'bl')}
          <Repli icone="camion" titre={titreLiens(d.type)} resume={resumeLiens(d, rap)} bind:ouvert={ouvrirLiens}>
            <DocRapprochement {d} {rap} onchange={apresLien} onerreur={(m) => (erreur = m)} />
          </Repli>
        {/if}
        {#if d.statut_depot || deposable}
          <Repli icone="envoyer" titre={titreEnvoi(outil)} resume={resumeEnvoi}>
            <div class="doc-envoi">
              {#if d.societe_adresse_depot}
                <p>Destination : <span class="num">{d.societe_adresse_depot}</span>{d.societe ? `, ${d.societe}` : ''}</p>
              {:else}
                <p class="t2">{d.societe ?? 'La société'} n’a pas d’adresse {nomOutil(outil)}.{#if admin} <a href="/reglages">Réglages</a>{/if}</p>
              {/if}
              {#if d.statut_depot}
                <p><Etiquette ton={tonStatutEnvoi(d.statut_depot)}>{libelleStatutEnvoi(d.statut_depot, outil)}</Etiquette>
                  {#if d.valide_le}<span class="t2 petit">validée le {jourHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}</span>{/if}
                </p>
              {/if}
            </div>
            {#if d.envois.length}
              <table>
                <thead><tr><th>Envoi</th><th>Adresse</th><th>Résultat</th><th>Par</th></tr></thead>
                <tbody>
                  {#each d.envois as e (e.id)}
                    <tr>
                      <td class="num">{jourHeure(e.quand)}</td>
                      <td class="num">{e.adresse}</td>
                      <td>
                        <Etiquette ton={e.etat === 'envoye' ? 'ok' : e.etat === 'erreur' ? 'erreur' : 'neutre'}>{e.etat === 'envoye' ? 'Envoyé' : e.etat === 'erreur' ? 'Échec' : 'En cours'}</Etiquette>
                        {#if e.message}<div class="t2 petit">{e.message}</div>{/if}
                      </td>
                      <td class="t2">{e.auteur ?? 'automatique'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {:else}
              <p class="t2 petit">Aucun envoi pour l’instant.</p>
            {/if}
          </Repli>
        {/if}
        <Repli icone="mail" titre="Mail d’origine" resume="{nomExpediteur(d.mail_expediteur) || 'Expéditeur inconnu'}{`, ${jourHeure(d.recu_le)}`}">
          <div class="doc-mail">
            <p><span class="t2">De</span> {d.mail_expediteur || '—'}</p>
            {#each d.boites as b (b.boite_id)}
              <p><span class="t2">À</span> {b.adresse}{b.libelle && b.libelle !== b.adresse ? ` (${b.libelle})` : ''}</p>
            {/each}
            <p><span class="t2">Objet</span> {d.mail_objet || '(sans objet)'}</p>
            <p><span class="t2">Pièce</span> {d.nom_fichier}</p>
            <p><a class="btn sm" href="/mails/{d.mail_id}"><Icone nom="mail" />Ouvrir le mail</a></p>
          </div>
        </Repli>
        {#if d.etat === 'lu' && (rap?.lignes.length || d.texte || corrections.length || admin)}
          <Repli icone="fichier" titre="Lecture" resume={rap?.lignes.length ? `${rap.lignes.length} ligne${rap.lignes.length > 1 ? 's' : ''} lue${rap.lignes.length > 1 ? 's' : ''}` : d.lu_le ? `lu le ${jourHeure(d.lu_le)}` : ''}>
            {#if admin && d.voie}
              <p class="t2 petit">{libelleVoie(d.voie)}{d.modele ? ` (${d.modele}${d.effort ? `, ${d.effort}` : ''})` : ''}{d.lu_le ? `, le ${jourHeure(d.lu_le)}` : ''}</p>
            {/if}
            {#if corrections.length}
              <p class="t2 petit">Corrigé à la main : {#each corrections as [champ, c], i (champ)}{i ? ' ; ' : ''}{NOMS_CHAMPS[champ] ?? champ} ({brut(c.avant)} → {brut(c.apres)}){/each}</p>
            {/if}
            {#if rap?.lignes.length}
              <table>
                <thead><tr><th>Réf.</th><th>Désignation</th><th class="n">Qté</th><th class="n">PU HT</th><th class="n">Montant HT</th></tr></thead>
                <tbody>
                  {#each rap.lignes as l (l.rang)}
                    <tr>
                      <td class="num">{l.reference ?? '—'}</td>
                      <td>{l.designation ?? '—'}</td>
                      <td class="n num">{quantite(l.quantite)}{l.unite ? ` ${l.unite}` : ''}</td>
                      <td class="n"><Montant valeur={l.prix_unitaire_ht} devise="" /></td>
                      <td class="n"><Montant valeur={l.montant_ht} devise="" /></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
            {#if d.texte}
              <details class="doc-texte">
                <summary class="t2 petit">Texte lu</summary>
                <pre>{d.texte}</pre>
              </details>
            {/if}
          </Repli>
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if !erreurChargement}
  <BarreAction principal={plan.principal} secondaires={plan.secondaires} {ensuite} toucheEnsuite={pleinEcran && ensuite && onsuivant ? 'J' : undefined} aligne={pleinEcran} />
{/if}

<Confirmation bind:ouvert={dlgEcarter} titre="Écarter ce document ?" libelle="Écarter" danger onconfirmer={ecarter}>
  <p>{titreDoc} sera marqué « {libelleEtat('ignore')} » : {deposable ? `pas d’envoi à ${nomOutil(outil)}, ` : ''}il quitte la file. Il reste visible dans la recherche.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgForcer} titre="Envoyer ce doublon quand même ?" libelle="Envoyer quand même" danger onconfirmer={() => valider(true)}>
  <p>{refus?.code === 'doublon' ? phrase(refus.message) : doublon ? phrase(doublon.message) : ''} La facture partira à {nomOutil(outil)} telle quelle.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgRelire} titre="Relire avec Claude ?" libelle="Relire" onconfirmer={relire}>
  <p>La lecture et la validation actuelles seront remplacées par une nouvelle lecture.</p>
</Confirmation>

<style>
  .doc-proposer {
    margin-left: auto;
  }
  .doc-choix {
    grid-column: 2;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 0 0 var(--e2);
  }
  .doc-question {
    margin-top: var(--e3);
  }
  .doc-note {
    margin: var(--e2) 0 0;
  }
  .doc-envoi p,
  .doc-mail p {
    margin: 0 0 var(--e2);
  }
  .doc-mail .t2 {
    display: inline-block;
    width: 52px;
  }
  .doc-texte pre {
    white-space: pre-wrap;
    font: 12px/18px var(--mono);
    color: var(--text2);
    max-height: 320px;
    overflow: auto;
    margin: var(--e2) 0 0;
  }
  .doc-texte summary {
    cursor: pointer;
    margin-top: var(--e2);
  }
</style>
