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
  // Fiche document (maquette Sauge S1, S4, S5), dans le volet d'une liste ou en plein écran :
  // aperçu au centre (barre : position, pages, zoom, plein écran ; la page en carte blanche) ;
  // colonne à droite : nom, « Facture n°, reçue le … de … », alerte en une phrase, champs
  // modifiables au clic (douteux en ambre, correction proposée en un clic ; Type et N° seulement
  // s'ils sont douteux, vides ou demandés) ; sous les bandeaux, la chaîne commande → BL → facture
  // → Phénix / Dext → Logiflow (DocChaine : montants de la pièce, pièces liées dépliées sur place),
  // puis « Détails de la pièce », Export Phénix (BL), Origine (mail, lecture, fournisseur) ;
  // « Relire avec Claude », « Relire en détail » ; en pied « Valider et envoyer à Dext » (POST /valider
  // avec les corrections), refus 409 par code, « Écarter », « Ensuite : … ». Suit la lecture et l'envoi en cours.
  // Raccourcis : Entrée (action principale), E (écarter), F (plein écran, volet), Échap (plein écran).
  import { goto } from '$app/navigation';
  import { tick, untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
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
    nomCourtSociete,
    nomOutil,
    detailAlerte,
    titreAlerte,
    titreEnvoi,
    tonStatutEnvoi,
    type Ton
  } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type {
    CodeRefus,
    Controle,
    CorrectionDocument,
    DetailRapprochement,
    EtatLivraisonLogiflow,
    FicheDocument,
    ChangementsFournisseur,
    Fournisseur,
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
  import DocArticlesBl from './DocArticlesBl.svelte';
  import DocChaine, { type FactureLiee, type PointChaine } from './DocChaine.svelte';
  import DocExportPhenix, { resumeExport } from './DocExportPhenix.svelte';
  import type { EtatExportPhenix } from '#lib/types.ts';
  import {
    typeNumero,
    champsDeLAlerte,
    champsPeuSurs,
    dateFr,
    ecrireMontant,
    jourHeure,
    lireMontant,
    nomCourtDocument,
    nomExpediteur,
    phrase,
    quandCourt,
    tauxTva,
    phraseEcartLigne,
    type CleChamp
  } from './DocOutils.ts';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import Montant, { formatMontant } from './Montant.svelte';
  import Repli from './Repli.svelte';
  import Poignee from './Poignee.svelte';
  import { largeurGardee } from '#lib/largeurs.ts';

  let {
    id,
    pleinEcran = false,
    retour = '/',
    retourLibelle = 'Accueil',
    position = null,
    ensuite,
    sansSuite,
    annonce = '',
    onfait,
    onchange,
    onprecedent,
    onsuivant
  }: {
    id: number;
    pleinEcran?: boolean;
    /** Lien de retour (et Échap en plein écran). */
    retour?: string;
    /** Nom de la page de retour : « Factures », « BL »… */
    retourLibelle?: string;
    position?: PositionFiche | null;
    /** « Sodistra, facture 2026/11/0457 » : ce qui s'ouvrira après. */
    ensuite?: string;
    /** « Aucune autre facture dans cette liste » : pied de la fiche quand rien ne suit. */
    sansSuite?: string;
    /** Phrase laissée par l'action précédente (« Envoi à Dext demandé : … »). */
    annonce?: string;
    /** Validé, écarté : la page ouvre le suivant. */
    onfait: (r: ResultatFiche) => void;
    /** Autre changement (lien, relecture, fournisseur) : la page relit sa file. */
    onchange?: () => void;
    onprecedent?: () => void;
    onsuivant?: () => void;
  } = $props();

  // Fiche élargissable à la souris (poignée entre l'aperçu et la fiche), une largeur pour la vue
  // en volets et une pour le plein écran.
  let cleFiche = $derived(pleinEcran ? 'fiche.pleine' : 'fiche');
  let defautFiche = $derived(pleinEcran ? 400 : 360);
  let largFiche = $state(360);
  let ficheChoisie = $state(false);
  $effect.pre(() => {
    largFiche = largeurGardee(cleFiche, defautFiche);
    ficheChoisie = largeurGardee(cleFiche, 0) > 0;
  });

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
    echeance: string;
    ht: string;
    tva: string;
    ttc: string;
  };
  const VIDE: Choix = { type: '', societe: '', fournisseur: '', fournisseurNom: '', numero: '', date: '', commande: '', echeance: '', ht: '', tva: '', ttc: '' };

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
  /** Maillons dépliés de la chaîne (DocChaine). */
  let ouverts = $state(new SvelteSet<string>());
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
      echeance: v.date_echeance ?? '',
      ht: ecrireMontant(v.montant_ht),
      tva: ecrireMontant(v.montant_tva),
      ttc: ecrireMontant(v.montant_ttc)
    };
  }

  /** Validée, envoi demandé mais pas encore pris par le worker. */
  const envoiDemande = (v: FicheDocument) => !!v.valide_le && (v.statut_depot === 'pret' || v.statut_depot === 'a_verifier');
  const enCours = (v: FicheDocument) =>
    v.etat === 'a_lire' ||
    v.etat === 'en_cours' ||
    v.statut_depot === 'en_envoi' ||
    envoiDemande(v) ||
    v.envoi_relance ||
    v.logiflow_attendu ||
    (attenteLogiflow && !v.logiflow);

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

  /**
   * BL : fiche et rapprochement de chaque facture liée (troisième temps, après la fiche puis le
   * rapprochement) ; au chargement et après un lien, pas pendant le suivi. Jeté si la fiche a changé.
   */
  let factures = $state.raw(new Map<number, FactureLiee>());
  async function chargerFactures(ident: number) {
    if (ident !== id) return;
    if (!d || d.etat !== 'lu' || d.type !== 'bl' || !rap?.liens.length) {
      factures = new Map();
      return;
    }
    const ids = [...new Set(rap.liens.map((l) => l.autre.id))];
    const lues = await Promise.all(
      ids.map(async (fid): Promise<[number, FactureLiee]> => {
        const [f, r] = await Promise.allSettled([
          api.get<FicheDocument>(`/api/documents/${fid}`),
          api.get<DetailRapprochement>(`/api/documents/${fid}/rapprochement`)
        ]);
        return [fid, { fiche: f.status === 'fulfilled' ? f.value : null, rap: r.status === 'fulfilled' ? r.value : null }];
      })
    );
    if (ident !== id) return;
    factures = new Map(lues);
  }

  /** « Relancer » Logiflow cliqué : la fiche suit jusqu'au nouveau résultat. */
  let attenteLogiflow = $state(false);
  const LIBELLES_LOGIFLOW: Record<EtatLivraisonLogiflow, { texte: string; ton: Ton }> = {
    complete: { texte: 'Complétée', ton: 'ok' },
    deja_fait: { texte: 'Déjà faite', ton: 'ok' },
    deja_rapproche: { texte: 'Déjà rapprochée', ton: 'neutre' },
    autre_facture: { texte: 'Autre facture', ton: 'alerte' },
    introuvable: { texte: 'Introuvable', ton: 'alerte' },
    ambigu: { texte: 'À choisir', ton: 'alerte' },
    erreur: { texte: 'Échec', ton: 'erreur' }
  };

  async function relancerLogiflow() {
    if (!d) return;
    erreur = '';
    try {
      await api.post(`/api/documents/${d.id}/logiflow`, {});
      attenteLogiflow = true;
      info = 'Recherche relancée dans Logiflow : le résultat s’affiche dans quelques secondes.';
      await charger(d.id, true);
      suivre();
    } catch (e) {
      erreur = message(e);
    }
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
      voirTN = false;
      ouverts.clear();
      factures = new Map();
      charger(ident, false)
        .then((attendre) => {
          if (ident !== id) return;
          if (attendre) suivre();
          void chargerFactures(ident);
          // Plein écran : sur un BL, les factures liées qui ont des écarts d'articles sont dépliées.
          if (pleinEcran && rap && d?.type === 'bl') for (const l of rap.liens) if (l.ecarts.length) ouverts.add(`facture-${l.autre.id}`);
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
    const ident = id;
    charger(ident, false)
      .then((attendre) => {
        if (attendre) suivre();
        void chargerFactures(ident);
      })
      .catch((e) => (erreurChargement = message(e)));
  }

  // ---------- Aperçu : zoom de la page ----------
  const ZOOMS = [0.5, 0.67, 0.8, 1, 1.25, 1.5, 2];
  let zoom = $state(1);
  let pages = $state<number | null>(null);
  let zoomable = $state(false);
  function zoomer(sens: 1 | -1) {
    const i = ZOOMS.indexOf(zoom);
    zoom = ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, (i < 0 ? 3 : i) + sens))];
  }

  // ---------- Code BO Phénix du fournisseur (fiche fournisseur) ----------
  let codeBo = $state<{ id: number; code: string | null } | null>(null);
  $effect(() => {
    const fid = d?.fournisseur_id ?? null;
    if (fid === null || codeBo?.id === fid) return;
    api
      .get<Fournisseur>(`/api/fournisseurs/${fid}`)
      .then((f) => {
        if (d?.fournisseur_id === fid) codeBo = { id: fid, code: f.code_gestion };
      })
      .catch(() => {});
  });

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
  // Facture : ses écarts d'articles une fois (le serveur écrit les mêmes sur tous ses liens) ; BL :
  // ceux de chacune de ses factures.
  let articlesEcart = $derived(
    rap ? (d?.type === 'bl' ? rap.liens.flatMap((l) => l.ecarts) : (rap.liens[0]?.ecarts ?? [])).map(phraseEcartLigne) : []
  );
  let doublon = $derived(d?.controles.find((c) => c.code === 'doublon') ?? null);
  /** Société non reconnue ou douteuse : bandeau à part, avec le choix de la société ou le rejet. */
  let alerteSociete = $derived(alertes.find((c) => c.code === 'societe') ?? null);
  let autresAlertes = $derived(alertes.filter((c) => c !== alerteSociete));
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
      montant_ttc: lire(choix.ttc, 'Total TTC'),
      date_echeance: ou(choix.echeance)
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

  async function relire(approfondie = false) {
    if (!d) return;
    erreur = '';
    try {
      await api.post(`/api/admin/documents/${d.id}/relire`, { approfondie });
      info = approfondie
        ? 'Relecture en détail demandée (modèle plus fort, PDF entier) : la fiche se met à jour toute seule, compter une à deux minutes.'
        : 'Relecture demandée : la fiche se met à jour toute seule.';
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
    const ident = id;
    await charger(ident, true);
    void chargerFactures(ident);
    onchange?.();
    void rafraichirCompteurs();
  }

  /** « Lire maintenant » (admins) : la lecture passe devant toute la file, hors quota du jour. */
  async function lireMaintenant() {
    if (!d) return;
    erreur = '';
    occupe = 'relire';
    try {
      await api.post(`/api/admin/documents/${d.id}/relire`, {});
      info = 'Lecture demandée : ce document passe devant les autres. La fiche se met à jour toute seule.';
      await charger(d.id, false);
      suivre();
      onchange?.();
    } catch (e) {
      erreur = message(e);
    } finally {
      occupe = null;
    }
  }

  /** Bandeau d'un document pas encore lu : pourquoi il attend, depuis quand. */
  const GENRES_BLOQUANTS = ['jeton_absent', 'claude_refuse', 'quota', 'lecture_coupee', 'bloque', 'file_arretee'];
  let attenteBloquee = $derived(!!d?.attente && GENRES_BLOQUANTS.includes(d.attente.genre));
  let depuisAttente = $derived.by(() => {
    const q = quandCourt(d?.attente?.depuis);
    if (!q) return '';
    return /^\d\d:\d\d$/.test(q) || q === 'hier' ? `En attente depuis ${q}.` : `En attente depuis le ${q}.`;
  });
  /** Lisible tout de suite : en attente, Claude ni absent ni en refus, file de lecture en marche. */
  let lisibleMaintenant = $derived(
    admin &&
      !!d &&
      d.etat === 'a_lire' &&
      !['jeton_absent', 'claude_refuse', 'en_cours', 'file_arretee'].includes(d.attente?.genre ?? '')
  );

  let dlgEcarter = $state(false);
  let dlgRejeter = $state(false);
  let dlgForcer = $state(false);
  let dlgRelire = $state(false);
  let dlgRelireDetail = $state(false);
  /** Relisible par un admin : lu ou illisible, ni en lecture ni en envoi. */
  let relisible = $derived(admin && !!d && d.etat !== 'a_lire' && d.etat !== 'en_cours' && d.etat !== 'ignore' && d.statut_depot !== 'en_envoi');
  /** Facture ou avoir lus sans total : la lecture a manqué le pied de page. */
  let montantsManquants = $derived(
    !!d && d.etat === 'lu' && (d.type === 'facture' || d.type === 'avoir') && d.montant_ttc === null && d.statut_depot !== 'depose'
  );

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
        : { principal: { libelle: `Retour à ${retourLibelle}`, href: retour }, secondaires: [] };
    }
    if (d.statut_depot === 'en_envoi') return { principal: { libelle: libelleStatutEnvoi('en_envoi'), enCours: true }, secondaires: [] };
    if (d.statut_depot === 'depose' || envoiDemande(d) || d.envoi_relance) return { principal: { libelle: `Retour à ${retourLibelle}`, href: retour }, secondaires: [] };
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
        libelle: choix.type === 'bl' ? 'Valider le BL' : libelleValider((choix.type || null) as TypeDocument | null, outil),
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
    const s = d.statut_depot;
    const etat =
      s === 'depose' && dernierEnvoi
        ? `envoyé${fem} le ${jourHeure(dernierEnvoi.quand)}`
        : s === 'erreur' || s === 'en_envoi' || s === 'doublon'
          ? libelleStatutEnvoi(s, outil).toLowerCase()
          : d.valide_le
            ? 'envoi demandé'
            : `pas encore envoyé${fem}`;
    return etat;
  });
  let corrections = $derived(d ? Object.entries(d.corrections ?? {}) : []);
  const NOMS_CHAMPS: Record<string, string> = {
    type: 'type',
    societe_id: 'société',
    fournisseur_id: 'fournisseur',
    numero: 'n°',
    date_document: 'date',
    numero_commande: 'n° de commande',
    date_echeance: 'échéance',
    montant_ht: 'HT',
    montant_tva: 'TVA',
    montant_ttc: 'TTC'
  };
  const brut = (v: unknown) => (v === null || v === undefined || v === '' ? 'vide' : String(v));
  const quantite = (v: number | null) => (v === null ? '—' : v.toLocaleString('fr-FR'));
  let estBl = $derived(choix.type === 'bl');

  /** Type et N° : dans le sous-titre ; en champ s'ils sont douteux, vides, corrigés, ou demandés. */
  let voirTN = $state(false);
  let montrerType = $derived(voirTN || !!aControler('type') || change('type') || !choix.type);
  let montrerNumero = $derived(voirTN || !!aControler('numero') || change('numero') || !choix.numero);
  let nomFiche = $derived(choix.fournisseurNom || d?.fournisseur_nom || 'Fournisseur non reconnu');
  let feminin = $derived(choix.type === 'facture' || choix.type === 'relance');
  /** « 24/11 à 13:26 » */
  const jourAHeure = (iso: string | null | undefined) => jourHeure(iso).replace(' ', ' à ');
  /** « de LIDIS », « d’EDITIONS DU TONNERRE » */
  const de = (nom: string) => (/^du /.test(nom) ? nom : /^[aeiouyhàâäéèêëîïôöûüù]/i.test(nom) ? `d’${nom}` : `de ${nom}`);
  /** Facture ou avoir à envoyer, société sans adresse Dext / Yooz (quand le serveur ne l'a pas déjà signalé). */
  let sansAdresse = $derived(
    !!d &&
      d.etat === 'lu' &&
      deposable &&
      d.societe_id !== null &&
      !d.societe_adresse_depot &&
      !d.valide_le &&
      !d.statut_depot?.match(/^(depose|en_envoi|erreur|doublon)$/) &&
      !alertes.some((c) => c.code === 'depot')
  );
  /** BL lu, sans facture liée : il attend la sienne. */
  let blEnAttente = $derived(!!d && d.etat === 'lu' && estBl && !!rap && !rap.ignore && rap.liens.length === 0);
  let resumeLecture = $derived.by(() => {
    if (!d) return '';
    const n = rap?.lignes.length ?? 0;
    if (n) return `${n} ligne${n > 1 ? 's' : ''} lue${n > 1 ? 's' : ''}${d.modele ? ' par Claude' : ''}`;
    return d.lu_le ? `lu le ${jourHeure(d.lu_le)}` : '';
  });
  let exportPhenix = $state<EtatExportPhenix | null>(null);

  // ---------- Chaîne ----------
  let ficheEl = $state<HTMLElement>();
  /** Chaîne pour une facture, un avoir ou un BL lus (d.type : la structure suit la pièce enregistrée). */
  let avecChaine = $derived(!!d && d.etat === 'lu' && (d.type === 'facture' || d.type === 'avoir' || d.type === 'bl'));
  /** BL lu avec ses lignes : tableau des articles sous la chaîne (plus dans le repli Lecture). */
  let articlesBl = $derived(avecChaine && d?.type === 'bl' && !!rap?.lignes.length);
  let avecEnvoi = $derived(!!d && (!!d.statut_depot || deposable));
  let avecLogiflow = $derived(
    !!d &&
      ((d.type === 'facture' && d.statut_depot === 'depose' && (d.logiflow_configure || !!d.logiflow)) ||
        (estBl && (d.logiflow_configure || !!d.logiflow_livraison)))
  );
  let resumeLogiflow = $derived.by(() => {
    if (!d) return '';
    if (d.type === 'facture') return d.logiflow ? d.logiflow.message : d.logiflow_attendu || attenteLogiflow ? 'recherche en cours' : 'pas encore reporté';
    const ll = d.logiflow_livraison;
    return !ll ? 'pas encore' : ll.etat === 'ambigu' ? `${ll.possibles} livraisons possibles` : ll.facture ? `livraison n° ${ll.livraison_id}, facture ${ll.facture}` : `livraison n° ${ll.livraison_id}, sans facture`;
  });
  let pointEnvoi = $derived<PointChaine>(d?.statut_depot === 'depose' ? 'ok' : d?.statut_depot === 'erreur' ? 'alerte' : 'creux');
  let pointLogiflow = $derived.by((): PointChaine => {
    if (!d) return 'creux';
    if (d.type === 'facture') return d.logiflow ? (d.logiflow.ok ? 'ok' : 'alerte') : 'creux';
    const ll = d.logiflow_livraison;
    return !ll ? 'creux' : ll.etat === 'ambigu' ? 'alerte' : 'ok';
  });

  /** « Voir » du bandeau d'écart d'articles : sur un BL, déplie ses factures qui en ont ; amène la vue dessus. */
  async function voirEcarts() {
    if (!d || !rap) return;
    let cible = 'courant';
    if (d.type === 'bl') {
      const cles = rap.liens.filter((l) => l.ecarts.length).map((l) => `facture-${l.autre.id}`);
      for (const k of cles) ouverts.add(k);
      cible = cles[0] ?? cible;
    }
    await tick();
    ficheEl?.querySelector(`[data-maillon="${cible}"]`)?.scrollIntoView({ block: 'nearest' });
  }
  function voirPhenix() {
    document.getElementById(`${uid}-phenix`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }
</script>

{#snippet montantLu(s: string)}
  {#if s === ''}
    <span class="vide-val">Non lu</span>
  {:else if Number.isNaN(lireMontant(s))}
    <span class="num">{s}</span><span class="indice">montant illisible</span>
  {:else}
    <Montant valeur={lireMontant(s)} devise="" />
  {/if}
{/snippet}

{#snippet proposer(k: 'ht' | 'tva' | 'ttc')}
  {@const v = propositions[k]}
  {#if v !== undefined && editable}
    <button type="button" class="btn sm doc-proposer" onclick={() => (choix[k] = ecrireMontant(v))}>Mettre {formatMontant(v)}</button>
  {/if}
{/snippet}

{#snippet champsMontants()}
  {#if d}
    <div class="lignes-champs doc-montants">
      {#if estBl && !lu.ht && !lu.tva && !lu.ttc}
        <Champ disposition="ligne" libelle="Total HT" bind:valeur={choix.ht} mono inputmode="decimal" modifiable={editable}>
          {#snippet affichage()}
            {#if choix.ht === ''}<span class="vide-val">Non lu sur le BL</span>{:else}{@render montantLu(choix.ht)}{/if}
          {/snippet}
        </Champ>
      {:else}
        <Champ disposition="ligne" libelle="Total HT" bind:valeur={choix.ht} mono inputmode="decimal" modifiable={editable} aControler={aControler('ht')}>
          {#snippet affichage()}{@render montantLu(choix.ht)}{/snippet}
          {#snippet apres()}{@render proposer('ht')}{/snippet}
        </Champ>
        <Champ disposition="ligne" libelle="TVA" bind:valeur={choix.tva} mono inputmode="decimal" modifiable={editable} aControler={aControler('tva')} indice={tauxTva(ht, tva) || undefined}>
          {#snippet affichage()}{@render montantLu(choix.tva)}{/snippet}
          {#snippet apres()}{@render proposer('tva')}{/snippet}
        </Champ>
        <Champ disposition="ligne" libelle="TTC" bind:valeur={choix.ttc} mono inputmode="decimal" modifiable={editable} aControler={aControler('ttc')}>
          {#snippet affichage()}{@render montantLu(choix.ttc)}{/snippet}
          {#snippet apres()}{@render proposer('ttc')}{/snippet}
        </Champ>
      {/if}
    </div>
    {#if d.confiance?.montants_calcules?.length}
      <p class="annexes-montants">
        {d.confiance.montants_calcules.map((c) => ({ ht: 'HT', tva: 'TVA', ttc: 'TTC' })[c]).join(', ')} non imprimés sur le BL : calculés à partir des lignes (quantité × prix,
        éco-participations comprises, TVA 20 % sauf taux indiqué).
      </p>
    {/if}
  {/if}
{/snippet}

<!-- Contenu du maillon Dext/Yooz (ou du repli Envoi d'une pièce sans chaîne) -->
{#snippet contenuEnvoi()}
  {#if d}
    <div class="doc-envoi">
      {#if d.societe_adresse_depot}
        <p>Destination : <span class="num">{d.societe_adresse_depot}</span>{d.societe ? `, ${d.societe}` : ''}</p>
      {:else}
        <p class="t2">{d.societe ?? 'La société'} n’a pas d’adresse {nomOutil(outil)}.{#if admin} <a href="/reglages#societes">Réglages</a>{/if}</p>
      {/if}
      {#if d.statut_depot}
        <p><Etiquette ton={tonStatutEnvoi(d.statut_depot)}>{libelleStatutEnvoi(d.statut_depot, outil)}</Etiquette>
          {#if d.valide_le}<span class="t2 petit">validée le {jourHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}</span>{/if}
        </p>
      {/if}
    </div>
  {#if d.envois.length}
      <div class="defile-x"><table>
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
      </table></div>
    {:else}
      <p class="t2 petit">Aucun envoi pour l’instant.</p>
  {/if}
  {/if}
{/snippet}

<!-- Contenu du maillon Logiflow : report de la facture, ou livraison du BL -->
{#snippet contenuLogiflow()}
  {#if d && d.type === 'facture'}
    {@const lf = d.logiflow}
    <div class="doc-envoi">
      {#if lf}
        <p>
          <Etiquette ton={lf.ok ? 'ok' : 'alerte'}>{lf.ok ? 'Reportée' : 'À regarder'}</Etiquette>
          <span class="t2 petit">le {jourHeure(lf.quand)} : {lf.message}</span>
        </p>
      {:else if d.logiflow_attendu || attenteLogiflow}
        <p class="t2 petit">Recherche des livraisons dans Logiflow…</p>
      {:else}
        <p class="t2 petit">Cette facture n’a pas encore été reportée dans Logiflow.</p>
      {/if}
    </div>
    {#if lf?.livraisons.length}
      <div class="defile-x"><table>
        <thead><tr><th>BL</th><th>Livraison</th><th>Résultat</th></tr></thead>
        <tbody>
          {#each lf.livraisons as l, i (i)}
            <tr>
              <td class="num" style="white-space: nowrap">{l.bl}</td>
              <td class="t2">{#if l.livraison_id}n° <span class="num">{l.livraison_id}</span>{l.magasin ? `, ${l.magasin}` : ''}{:else}—{/if}</td>
              <td>
                <Etiquette ton={LIBELLES_LOGIFLOW[l.etat]?.ton ?? 'neutre'}>{LIBELLES_LOGIFLOW[l.etat]?.texte ?? l.etat}</Etiquette>
                <div class="t2 petit">{l.message}</div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table></div>
  {/if}
  {#if d.logiflow_configure && !(d.logiflow_attendu || (attenteLogiflow && !lf))}
      <p><button type="button" class="btn sm" onclick={relancerLogiflow}><Icone nom="rafraichir" />Relancer la recherche</button></p>
  {/if}
  {:else if d}
    {@const ll = d.logiflow_livraison}
    <div class="doc-envoi">
      {#if !ll}
        <p class="t2 petit">Aucune livraison livrée avec ce n° de BL dans Logiflow pour l’instant. GMFlow vérifie toutes les heures.</p>
      {:else if ll.etat === 'ambigu'}
        <p>
          <Etiquette ton="alerte">À rapprocher dans Logiflow</Etiquette>
          <span class="t2 petit">{ll.possibles} livraisons portent ce n° de BL : GMFlow n’en choisit aucune.</span>
        </p>
      {:else}
        <p>
          <Etiquette ton={ll.rapprochee ? 'ok' : ll.facture ? 'ok' : 'neutre'}>{ll.rapprochee ? 'Rapprochée' : ll.facture ? 'Facture saisie' : 'Sans facture'}</Etiquette>
          <span class="t2 petit">vu le {jourHeure(ll.vu_le)}</span>
        </p>
        <p><span class="t2">Livraison</span> n° <span class="num">{ll.livraison_id}</span>{ll.magasin ? `, ${ll.magasin}` : ''}{ll.livree_le ? `, livrée le ${dateFr(ll.livree_le)}` : ''}</p>
        {#if ll.fournisseur}<p><span class="t2">Fournisseur</span> {ll.fournisseur}</p>{/if}
        {#if ll.montant_bl !== null}<p><span class="t2">Montant du BL</span> <Montant valeur={ll.montant_bl} /></p>{/if}
        {#if ll.facture}
          <p>
            <span class="t2">Facture</span> <span class="num">{ll.facture}</span>{#if ll.facture_ht !== null}, HT <Montant valeur={ll.facture_ht} />{/if}{#if ll.facture_ttc !== null}, TTC <Montant valeur={ll.facture_ttc} />{/if}{#if ll.echeance}, échéance <span class="num">{dateFr(ll.echeance)}</span>{/if}
          </p>
        {:else}
          <p class="t2 petit">La facture sera écrite dans Logiflow quand une facture liée à ce BL sera envoyée à Dext/Yooz.</p>
        {/if}
      {/if}
    </div>
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
  {#if codeBo?.code && !change('fournisseur') && codeBo.id === d?.fournisseur_id}
    <span class="doc-petit doc-ligne">Code BO Phénix {codeBo.code}</span>
  {/if}
{/snippet}

<div class="volet-b doc-corps">
  <section class="apercu-zone doc-apercu" aria-label="Document" class:sr={erreurChargement !== ''}>
    <div class="apercu-barre">
      {#if pleinEcran}
        <a class="retour" href={retour}><Icone nom="gauche" />{retourLibelle} <Kbd t="Escape" /></a>
      {:else}
        <a class="retour tel-seul" href={retour}><Icone nom="gauche" />{retourLibelle}</a>
      {/if}
      {#if position}
        <span class="pos large-seul">{position.section} <span class="num">{position.n}</span> sur <span class="num">{position.total}</span></span>
      {/if}
      {#if pages}<span class="num large-seul">{pages === 1 ? 'Page 1 / 1' : `${pages} pages`}</span>{/if}
      <div class="outils">
        {#if onprecedent}
          <button type="button" class="outil carre" class:tel-seul={!pleinEcran} aria-label="Précédent (K)" title="Précédent (K)" onclick={onprecedent}><Icone nom="haut" /></button>
        {/if}
        {#if onsuivant}
          <button type="button" class="outil carre" class:tel-seul={!pleinEcran} aria-label="Suivant (J)" title="Suivant (J)" onclick={onsuivant}><Icone nom="bas" /></button>
        {/if}
        {#if zoomable}
          <button type="button" class="outil carre large-seul" aria-label="Dézoomer" disabled={zoom <= ZOOMS[0]} onclick={() => zoomer(-1)}>−</button>
          <button type="button" class="outil carre large-seul" aria-label="Zoomer" disabled={zoom >= ZOOMS[ZOOMS.length - 1]} onclick={() => zoomer(1)}>+</button>
        {/if}
        {#if pleinEcran && d}
          {#if /^(application\/pdf|image\/)/.test(d.type_mime)}
            <a class="outil large-seul" href={urlApercu(d.piece_id)} target="_blank" rel="noopener"><Icone nom="ouvrir" />Ouvrir</a>
          {/if}
          <a class="outil" href="/api/pieces/{d.piece_id}" download={d.nom_fichier}><Icone nom="telecharger" />{d.type_mime === 'application/pdf' ? 'PDF' : 'Fichier'}</a>
        {:else if !pleinEcran}
          <a class="outil large-seul" href="/documents/{id}">Plein écran <Kbd t="F" /></a>
        {/if}
      </div>
    </div>
    {#if d}<ApercuPiece id={d.piece_id} typeMime={d.type_mime} nom={d.nom_fichier} outils={false} carte {zoom} bind:pages bind:zoomable />{/if}
  </section>

  <Poignee bind:largeur={largFiche} bind:choisie={ficheChoisie} defaut={defautFiche} min={300} max={900} cote="droite" cle={cleFiche} libelle="Largeur de la fiche" fiche />

  <section bind:this={ficheEl} class="doc-fiche" class:choisie={ficheChoisie} style:--larg-choisie="{largFiche}px" aria-label={estBl ? 'Détails du BL' : 'Détails de la pièce'}>
    <div class="contenu-fiche avec-barre">
      {#if annonce}<Bandeau ton="ok" role="status">{annonce}</Bandeau>{/if}
      {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
      {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}

      {#if erreurChargement}
        <Bandeau ton="erreur">
          {erreurChargement}
          {#snippet action()}
            {#if introuvable}<a class="btn sm" href={retour}>Retour à {retourLibelle}</a>{:else}<button type="button" class="btn sm" onclick={reessayer}>Réessayer</button>{/if}
          {/snippet}
        </Bandeau>
      {:else if !d}
        <Chargement texte="Chargement du document" />
      {:else}
        <div class="doc-tete">
          <h1 class="doc-nom">{nomFiche}</h1>
          <p class="doc-sous">
            {typeNumero((choix.type || null) as TypeDocument | null, choix.numero)}, reçu{feminin ? 'e' : ''} le {jourAHeure(d.recu_le)}{#if d.valide_le}, validation le {jourAHeure(d.valide_le)}{d.valide_par ? ` par ${d.valide_par}` : ''}{:else if d.verifie_le}, validation le {jourAHeure(d.verifie_le)}{d.verifie_par ? ` par ${d.verifie_par}` : ''}{/if}
            {#if d.copie_de}<span class="petit">· même fichier que le <a href="/documents/{d.copie_de}">document {d.copie_de}</a></span>{/if}
          </p>
        </div>

        <!-- Une phrase en tête : refus, état, ou première alerte -->
        {#if refus}
          {#if refus.code === 'doublon'}
            <Bandeau ton="erreur" titre="Doublon.">{phrase(refus.message)}</Bandeau>
          {:else if refus.code === 'expedition'}
            <Bandeau ton="erreur" titre="Envoi impossible.">
              L’adresse d’envoi vers {nomOutil(outil)} n’est pas encore configurée.
              {#snippet action()}{#if admin}<a class="btn sm" href="/reglages#envoi">Ouvrir les réglages</a>{/if}{/snippet}
            </Bandeau>
          {:else if refus.code === 'sans_adresse'}
            <Bandeau ton="erreur" titre="Envoi impossible.">
              {d.societe ?? 'La société'} n’a pas d’adresse {nomOutil(outil)}.{admin ? '' : ' Un administrateur doit la renseigner.'}
              {#snippet action()}{#if admin}<a class="btn sm" href="/reglages#societes">Ouvrir les réglages</a>{/if}{/snippet}
            </Bandeau>
          {:else}
            <Bandeau ton="erreur">{phrase(refus.message)}</Bandeau>
          {/if}
        {/if}
        {#if d.etat === 'a_lire' || d.etat === 'en_cours'}
          <Bandeau ton={attenteBloquee ? 'attention' : 'info'} titre="{libelleEtat(d.etat)}.">
            {#if d.attente}{phrase(d.attente.raison)}{/if}
            {#if d.etat === 'a_lire'}{depuisAttente}{/if}
            {#if d.attente?.dernier_essai}<span class="dernier-essai">Dernier essai : {phrase(d.attente.dernier_essai)}</span>{/if}
            <span class="dernier-essai">La fiche se met à jour toute seule.</span>
            {#snippet action()}
              {#if lisibleMaintenant}
                <button type="button" class="btn sm principal" onclick={lireMaintenant} disabled={occupe !== null}>
                  {occupe === 'relire' ? 'Demande…' : 'Lire maintenant'}
                </button>
              {/if}
            {/snippet}
          </Bandeau>
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
        {:else if autresAlertes.length && !refus}
          {@const a0 = autresAlertes[0]}
          <Bandeau titre={detailAlerte(a0) ? `${titreAlerte(a0)} :` : `${titreAlerte(a0)}.`}>
            {#if detailAlerte(a0)}{phrase(detailAlerte(a0))}{/if}{#if a0.code === 'depot' && admin}{' '}<a href="/reglages#societes">Indiquer l’adresse</a>{/if}{#if autresAlertes.length > 1}
              {' '}{autresAlertes.length - 1 === 1 ? 'Une autre alerte' : `${autresAlertes.length - 1} autres alertes`}, surlignée{autresAlertes.length > 2 ? 's' : ''} dans les champs.{/if}
          </Bandeau>
        {/if}
        {#if sansAdresse && !refus}
          <Bandeau titre="À vérifier :">
            {d.societe ?? 'La société'} n’a pas d’adresse d’envoi vers {outil ? nomOutil(outil) : 'Dext ou Yooz'}.{#if admin}{' '}<a href="/reglages#societes">Indiquer l’adresse</a>{:else}{' '}Un administrateur doit l’indiquer.{/if}
          </Bandeau>
        {/if}
        {#if alerteSociete && !refus && d.etat === 'lu' && !envoiDemande(d) && d.statut_depot !== 'depose' && d.statut_depot !== 'en_envoi'}
          <Bandeau titre="{titreAlerte(alerteSociete)}.">
            {d.societe_id === null
              ? 'Aucune de vos sociétés n’est trouvée sur le document.'
              : `${d.societe ?? 'La société'} est trouvée avec peu de certitude.`}
            Choisissez la bonne société, ou rejetez la pièce si elle n’est pas pour vous.
            <span class="choix-societe">
              {#each societe.liste as s (s.id)}
                <button
                  type="button"
                  class="btn sm"
                  class:principal={choix.societe === String(s.id)}
                  aria-pressed={choix.societe === String(s.id)}
                  disabled={!editable}
                  onclick={() => (choix.societe = String(s.id))}>{nomCourtSociete(s.raison_sociale)}</button
                >
              {/each}
              <button type="button" class="btn sm danger" disabled={!editable} onclick={() => (dlgRejeter = true)}>Rejeter</button>
            </span>
          </Bandeau>
        {:else if change('societe') && lu.societe === '' && choix.societe !== '' && d.etat === 'lu' && !fige}
          <Bandeau ton="info">Attribué à {nomSocieteChoisie}. Validez pour l’enregistrer.</Bandeau>
        {/if}
        {#if articlesEcart.length && d.etat === 'lu'}
          {@const p = articlesEcart[0]}
          <Bandeau titre="{articlesEcart.length > 1 ? `${articlesEcart.length} articles en écart avec ${d.type === 'bl' ? 'la facture' : 'le BL'}` : `${p.titre}`}.">
            {p.article} : {p.detail}{p.montant !== null && Math.abs(p.montant) > 0.005 ? ` (${p.montant > 0 ? '+' : ''}${formatMontant(p.montant)} HT)` : ''}.{articlesEcart.length > 1 ? ' Détail dans la chaîne.' : ''}
            {#snippet action()}<button type="button" class="btn sm" onclick={voirEcarts}>Voir</button>{/snippet}
          </Bandeau>
        {/if}
        {#if d.etat === 'lu' && d.message && !envoiDemande(d) && d.statut_depot !== 'depose' && !d.verifie_le}
          <Bandeau titre="Lecture incomplète.">
            {phrase(d.message.replace(/^Lecture à vérifier : /, ''))} Vérifiez sur l’aperçu{relisible ? ', ou faites-le relire en détail' : ''}.
            {#snippet action()}{#if relisible}<button type="button" class="btn sm principal" onclick={() => (dlgRelireDetail = true)}>Relire en détail</button>{/if}{/snippet}
          </Bandeau>
        {:else if montantsManquants && relisible}
          <Bandeau titre="Montants non lus.">
            Les totaux sont sur le document mais la lecture les a manqués. La relecture en détail lit le PDF entier avec un modèle plus fort.
            {#snippet action()}<button type="button" class="btn sm principal" onclick={() => (dlgRelireDetail = true)}>Relire en détail</button>{/snippet}
          </Bandeau>
        {/if}
        {#if blEnAttente}
          <Bandeau ton="info" titre="En attente de facture.">
            Il sera rapproché dès que la facture {de(choix.fournisseurNom || d.fournisseur_nom || 'du fournisseur')} arrivera.
          </Bandeau>
        {/if}


        {#if avecChaine}
          <DocChaine
            {d}
            {rap}
            {factures}
            bind:ouverts
            onchange={apresLien}
            onerreur={(m) => (erreur = m)}
            lu={{ ht: d.montant_ht, corrige: change('ht') }}
            resumes={{ phenix: exportPhenix ? resumeExport(exportPhenix) : '', envoi: resumeEnvoi, logiflow: resumeLogiflow }}
            points={{ envoi: pointEnvoi, logiflow: pointLogiflow }}
            onphenix={voirPhenix}
            montantsPiece={champsMontants}
            envoi={avecEnvoi ? contenuEnvoi : undefined}
            logiflow={avecLogiflow ? contenuLogiflow : undefined}
          />
        {:else if d.etat === 'lu'}
          <div class="carte-fiche">
            <h3 class="titre-carte">Montants</h3>
            {@render champsMontants()}
          </div>
        {/if}
        {#if articlesBl && rap}
          <DocArticlesBl
            lignes={rap.lignes}
            ecarts={rap.liens.flatMap((l) => l.ecarts)}
            factureLiee={rap.liens.length > 0}
            plusieursBl={rap.liens.some((l) => (factures.get(l.autre.id)?.rap?.liens.length ?? 0) >= 2)}
          />
        {/if}

        {#if d.etat === 'lu'}
          {@const ctrlF = aControler('fournisseur')}
          <div class="carte-fiche">
          <h3 class="titre-carte">Détails de la pièce</h3>
          <div class="lignes-champs">
            {#if montrerType}
              <Champ
                disposition="ligne"
                libelle="Type"
                type="select"
                bind:valeur={choix.type}
                options={optionsType}
                modifiable={editable}
                aControler={aControler('type')}
              />
            {/if}
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
            <Champ
              disposition="ligne"
              libelle="Société"
              type="select"
              bind:valeur={choix.societe}
              options={optionsSociete}
              modifiable={editable}
              aControler={aControler('societe')}
            />
            {#if montrerNumero}
              <Champ disposition="ligne" libelle="N°" bind:valeur={choix.numero} mono modifiable={editable} aControler={aControler('numero')} />
            {/if}
            <Champ disposition="ligne" libelle="Date" type="date" bind:valeur={choix.date} mono modifiable={editable} aControler={aControler('date')}>
              {#snippet affichage()}
                {#if choix.date}<span class="num">{dateFr(choix.date)}</span>{:else}<span class="vide-val">Non lue</span>{/if}
              {/snippet}
            </Champ>
            {#if choix.commande || lu.commande}
              <Champ disposition="ligne" libelle="Commande" bind:valeur={choix.commande} mono modifiable={editable} />
            {/if}
            {#if choix.type === 'facture'}
              <Champ disposition="ligne" libelle="Échéance" type="date" bind:valeur={choix.echeance} mono modifiable={editable}>
                {#snippet affichage()}
                  {#if choix.echeance}<span class="num">{dateFr(choix.echeance)}</span>{:else}<span class="vide-val">Non lue</span>{/if}
                {/snippet}
              </Champ>
            {/if}
          </div>
          </div>

          {#if proposerRangement}
            <Bandeau ton="info">
              Toujours ranger {choix.fournisseurNom} dans {nomSocieteChoisie} ?
              {#snippet action()}
                <button type="button" class="btn sm" class:en-cours={occupe === 'fournisseur'} disabled={occupe !== null} onclick={() => majFournisseur(Number(choix.fournisseur), { societe_defaut_id: Number(choix.societe) }, `${choix.fournisseurNom} sera rangé dans ${nomSocieteChoisie}`)}>Oui</button>
              {/snippet}
            </Bandeau>
          {/if}
          {#if modifie && !fige}
            <p class="petit t2 doc-note">Corrections gardées à la validation.
              <button type="button" class="btn-lien" onclick={() => (choix = { ...lu })}>Revenir à la lecture</button>
            </p>
          {/if}
        {/if}

        {#if d.etat === 'lu' && d.type === 'bl'}
          <div class="carte-fiche" id="{uid}-phenix">
            <h3 class="titre-carte">Export Phénix {#if exportPhenix}<span class="etiquette-carte">{resumeExport(exportPhenix)}</span>{/if}</h3>
            <DocExportPhenix id={d.id} bind:etat={exportPhenix} onerreur={(m) => (erreur = m)} />
          </div>
        {/if}

        <div class="carte-fiche">
          <h3 class="titre-carte">Origine</h3>
          <div class="replis doc-liens">
            {#if !avecChaine && avecEnvoi}
              <Repli icone="envoyer" titre={titreEnvoi(outil)} resume={resumeEnvoi}>{@render contenuEnvoi()}</Repli>
            {/if}
            {#if !avecChaine && avecLogiflow}
              <Repli icone="camion" titre="Logiflow" resume={resumeLogiflow}>{@render contenuLogiflow()}</Repli>
            {/if}
            <Repli icone="mail" titre="Mail" resume="{nomExpediteur(d.mail_expediteur) || 'expéditeur inconnu'}, {jourHeure(d.recu_le)}">
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
            {#if d.etat === 'lu' && ((rap?.lignes.length && !articlesBl) || d.texte || corrections.length || admin)}
              <Repli icone="fichier" titre="Lecture" resume={resumeLecture}>
                {#if admin && d.voie}
                  <p class="t2 petit">{libelleVoie(d.voie)}{d.modele ? ` (${d.modele}${d.effort ? `, ${d.effort}` : ''})` : ''}{d.lu_le ? `, le ${jourHeure(d.lu_le)}` : ''}</p>
                {/if}
                {#if corrections.length}
                  <p class="t2 petit">Corrigé à la main : {#each corrections as [champ, c], i (champ)}{i ? ' ; ' : ''}{NOMS_CHAMPS[champ] ?? champ} ({brut(c.avant)} → {brut(c.apres)}){/each}</p>
                {/if}
                {#if rap?.lignes.length && !articlesBl}
                  <div class="defile-x"><table>
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
                  </table></div>
                {/if}
                {#if d.texte}
                  <details class="doc-texte">
                    <summary class="t2 petit">Texte lu</summary>
                    <pre>{d.texte}</pre>
                  </details>
                {/if}
              </Repli>
            {/if}
            {#if estBl && d.fournisseur_id}
              <a class="ligne-lien" href="/fournisseurs?fiche={d.fournisseur_id}">
                <Icone nom="batiment" />
                <span class="titre-repli">Fournisseur</span>
                <span class="resume-repli">{d.fournisseur ?? choix.fournisseurNom}</span>
                <Icone nom="droite" class="chev" />
              </a>
            {/if}
          </div>
        </div>

        {#if relisible || (d.etat === 'lu' && !voirTN && editable && (!montrerType || !montrerNumero))}
          <div class="doc-actions">
            {#if relisible}
              <button type="button" class="btn-lien" onclick={() => (dlgRelire = true)}>Relire avec Claude</button>
              <button type="button" class="btn-lien" onclick={() => (dlgRelireDetail = true)}>Relire en détail</button>
            {/if}
            {#if d.etat === 'lu' && !voirTN && editable && (!montrerType || !montrerNumero)}
              <button type="button" class="btn-lien" onclick={() => (voirTN = true)}>Corriger le type ou le n°</button>
            {/if}
          </div>
        {/if}
      {/if}
    </div>

    {#if !erreurChargement}
      <BarreAction
        principal={plan.principal}
        secondaires={plan.secondaires}
        {ensuite}
        {sansSuite}
        toucheEnsuite={pleinEcran && ensuite && onsuivant ? 'J' : undefined}
      />
    {/if}
  </section>
</div>

<Confirmation bind:ouvert={dlgEcarter} titre="Écarter ce document ?" libelle="Écarter" danger onconfirmer={ecarter}>
  <p>{titreDoc} sera marqué « {libelleEtat('ignore')} » : {deposable ? `pas d’envoi à ${nomOutil(outil)}, ` : ''}il quitte la file. Il reste visible dans la recherche.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgRejeter} titre="Rejeter ce document ?" libelle="Rejeter" danger onconfirmer={ecarter}>
  <p>{titreDoc} n’est pour aucune de vos sociétés : {deposable ? `pas d’envoi à ${nomOutil(outil)}, ` : ''}il quitte la file. Il reste visible dans la recherche.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgForcer} titre="Envoyer ce doublon quand même ?" libelle="Envoyer quand même" danger onconfirmer={() => valider(true)}>
  <p>{refus?.code === 'doublon' ? phrase(refus.message) : doublon ? phrase(doublon.message) : ''} La facture partira à {nomOutil(outil)} telle quelle.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgRelire} titre="Relire avec Claude ?" libelle="Relire" onconfirmer={() => relire()}>
  <p>La lecture et la validation actuelles seront remplacées par une nouvelle lecture.</p>
</Confirmation>
<Confirmation bind:ouvert={dlgRelireDetail} titre="Relire en détail ?" libelle="Relire en détail" onconfirmer={() => relire(true)}>
  <p>
    Claude relit le fichier d’origine page par page, totaux de la dernière page compris, avec le modèle le plus fort (plus lent, compte dans le quota
    du jour). La lecture et la validation actuelles seront remplacées.
  </p>
</Confirmation>

<style>
  /* trois colonnes avec la liste : aperçu au centre, fiche de 360 px à droite (pied d'action compris) */
  .doc-corps {
    overflow: hidden;
  }
  .doc-apercu {
    flex: 1;
    min-width: 0;
  }
  .doc-fiche {
    flex: none;
    width: 360px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--white);
  }
  :global(.pleine) .doc-fiche {
    width: 400px;
  }
  .doc-fiche.choisie {
    width: var(--larg-choisie);
    max-width: calc(100% - 380px); /* l'aperçu garde sa barre d'outils */
  }
  .doc-fiche :global(.contenu-fiche) {
    flex: 1;
    width: auto;
    border-left: 0;
    padding: var(--e6);
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    overflow: auto;
  }
  .doc-fiche :global(.bandeau) {
    margin: 0;
  }
  .doc-fiche :global(.barre-action) {
    padding: var(--e4) var(--e6);
  }
  .doc-tete {
    display: flex;
    flex-direction: column;
    gap: var(--e1);
  }
  .doc-nom {
    margin: 0;
    font-size: 22px;
    line-height: 1.2;
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .doc-sous {
    margin: 0;
    font-size: 15px;
    color: var(--text2);
  }
  .doc-petit {
    font-size: 14px;
    color: var(--text2);
  }
  .doc-ligne {
    flex-basis: 100%;
  }
  /* montants de la pièce dans la chaîne : valeurs alignées à droite, dans la colonne des montants */
  .doc-montants :global(.champ-ligne) {
    --larg-cle: 96px;
  }
  .doc-montants :global(.champ-ligne > .k),
  .doc-montants :global(.champ-ligne > .v) {
    min-height: 40px;
    padding-top: 8px;
    padding-bottom: 8px;
  }
  .doc-montants :global(.champ-ligne > .v),
  .doc-montants :global(.champ-ligne > .v .val) {
    justify-content: flex-end;
    text-align: right;
  }
  /* taux de TVA à gauche du montant : les chiffres restent dans la colonne */
  .doc-montants :global(.champ-ligne > .v .indice) {
    order: -1;
  }
  .doc-montants :global(.champ-ligne:last-child) {
    border-bottom: 0;
  }
  .doc-liens {
    margin-top: 0;
  }
  /* ligne-lien vers une autre page : même dessin qu'une ligne repliable */
  .ligne-lien {
    display: flex;
    align-items: center;
    gap: var(--e3);
    min-height: 48px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line-faint);
    color: var(--text);
    text-decoration: none;
    font-weight: 500;
  }
  .ligne-lien > :global(svg.i:first-child) {
    color: var(--text2);
  }
  .ligne-lien .titre-repli {
    flex: none;
    white-space: nowrap;
  }
  .ligne-lien .resume-repli {
    flex: 0 1 auto;
    min-width: 0;
    color: var(--text2);
    font-weight: 400;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ligne-lien :global(.chev) {
    flex: none;
    margin-left: auto;
    color: var(--muted);
  }
  .ligne-lien:hover .titre-repli {
    color: var(--accent);
  }
  .doc-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2) var(--e4);
    font-size: 15px;
  }
  .dernier-essai {
    display: block;
    margin-top: 2px;
    color: var(--text2);
  }
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
  .choix-societe {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
    margin-top: var(--e2);
  }
  .doc-note {
    margin: 0;
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
  /* < 1200 px : l'aperçu au-dessus de la fiche, le tout défile */
  @media (max-width: 1200px) {
    .doc-corps {
      flex-direction: column;
      overflow: auto;
    }
    .doc-apercu {
      flex: none;
      height: 420px;
      border-bottom: 1px solid var(--line);
    }
    .doc-fiche,
    .doc-fiche.choisie,
    :global(.pleine) .doc-fiche {
      max-width: none;
      flex: none;
      width: auto;
      min-height: auto;
      border-left: 0;
    }
    .doc-fiche :global(.contenu-fiche) {
      overflow: visible;
    }
  }
  @media (max-width: 760px) {
    .doc-corps {
      overflow: visible;
    }
    .doc-apercu {
      height: 300px;
    }
    .doc-fiche :global(.contenu-fiche) {
      padding: var(--e4);
    }
  }
</style>
