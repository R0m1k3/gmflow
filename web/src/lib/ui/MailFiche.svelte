<script lang="ts" module>
  // Fiche mail (maquette Sauge S2) : le mail et ses pièces au centre, la colonne « Réponse » à droite.
  // Utilisée par /mails/[id] ; peut aussi s'ouvrir dans le volet de droite de À traiter
  // (`ouvrir` dit où aller après un envoi ou un classement).

  const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
  const jourMois = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: '2-digit', month: '2-digit' });
  const jourMoisAn = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: '2-digit', month: '2-digit', year: '2-digit' });
  const cleJour = new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Paris' });

  /** « aujourd’hui 09:41 », « hier 17:02 », « 14/11 08:57 », « 14/11/25 » (heure de Paris). */
  export function dateMail(iso: string | null | undefined, avecHeure = true): string {
    if (!iso) return '';
    const d = new Date(iso);
    const k = cleJour.format(d);
    const h = avecHeure ? ` ${heure.format(d)}` : '';
    if (k === cleJour.format(new Date())) return avecHeure ? `aujourd’hui${h}` : heure.format(d);
    if (k === cleJour.format(new Date(Date.now() - 86_400_000))) return `hier${h}`;
    if (k.slice(0, 4) === cleJour.format(new Date()).slice(0, 4)) return `${jourMois.format(d)}${h}`;
    return jourMoisAn.format(d);
  }

  /** « Céline Marchal <c.marchal@arc-france.fr> » devient « Céline Marchal ». */
  export function nomExpediteur(e: string | null | undefined): string {
    if (!e) return '';
    const m = e.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>\s*$/);
    if (m) return m[1].trim() || m[2].trim();
    return e.trim();
  }

  /** Adresse seule : « Céline Marchal <c.marchal@arc-france.fr> » devient « c.marchal@arc-france.fr ». */
  export function adresseExpediteur(e: string | null | undefined): string {
    if (!e) return '';
    return (e.match(/<([^>]+)>/)?.[1] ?? e).trim();
  }

  const GENERIQUES = /^(no-?reply|ne-?pas-?repondre|contact|info|infos|compta|comptabilite|facturation|factures?|service|admin|commercial|ventes?|news|newsletter|bonjour|hello)$/i;
  const majuscules = (t: string) => t.replace(/(^|[\s-])(\p{L})/gu, (_, a: string, b: string) => a + b.toUpperCase());

  /** Nom lisible d'un expéditeur pour les listes : le nom affiché, sinon tiré de l'adresse
   *  (« betty.pecqueur@… » → « Betty Pecqueur », « contact@lidis.fr » → « Lidis »). */
  export function nomLisible(e: string | null | undefined): string {
    const nom = nomExpediteur(e);
    if (!nom.includes('@')) return nom;
    const [local, domaine = ''] = nom.toLowerCase().split('@');
    const mots = local.split(/[._]+/).filter((x) => /^\p{L}[\p{L}-]+$/u.test(x));
    if (mots.length >= 2 && !GENERIQUES.test(mots[0])) return majuscules(mots.join(' '));
    const d = domaine.split('.').slice(-2, -1)[0];
    return d ? majuscules(d.replace(/[-_]+/g, ' ')) : nom;
  }

  /** « aujourd’hui à 14:38 », « hier à 17:02 », « 14/11 à 08:57 », « 14/11/25 à 08:57 ». */
  export function dateLongue(iso: string | null | undefined): string {
    if (!iso) return '';
    const d = new Date(iso);
    const k = cleJour.format(d);
    const h = heure.format(d);
    if (k === cleJour.format(new Date())) return `aujourd’hui à ${h}`;
    if (k === cleJour.format(new Date(Date.now() - 86_400_000))) return `hier à ${h}`;
    if (k.slice(0, 4) === cleJour.format(new Date()).slice(0, 4)) return `${jourMois.format(d)} à ${h}`;
    return `${jourMoisAn.format(d)} à ${h}`;
  }

  /** Paragraphes du corps, et la partie citée (réponse précédente) à part. */
  function decouper(corps: string): { paragraphes: string[]; cite: string } {
    const lignes = corps.replace(/\r\n?/g, '\n').split('\n');
    const debutCite = lignes.findIndex(
      (l, i) =>
        i > 0 &&
        (/^\s*>/.test(l) ||
          /^\s*(Le .{4,200} a écrit\s?:|On .{4,200} wrote:)\s*$/.test(l) ||
          /^\s*-{2,}\s*(Message d.origine|Original Message|Message transféré|Forwarded message)/i.test(l) ||
          /^\s*_{8,}\s*$/.test(l))
    );
    const corpsPropre = (debutCite > 0 ? lignes.slice(0, debutCite) : lignes).join('\n').trim();
    const cite = debutCite > 0 ? lignes.slice(debutCite).join('\n').trim() : '';
    const paragraphes = corpsPropre ? corpsPropre.split(/\n\s*\n+/).map((p) => p.trim()).filter(Boolean) : [];
    return { paragraphes, cite };
  }

  /** Repère laissé par Claude quand une information manque (voir crates/gmflow-worker/src/redaction.rs). */
  const A_COMPLETER = /\[à compléter\]/gi;

  /** Message affiché une fois sur la fiche suivante (« Réponse envoyée… »). */
  const annonce = $state({ texte: '' });
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { beforeNavigate, goto } from '$app/navigation';
  import { aTraiter, api, ErreurApi, filFournisseur, message, sonder } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { taille } from '#lib/format.ts';
  import {
    ACTIONS_REPONSE,
    libelleBrouillon,
    libelleCategorie,
    libelleEnvoyerDepuis,
    libelleEtat,
    libellePriorite,
    libelleStatutEnvoi,
    libelleStatutReponse,
    libelleTriePar,
    libelleType,
    nomCourtSociete,
    nomOutil,
    tonStatutReponse
  } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import ApercuPiece from './ApercuPiece.svelte';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Brouillon, Categorie, EtatFournisseur, FicheMail, MailARepondre } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Chargement from './Chargement.svelte';
  import Confirmation from './Confirmation.svelte';
  import Etiquette from './Etiquette.svelte';
  import Icone from './Icone.svelte';
  import Kbd from './Kbd.svelte';
  import { formatMontant } from './Montant.svelte';
  import Repli from './Repli.svelte';

  let {
    id,
    retour = { href: '/a-traiter', libelle: 'À traiter' },
    ouvrir,
    volet = false
  }: {
    id: number;
    /** Lien « retour » de l'en-tête (Échap). */
    retour?: { href: string; libelle: string };
    /** Ouvre le mail suivant de la file À répondre, ou `null` quand elle est vide. Par défaut : /mails/ID, sinon /. */
    ouvrir?: (suivant: number | null) => void | Promise<void>;
    /** Dans le volet de droite de À traiter : la page garde J, K et Échap ; retour sur téléphone seulement. */
    volet?: boolean;
  } = $props();

  const aller = (suivant: number | null) =>
    ouvrir ? ouvrir(suivant) : goto(suivant ? `/mails/${suivant}` : '/a-repondre');

  const ACTIFS = ['generation', 'propose', 'modifie', 'envoi'];
  const EN_COURS = ['generation', 'envoi'];
  const CATEGORIES: Categorie[] = ['question', 'information', 'document_seul', 'publicite', 'notification'];

  let m = $state<FicheMail | null>(null);
  let erreurChargement = $state('');
  let erreur = $state('');
  let info = $state('');
  let occupe = $state<string | null>(null);
  let texte = $state('');
  let boiteEnvoi = $state('');
  /** Le mail était dans la file À répondre à l'ouverture : après envoi ou classement, on passe au suivant. */
  let etaitDansFile = $state(false);
  /** Brouillon dont on attend l'envoi par le worker. */
  let attenteEnvoi = $state<number | null>(null);
  let file = $state<MailARepondre[] | null>(null);
  let contexte = $state<{ fournisseurId: number; nom: string; societe: string | null; etat: EtatFournisseur } | null>(null);
  let maintenant = $state(Date.now());
  let citeOuverte = $state(false);
  let arret: (() => void) | null = null;
  let tour = 0;

  let admin = $derived(session.moi?.role === 'admin');
  let actif = $derived<Brouillon | undefined>(m?.brouillons.find((b) => ACTIFS.includes(b.statut)));
  let envoyes = $derived(m?.brouillons.filter((b) => b.statut === 'envoye') ?? []);
  let editable = $derived(!!actif && (actif.statut === 'propose' || actif.statut === 'modifie'));
  let modifieIci = $derived(editable && texte !== actif!.texte);
  let aCompleter = $derived(editable ? (texte.match(A_COMPLETER)?.length ?? 0) : 0);
  let peutEnvoyer = $derived(editable && !!actif?.smtp && texte.trim().length > 0 && !occupe);
  let corps = $derived(decouper(m?.corps_texte ?? ''));
  let indiceFile = $derived(file ? file.findIndex((x) => x.id === id) : -1);
  let aRepondre = $derived(m?.statut_reponse === 'a_repondre');
  let depuisSecondes = $derived(actif && EN_COURS.includes(actif.statut) ? Math.max(0, Math.round((maintenant - Date.parse(actif.modifie_le)) / 1000)) : 0);

  /** Société de chaque boîte du mail (pour « à compta@…, Frouard »). */
  let societesDuMail = $derived(m ? [...new Set(m.boites.map((b) => nomCourtSociete(b.societe)))] : []);
  /** Adresses des boîtes qui ont reçu le mail (une boîte peut être vue dans plusieurs dossiers). */
  let adressesDuMail = $derived(m ? [...new Set(m.boites.map((b) => b.adresse))] : []);

  // ---------- Chargement ----------

  async function lire(): Promise<FicheMail | null> {
    const ident = id;
    const moi = ++tour;
    const v = await api.get<FicheMail>(`/api/mails/${ident}`);
    if (ident !== id || moi !== tour) return null;
    const avant = actif;
    m = v;
    const b = v.brouillons.find((x) => ACTIFS.includes(x.statut));
    // Le texte saisi n'est remplacé que quand le brouillon change côté serveur.
    if (!b || !avant || b.id !== avant.id || b.modifie_le !== avant.modifie_le) texte = b?.texte ?? '';
    if (!boiteEnvoi || !v.boites.some((x) => String(x.boite_id) === boiteEnvoi))
      boiteEnvoi = v.boites.length ? String(v.boites[0].boite_id) : '';
    if (attenteEnvoi !== null) {
      const e = v.brouillons.find((x) => x.id === attenteEnvoi);
      if (e && e.statut !== 'envoi') {
        attenteEnvoi = null;
        if (e.statut === 'envoye') void apresEnvoi(e);
        else void rafraichirCompteurs();
      }
    }
    return v;
  }

  function suivre() {
    arret?.();
    arret = sonder(async () => {
      const v = await lire();
      if (!v) return true; // lecture dépassée par une plus récente : on continue
      const b = v.brouillons.find((x) => ACTIFS.includes(x.statut));
      return !!b && EN_COURS.includes(b.statut);
    }, 2000);
  }

  /** Encadré « fournisseur » : le fil du fournisseur de la première pièce lue qui en a un,
   *  à défaut celui reconnu par le domaine de l'expéditeur. */
  async function lireContexte(v: FicheMail) {
    const p = v.pieces.find(
      (x) => x.document_etat === 'lu' && x.document_fournisseur_id && (x.document_type === 'facture' || x.document_type === 'avoir' || x.document_type === 'bl')
    );
    const fournisseurId = p?.document_fournisseur_id ?? v.fournisseur_id;
    if (!fournisseurId) return;
    try {
      const fil = await filFournisseur(fournisseurId, { societe: societe.id, mois: 1 });
      if (v.id !== id) return;
      contexte = {
        fournisseurId: fil.fournisseur.id,
        nom: fil.fournisseur.nom,
        societe: nomCourtSociete(p?.document_societe ?? v.boites[0]?.societe ?? societe.courante?.raison_sociale) || null,
        etat: fil.etat
      };
    } catch {
      /* encadré facultatif */
    }
  }

  async function lireFile(): Promise<MailARepondre[]> {
    try {
      const f = await aTraiter.file(societe.id);
      file = f.a_repondre;
    } catch {
      file = file ?? [];
    }
    return file ?? [];
  }

  // Nouveau mail : tout repart de zéro.
  $effect(() => {
    const ident = id;
    untrack(() => ouvrirMail(ident));
    return () => arret?.();
  });

  function ouvrirMail(ident: number) {
    arret?.();
    m = null;
    erreurChargement = '';
    erreur = '';
    info = annonce.texte;
    annonce.texte = '';
    occupe = null;
    attenteEnvoi = null;
    contexte = null;
    citeOuverte = false;
    texte = '';
    lire()
      .then((v) => {
        if (!v) return;
        etaitDansFile = v.statut_reponse === 'a_repondre';
        if (v.brouillons.some((b) => EN_COURS.includes(b.statut))) suivre();
        void lireContexte(v);
      })
      .catch((e) => {
        if (ident === id) erreurChargement = e instanceof ErreurApi && e.code === 404 ? 'Mail introuvable, ou hors de vos sociétés.' : message(e);
      });
  }

  // Position dans la file À répondre (« 1 sur 5 »), relue au changement de société.
  $effect(() => {
    void societe.id;
    void lireFile();
  });

  // Chronomètre pendant la préparation ou l'envoi.
  $effect(() => {
    if (!actif || !EN_COURS.includes(actif.statut)) return;
    const t = setInterval(() => (maintenant = Date.now()), 1000);
    return () => clearInterval(t);
  });

  // Texte modifié et pas encore enregistré : gardé avant de quitter la fiche.
  beforeNavigate(() => {
    if (modifieIci && actif) void api.put(`/api/brouillons/${actif.id}`, { texte }).catch(() => {});
  });

  // ---------- Actions ----------

  async function agir(nom: string, f: () => Promise<void>) {
    erreur = '';
    info = '';
    occupe = nom;
    try {
      await f();
    } catch (e) {
      erreur = message(e);
    } finally {
      occupe = null;
    }
  }

  /** Ouvre le mail qui suivait celui-ci dans la file À répondre, sinon retour à À traiter. */
  async function passerAuSuivant() {
    const avant = file ?? [];
    const i = avant.findIndex((x) => x.id === id);
    const reste = (await lireFile()).filter((x) => x.id !== id);
    if (!reste.length) return aller(null);
    for (const x of avant.slice(i + 1)) if (reste.some((r) => r.id === x.id)) return aller(x.id);
    return aller(reste[0].id);
  }

  /** Après un classement : le mail a quitté la file, on enchaîne ; sinon on reste avec un mot. */
  async function apres(texteInfo: string) {
    const v = (await lire()) ?? m;
    void rafraichirCompteurs();
    if (v && etaitDansFile && v.statut_reponse !== 'a_repondre') {
      annonce.texte = texteInfo;
      await passerAuSuivant();
    } else {
      info = texteInfo;
      void lireFile();
    }
  }

  async function apresEnvoi(b: Brouillon) {
    void rafraichirCompteurs();
    const texteInfo = b.erreur ? `Réponse envoyée depuis ${b.boite_adresse}, mais : ${b.erreur}` : `Réponse envoyée depuis ${b.boite_adresse}.`;
    if (etaitDansFile) {
      annonce.texte = texteInfo;
      await passerAuSuivant();
    } else {
      info = texteInfo;
    }
  }

  const classer = (c: Categorie, texteInfo: string) =>
    agir(`categorie:${c}`, async () => {
      await api.put(`/api/mails/${id}/categorie`, { categorie: c });
      await apres(texteInfo);
    });

  const changerStatut = (s: 'a_repondre' | 'repondu' | 'sans_suite', texteInfo: string) =>
    agir(`statut:${s}`, async () => {
      await api.post(`/api/mails/${id}/statut-reponse`, { statut: s });
      await apres(texteInfo);
    });

  const proposer = () =>
    agir('proposer', async () => {
      await api.post(`/api/mails/${id}/brouillons`, { boite_id: Number(boiteEnvoi) || null });
      await lire();
      maintenant = Date.now();
      suivre();
      void rafraichirCompteurs();
    });

  const enregistrer = () =>
    agir('enregistrer', async () => {
      await api.put(`/api/brouillons/${actif!.id}`, { texte });
      await lire();
    });

  async function enregistrerEnSilence() {
    if (!modifieIci || occupe || !actif) return;
    const b = actif;
    try {
      await api.put(`/api/brouillons/${b.id}`, { texte });
      await lire();
    } catch {
      /* gardé à l'écran ; « Enregistrer » redira l'erreur */
    }
  }

  const envoyer = () =>
    agir('envoyer', async () => {
      const b = actif!;
      if (texte !== b.texte) await api.put(`/api/brouillons/${b.id}`, { texte });
      await api.post(`/api/brouillons/${b.id}/envoyer`);
      attenteEnvoi = b.id;
      maintenant = Date.now();
      await lire();
      suivre();
    });

  const effacer = () =>
    agir('effacer', async () => {
      await api.post(`/api/brouillons/${actif!.id}/abandonner`);
      await lire();
      void rafraichirCompteurs();
    });

  const marquer = (portee: 'adresse' | 'domaine', cible: string) =>
    agir(`marquer:${portee}`, async () => {
      const r = await api.post<{ reclasses: number }>('/api/admin/expediteurs-marques', { mail_id: id, portee });
      const n = r?.reclasses ?? 0;
      await apres(`${cible} classé en publicité (${n} mail${n > 1 ? 's' : ''} reclassé${n > 1 ? 's' : ''}).`);
    });

  // ---------- Confirmations ----------

  type Demande = { genre: string; titre: string; libelle: string; danger?: boolean; texte: string; faire: () => Promise<void> };
  let demande = $state<Demande | null>(null);
  let confirmationOuverte = $state(false);

  function demander(d: Demande) {
    demande = d;
    confirmationOuverte = true;
  }

  /** Envoi toujours confirmé (règle de Michael, 2026-11-22) ; « [à compléter] » restant signalé en rouge. */
  function demanderEnvoi() {
    if (!peutEnvoyer || !actif) return;
    const reste = aCompleter
      ? ` Attention : ${aCompleter === 1 ? 'un passage « [à compléter] » reste' : `${aCompleter} passages « [à compléter] » restent`} dans le texte.`
      : '';
    demander({
      genre: 'envoi',
      titre: `Envoyer depuis ${actif.boite_adresse} ?`,
      libelle: 'Envoyer',
      danger: aCompleter > 0,
      texte: `La réponse part à ${m?.expediteur || 'l’expéditeur'} telle qu’affichée, dans le fil du mail reçu.${reste}`,
      faire: envoyer
    });
  }

  function demanderEffacer() {
    demander({
      genre: 'effacer',
      titre: 'Effacer la réponse proposée ?',
      libelle: 'Effacer',
      danger: true,
      texte: 'Le texte est perdu. Le mail reste dans À répondre.',
      faire: effacer
    });
  }

  function demanderAutre() {
    if (!modifieIci && actif?.statut !== 'modifie') return void proposer();
    demander({
      genre: 'autre',
      titre: 'Proposer une autre réponse ?',
      libelle: 'Remplacer',
      danger: true,
      texte: 'Vos modifications seront remplacées par un nouveau texte de Claude.',
      faire: proposer
    });
  }

  function demanderMarque(portee: 'adresse' | 'domaine') {
    if (!m) return;
    const adresse = m.expediteur.match(/<([^>]+)>/)?.[1] ?? m.expediteur;
    const cible = portee === 'adresse' ? adresse.trim() : (adresse.split('@')[1] ?? '').trim();
    demander({
      genre: 'marque',
      titre: `Toujours classer ${cible} en publicité ?`,
      libelle: 'Classer en publicité',
      texte: `Tous les mails de ${cible}, y compris les prochains, iront en publicité (purgés après 90 jours).`,
      faire: () => marquer(portee, cible)
    });
  }

  // ---------- Clavier ----------

  function voisin(sens: 1 | -1) {
    if (!file || indiceFile < 0) return false;
    const x = file[indiceFile + sens];
    if (x) void aller(x.id);
  }

  $effect(() =>
    raccourcis(
      {
        'Ctrl+Enter': () => {
          if (confirmationOuverte && demande?.genre === 'envoi') {
            confirmationOuverte = false;
            void demande.faire();
            return;
          }
          if (confirmationOuverte || !peutEnvoyer) return false;
          demanderEnvoi();
        },
        ...(volet
          ? {}
          : {
              j: () => voisin(1),
              k: () => voisin(-1),
              Escape: () => {
                if (confirmationOuverte) return false; // le dialogue se ferme tout seul
                void goto(retour.href);
              }
            })
      },
      { dansLesChamps: ['Ctrl+Enter'] }
    )
  );

  // ---------- Pièces jointes ----------
  const apercable = (p: { nom: string; type_mime: string }) =>
    /^(application\/(pdf|x-pdf)|image\/(png|jpe?g|gif|webp))$/i.test(p.type_mime) ||
    /spreadsheet|excel|csv/i.test(p.type_mime) ||
    /\.(pdf|xlsx|xlsm|xls|ods|csv)$/i.test(p.nom);
  let pieceOuverte = $state<number | null>(null);
  let pieceVue = $derived(m?.pieces.find((p) => p.id === pieceOuverte) ?? null);
  /** Pièce lue comme facture, avoir ou BL (les autres sont « écartées »). */
  const utile = (p: FicheMail['pieces'][number]) =>
    p.document_etat === 'lu' && (p.document_type === 'facture' || p.document_type === 'avoir' || p.document_type === 'bl');
  // Un autre mail : la première facture, avoir ou BL qui a un aperçu s'ouvre (pas les logos de signature).
  let dernierMail = 0;
  $effect(() => {
    if (!m || m.id === dernierMail) return;
    dernierMail = m.id;
    pieceOuverte = m.pieces.find((p) => utile(p) && apercable(p))?.id ?? null;
  });

  // ---------- Ne concerne pas cette société ----------
  /** Société pour laquelle masquer : celle du responsable, celle choisie en haut, sinon toutes. */
  let societeMasque = $derived(admin ? societe.courante : (societe.liste.find((s) => s.id === session.moi?.societe_id) ?? null));
  let libelleMasque = $derived(societeMasque ? `Ne concerne pas ${nomCourtSociete(societeMasque.raison_sociale)}` : 'Ne concerne aucune de nos sociétés');
  let dlgMasquer = $state(false);
  let masque = $state<{ documents: number } | null>(null);
  const corpsMasque = () => ({ societe_id: admin ? (societeMasque?.id ?? null) : null });
  async function masquer() {
    if (!m) return;
    erreur = '';
    try {
      masque = await api.post<{ documents: number }>(`/api/mails/${m.id}/hors-societe`, corpsMasque());
      void rafraichirCompteurs();
      await ouvrir?.(null);
    } catch (e) {
      erreur = message(e);
      throw e;
    }
  }
  async function annulerMasque() {
    if (!m) return;
    erreur = '';
    try {
      await api.del(`/api/mails/${m.id}/hors-societe`, corpsMasque());
      masque = null;
      info = 'Le mail est de retour dans les listes.';
      void rafraichirCompteurs();
      await ouvrir?.(null);
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    void id;
    masque = null;
  });

  // ---------- Affichage ----------

  function etatPiece(p: FicheMail['pieces'][number]): { texte: string; ton: 'ok' | 'alerte' | 'erreur' | 'pale' | 'neutre' } {
    if (!p.document_id || !p.document_etat) return { texte: `Pas lue, ${taille(p.taille)}`, ton: 'pale' };
    if (p.document_etat !== 'lu') {
      const ton = p.document_etat === 'erreur' ? 'erreur' : 'pale';
      return { texte: libelleEtat(p.document_etat), ton };
    }
    const type = libelleType(p.document_type, true);
    const quoi = [`${type}${p.document_numero ? ` ${p.document_numero}` : ''}`];
    if (p.document_montant_ttc !== null) quoi.push(formatMontant(p.document_montant_ttc));
    const avec = (etat: string) => [...quoi, etat].join(', ');
    if (p.document_type === 'facture' || p.document_type === 'avoir') {
      const outil = nomOutil(p.document_outil);
      if (p.document_statut_depot === 'depose')
        return { texte: avec(`envoyée à ${outil}${p.document_envoye_le ? ` le ${dateMail(p.document_envoye_le, false)}` : ''}`), ton: 'ok' };
      const st = p.document_statut_depot;
      const s = libelleStatutEnvoi(st, p.document_outil);
      const ton = st === 'erreur' || st === 'doublon' ? 'erreur' : st === 'a_verifier' ? 'alerte' : 'neutre';
      return { texte: avec(s ? s.charAt(0).toLowerCase() + s.slice(1) : 'lue'), ton };
    }
    if (p.document_type === 'bl') return p.document_verifie ? { texte: avec('validé'), ton: 'ok' } : { texte: avec('à valider'), ton: 'alerte' };
    return { texte: avec('lu'), ton: 'neutre' };
  }

  /** « betty.pecqueur@lafoirfouille.fr, aujourd’hui à 14:38 » */
  let meta = $derived.by(() => {
    if (!m) return '';
    const de = adresseExpediteur(m.expediteur) || 'Expéditeur inconnu';
    return m.date_mail ? `${de}, ${dateLongue(m.date_mail)}` : de;
  });

  let etatTri = $derived.by(() => {
    if (!m) return '';
    if (m.categorie) return `${libelleCategorie(m.categorie)}${m.categorie_par ? `, ${libelleTriePar(m.categorie_par)}` : ''}`;
    if (m.tri_etat === 'erreur') return 'Tri en erreur';
    return 'Pas encore trié';
  });

  /** Ce que dit l'encadré « En une phrase » quand Claude n'a pas encore résumé le mail. */
  let sansResume = $derived.by(() => {
    if (!m) return '';
    if (m.tri_etat === 'erreur') return 'Le tri de ce mail a échoué : pas de résumé.';
    if (m.tri_etat === 'a_trier' || m.tri_etat === 'en_cours' || !m.tri_etat) return 'Claude n’a pas encore lu ce mail.';
    return 'Pas de résumé pour ce mail.';
  });

  /** « 3 pièces jointes » + « écartées, aucune n’est une facture » ou « : 1 facture, 2 écartées ». */
  let piecesTitre = $derived.by(() => {
    if (!m || !m.pieces.length) return { n: '', suite: '' };
    const ps = m.pieces;
    const n = `${ps.length} pièce${ps.length > 1 ? 's' : ''} jointe${ps.length > 1 ? 's' : ''}`;
    const compte = (f: (p: FicheMail['pieces'][number]) => boolean) => ps.filter(f).length;
    const factures = compte((p) => utile(p) && p.document_type === 'facture');
    const avoirs = compte((p) => utile(p) && p.document_type === 'avoir');
    const bls = compte((p) => utile(p) && p.document_type === 'bl');
    const enLecture = compte((p) => p.document_etat === 'a_lire' || p.document_etat === 'en_cours');
    const erreurs = compte((p) => p.document_etat === 'erreur');
    const ecartees = ps.length - factures - avoirs - bls - enLecture - erreurs;
    if (ecartees === ps.length)
      return { n, suite: ps.length > 1 ? 'écartées, aucune n’est une facture' : 'écartée, ce n’est pas une facture' };
    const pl = (k: number, un: string, plusieurs = `${un}s`) => (k ? [`${k} ${k > 1 ? plusieurs : un}`] : []);
    const morceaux = [
      ...pl(factures, 'facture'),
      ...pl(avoirs, 'avoir'),
      ...pl(bls, 'BL', 'BL'),
      ...pl(enLecture, 'en lecture', 'en lecture'),
      ...pl(erreurs, 'en erreur', 'en erreur'),
      ...pl(ecartees, 'écartée')
    ];
    return { n, suite: `: ${morceaux.join(', ')}` };
  });

  /** Fiche document du lien « Ouvrir la fiche du document » : la pièce ouverte, sinon la première lue. */
  let documentLien = $derived(
    pieceVue?.document_id ?? m?.pieces.find(utile)?.document_id ?? m?.pieces.find((p) => p.document_id)?.document_id ?? null
  );

  let etatReponse = $derived.by(() => {
    if (!m) return '';
    if (m.statut_reponse === 'a_repondre') return 'Réponse attendue, pas encore écrite';
    if (m.statut_reponse === 'repondu') return 'Déjà répondu';
    if (m.statut_reponse === 'sans_suite') return 'Classé : pas besoin de réponse';
    return 'Pas de réponse attendue pour ce mail';
  });

  let boiteBrouillon = $derived(actif?.boite_adresse ?? m?.boites.find((b) => String(b.boite_id) === boiteEnvoi)?.adresse ?? '');
  /** Barre du haut : retour (Échap) en page pleine ; en volet, seulement la position dans À répondre (ou le retour sur téléphone). */
  let avecPosition = $derived(indiceFile >= 0 && !!file);
</script>

<div class="volet-h" class:tel-seul={volet && !avecPosition}>
  {#if volet}
    <a class="retour tel-seul" href={retour.href}><Icone nom="gauche" />{retour.libelle}</a>
  {:else}
    <a class="retour" href={retour.href}><Icone nom="gauche" />{retour.libelle}<Kbd t="Escape" /></a>
  {/if}
  {#if avecPosition && file}
    {#if !volet}<span class="vsep"></span>{/if}
    <span class="pos">À répondre <span class="num">{indiceFile + 1}</span> sur <span class="num">{file.length}</span></span>
  {/if}
  <span class="sp"></span>
  {#if avecPosition && file}
    <button type="button" class="btn sm fantome" aria-label="Mail précédent" disabled={indiceFile <= 0} onclick={() => voisin(-1)}>
      <Icone nom="haut" /><Kbd t="K" />
    </button>
    <button type="button" class="btn sm fantome" aria-label="Mail suivant" disabled={indiceFile >= file.length - 1} onclick={() => voisin(1)}>
      <Icone nom="bas" /><Kbd t="J" />
    </button>
  {/if}
</div>

{#if erreurChargement}
  <div class="charge-erreur">
    <Bandeau ton="erreur" titre="Impossible d’ouvrir ce mail.">
      {erreurChargement}
      {#snippet action()}<a class="btn sm" href={retour.href}>{retour.libelle}</a>{/snippet}
    </Bandeau>
  </div>
{:else if !m}
  <Chargement texte="Ouverture du mail" />
{:else}
  <div class="mail-b" class:redaction={!!actif}>
    <!-- ======== le mail ======== -->
    <article class="mail-l" aria-label="Mail reçu">
      {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
      {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}

      <header class="tete">
        <h1>{m.objet || '(sans objet)'}</h1>
        <div class="meta">
          <span>{meta}</span>
          {#if m.priorite === 'haute' && aRepondre}<span class="urgent">{libellePriorite(m.priorite)}</span>{/if}
          {#if !masque}
            <button type="button" class="masquer" onclick={() => (dlgMasquer = true)}><Icone nom="fermer" />{libelleMasque}</button>
          {/if}
        </div>
      </header>
      {#if masque}
        <Bandeau titre="Mail masqué.">
          Il n’apparaît plus dans les listes{societeMasque ? ` de ${nomCourtSociete(societeMasque.raison_sociale)}` : ''}{masque.documents
            ? ` ; ${masque.documents} pièce${masque.documents > 1 ? 's' : ''} écartée${masque.documents > 1 ? 's' : ''}`
            : ''}.
          {#snippet action()}<button type="button" class="btn sm" onclick={annulerMasque}>Annuler</button>{/snippet}
        </Bandeau>
      {/if}

      <section class="phrase" aria-label="En une phrase">
        <span class="lab">En une phrase</span>
        {#if m.resume}<p>{m.resume}</p>{:else}<p class="sans">{sansResume}</p>{/if}
        <div class="classe">
          <span class="cl">
            {#if m.categorie}
              <span>Classé en <b>{libelleCategorie(m.categorie)}</b>{m.categorie_par ? ` ${libelleTriePar(m.categorie_par)}` : ''}</span>
            {:else}
              <span>{etatTri}</span>
            {/if}
            {#if m.statut_reponse}<Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>{/if}
          </span>
          <span class="boutons">
            {#if aRepondre}
              <span class="mal">Mal trié ?</span>
              <button type="button" class="btn" class:en-cours={occupe === 'statut:sans_suite'} disabled={!!occupe} onclick={() => changerStatut('sans_suite', 'Classé : pas besoin de réponse.')}>
                {ACTIONS_REPONSE.sansSuite}
              </button>
              <button type="button" class="btn" class:en-cours={occupe === 'categorie:information'} disabled={!!occupe} onclick={() => classer('information', 'Classé en Information : le tri s’en souviendra.')}>
                Information
              </button>
              <button type="button" class="btn" class:en-cours={occupe === 'categorie:publicite'} disabled={!!occupe} onclick={() => classer('publicite', 'Classé en Publicité : le tri s’en souviendra.')}>
                Publicité
              </button>
              {#if m.categorie === 'question' && m.categorie_par !== 'humain'}
                <button type="button" class="btn" class:en-cours={occupe === 'categorie:question'} disabled={!!occupe} onclick={() => classer('question', 'Question confirmée : le tri s’en souviendra.')}>
                  Vraie question
                </button>
              {:else if m.categorie_par === 'humain'}
                <span class="mal">Question confirmée</span>
              {/if}
            {:else if m.statut_reponse === 'repondu' || m.statut_reponse === 'sans_suite' || (m.categorie === 'question' && !m.statut_reponse)}
              <button type="button" class="btn" disabled={!!occupe} class:en-cours={occupe === 'statut:a_repondre'} onclick={() => changerStatut('a_repondre', 'Remis dans À répondre.')}>
                Remettre à répondre
              </button>
            {:else if m.categorie !== 'question'}
              <button type="button" class="btn" disabled={!!occupe} class:en-cours={occupe === 'categorie:question'} onclick={() => classer('question', 'Classé en Question : ajouté à À répondre.')}>
                C’est une question
              </button>
            {/if}
          </span>
        </div>
      </section>

      <dl class="entetes">
        <div><dt>De</dt><dd title={m.expediteur}>{adresseExpediteur(m.expediteur) || '—'}</dd></div>
        <div><dt>À</dt><dd>{m.destinataires.join(', ') || '—'}</dd></div>
        <div><dt>Reçu</dt><dd>{[...adressesDuMail, ...societesDuMail].join(', ') || '—'}</dd></div>
      </dl>

      {#if m.pieces.length}
        <section class="pieces" aria-label="Pièces jointes">
          <div class="pieces-h">
            <h3>{piecesTitre.n} <span>{piecesTitre.suite}</span></h3>
            {#if documentLien}<a class="lien-doc" href="/documents/{documentLien}">Ouvrir la fiche du document</a>{/if}
          </div>
          <div class="pj-liste">
            {#each m.pieces as p (p.id)}
              {@const e = etatPiece(p)}
              {@const detaillee = utile(p) || p.document_etat === 'a_lire' || p.document_etat === 'en_cours' || p.document_etat === 'erreur'}
              <button
                type="button"
                class="pj"
                class:detaillee
                aria-pressed={p.id === pieceOuverte}
                title="{e.texte}. {apercable(p) ? 'Afficher l’aperçu' : 'Pas d’aperçu : téléchargez le fichier'}"
                onclick={() => (pieceOuverte = p.id === pieceOuverte ? null : p.id)}
              >
                {#if detaillee}<Icone nom="fichier" />{/if}
                <span class="nom">{p.nom}</span>
                {#if detaillee}<span class="etat etat-{e.ton}">{e.texte}</span>{/if}
              </button>
            {/each}
          </div>
          {#if pieceVue}
            <div class="pj-apercu">
              <ApercuPiece id={pieceVue.id} typeMime={pieceVue.type_mime} nom={pieceVue.nom} />
            </div>
          {/if}
        </section>
      {/if}

      <div class="mailtext">
        {#each corps.paragraphes as p, i (i)}<p>{p}</p>{:else}<p class="muted">(corps vide)</p>{/each}
        {#if corps.cite}
          <button type="button" class="btn-lien cite-bouton" aria-expanded={citeOuverte} onclick={() => (citeOuverte = !citeOuverte)}>
            {citeOuverte ? 'Masquer le message cité' : 'Afficher le message cité'}
          </button>
          {#if citeOuverte}<div class="cite">{corps.cite}</div>{/if}
        {/if}
      </div>

      {#if contexte}
        <div class="ctx">
          <h3><a href="/fournisseurs/{contexte.fournisseurId}">{contexte.nom}</a>{contexte.societe ? ` chez ${contexte.societe}` : ''}</h3>
          <div><span>Factures ce mois</span><span class="num">{contexte.etat.factures_mois}</span></div>
          {#if contexte.etat.a_valider}<div><span>À valider</span><span class="num">{contexte.etat.a_valider}</span></div>{/if}
          <div><span>BL en attente de facture</span><span class="num">{contexte.etat.bl_sans_facture}</span></div>
        </div>
      {/if}

      <div class="replis">
        <Repli icone="drapeau" titre="Tri" resume={etatTri}>
          {#if m.tri_message && m.categorie_par !== 'humain'}<p class="t2">{m.tri_message}</p>{/if}
          <div class="ligne-tri">
            <label for="cat-{m.id}">Catégorie</label>
            <select
              id="cat-{m.id}"
              value={m.categorie ?? ''}
              disabled={!!occupe}
              onchange={(e) => {
                const c = e.currentTarget.value as Categorie;
                void classer(c, `Classé en ${libelleCategorie(c)} : le tri automatique ne le changera plus.`);
              }}
            >
              {#if !m.categorie}<option value="" disabled>Choisir</option>{/if}
              {#each CATEGORIES as c (c)}<option value={c}>{libelleCategorie(c)}</option>{/each}
            </select>
          </div>
          <div class="actions-tri">
            {#if aRepondre}
              <button type="button" class="btn sm" disabled={!!occupe} onclick={() => changerStatut('repondu', 'Noté comme répondu ailleurs.')}>Répondu ailleurs</button>
            {/if}
            {#if admin && m.expediteur.includes('@')}
              <span class="t2 petit">Toujours publicité :</span>
              <button type="button" class="btn sm" disabled={!!occupe} onclick={() => demanderMarque('adresse')}>cet expéditeur</button>
              <button type="button" class="btn sm" disabled={!!occupe} onclick={() => demanderMarque('domaine')}>ce domaine</button>
            {/if}
          </div>
          <p class="petit muted">Chaque classement à la main sert d’exemple au tri des mails suivants ; après trois mails d’un même expéditeur classés pareil, ses mails suivants le sont directement.</p>
        </Repli>
        <Repli icone="info" titre="Détails techniques">
          <div class="from details">
            <span>Expéditeur</span><span>{m.expediteur || '—'}</span>
            <span>Message-ID</span><span class="num">{m.message_id}</span>
            {#each m.boites as b (b.boite_id)}<span>Boîte</span><span>{b.adresse}{b.libelle !== b.adresse ? ` (${b.libelle})` : ''}, dossier {b.dossier}</span>{/each}
            {#each m.pieces as p (p.id)}<span>Pièce</span><span>{p.nom} <span class="muted">({p.type_mime}, {taille(p.taille)})</span></span>{/each}
          </div>
        </Repli>
      </div>
    </article>

    <!-- ======== la réponse ======== -->
    <aside class="mail-r" aria-label="Réponse">
      <h2>Réponse</h2>
      {#if actif && editable}
        <p class="etat-rep">{actif.statut === 'modifie' || modifieIci ? 'Proposée par Claude, modifiée' : 'Proposée par Claude, à relire'}</p>
        {#if actif.erreur}<Bandeau ton={actif.erreur.includes('envoi') ? 'erreur' : 'attention'} titre={actif.erreur.includes('envoi') ? 'L’envoi n’est pas parti.' : undefined}>{actif.erreur}</Bandeau>{/if}
        <textarea
          class="editeur"
          bind:value={texte}
          disabled={!!occupe}
          spellcheck="true"
          aria-label="Texte de la réponse"
          onblur={enregistrerEnSilence}
        ></textarea>
        {#if aCompleter}
          <div class="rep-n"><Icone nom="alerte" />{aCompleter === 1 ? 'Un passage « [à compléter] » reste à remplir avant l’envoi' : `${aCompleter} passages « [à compléter] » restent à remplir avant l’envoi`}</div>
        {/if}
        {#if !actif.smtp}
          <div class="rep-n"><Icone nom="alerte" />Envoi impossible : le serveur d’envoi de {actif.boite_adresse} n’est pas configuré (Réglages, Boîtes mail)</div>
        {/if}
        <button type="button" class="btn principal plein envoyer" class:en-cours={occupe === 'envoyer'} disabled={!peutEnvoyer} onclick={demanderEnvoi}>
          <span>{libelleEnvoyerDepuis(actif.boite_adresse)}</span><Kbd t={['Ctrl', 'Enter']} />
        </button>
        {#if modifieIci}
          <button type="button" class="btn plein" disabled={!!occupe} class:en-cours={occupe === 'enregistrer'} onclick={enregistrer}>Enregistrer</button>
        {/if}
        <div class="rep-autres">
          <button type="button" class="btn sm fantome" disabled={!!occupe} class:en-cours={occupe === 'proposer'} onclick={demanderAutre}>{ACTIONS_REPONSE.autre}</button>
          <button type="button" class="btn sm fantome" disabled={!!occupe} onclick={demanderEffacer}>{ACTIONS_REPONSE.effacer}</button>
        </div>
        <p class="note">Rien ne part sans votre accord : l’envoi est confirmé. La réponse reste dans le fil du mail reçu.</p>
      {:else if actif && actif.statut === 'generation'}
        <p class="etat-rep">{libelleBrouillon('generation')}</p>
        <div class="attente" role="status" aria-live="polite">
          <span class="roue" aria-hidden="true"></span>
          <div>
            <div>Claude rédige une réponse dans le ton de vos réponses précédentes.</div>
            <div class="note">
              Depuis <span class="num">{depuisSecondes}</span> s, depuis {actif.boite_adresse}.
              {#if depuisSecondes > 90}C’est plus long que d’habitude : vous pouvez quitter la page, la réponse vous attendra ici.{/if}
            </div>
          </div>
        </div>
      {:else if actif && actif.statut === 'envoi'}
        <p class="etat-rep">{libelleBrouillon('envoi')}</p>
        <textarea class="editeur" value={actif.texte} disabled aria-label="Texte de la réponse"></textarea>
        <div class="attente" role="status" aria-live="polite">
          <span class="roue" aria-hidden="true"></span>
          <div>
            <div>Envoi depuis {actif.boite_adresse}.</div>
            <div class="note">Depuis <span class="num">{depuisSecondes}</span> s.</div>
          </div>
        </div>
        {#if depuisSecondes > 20 && etaitDansFile}
          <button type="button" class="btn plein" onclick={passerAuSuivant}>Passer au suivant</button>
        {/if}
      {:else}
        <p class="etat-rep">{etatReponse}</p>
        {#if m.boites.length > 1}
          <label class="depuis">
            <span>Répondre depuis</span>
            <select bind:value={boiteEnvoi} disabled={!!occupe}>
              {#each m.boites as b (b.boite_id)}<option value={String(b.boite_id)}>{b.adresse}</option>{/each}
            </select>
          </label>
        {/if}
        <button
          type="button"
          class="btn plein haut"
          class:principal={aRepondre || m.categorie === 'question'}
          class:en-cours={occupe === 'proposer'}
          disabled={!!occupe || !m.boites.length}
          onclick={proposer}
        >
          {ACTIONS_REPONSE.proposer}
        </button>
        <p class="note">Claude propose un texte depuis {boiteBrouillon || 'la boîte du mail'}. Vous le relisez, le retouchez et l’envoyez vous-même.</p>
      {/if}

      {#if envoyes.length}
        <div class="replis envoyes">
          {#each envoyes as b (b.id)}
            <Repli icone="envoyer" titre="Réponse envoyée" resume="{dateMail(b.envoye_le)}{b.envoye_par ? `, par ${b.envoye_par}` : ''}, depuis {b.boite_adresse}">
              {#if b.erreur}<Bandeau>{b.erreur}</Bandeau>{/if}
              <div class="texte-envoye">{b.texte}</div>
            </Repli>
          {/each}
        </div>
      {/if}
    </aside>
  </div>
{/if}

<Confirmation
  bind:ouvert={confirmationOuverte}
  titre={demande?.titre ?? ''}
  libelle={demande?.libelle ?? 'Confirmer'}
  danger={demande?.danger ?? false}
  onconfirmer={() => demande?.faire()}
>
  {demande?.texte ?? ''}
  {#if demande?.genre === 'envoi'}<span class="large-seul petit muted"> <Kbd t={['Ctrl', 'Enter']} /> pour envoyer.</span>{/if}
</Confirmation>

<Confirmation bind:ouvert={dlgMasquer} titre="{libelleMasque} ?" libelle="Masquer le mail" onconfirmer={masquer}>
  <p>
    Le mail disparaît des listes{societeMasque ? ` de ${nomCourtSociete(societeMasque.raison_sociale)}` : ''}. Ses factures et BL qui ne sont pas encore
    partis sont écartés : jamais envoyés à Dext ou Yooz. « Annuler » remet tout en place.
  </p>
</Confirmation>

<style>
  .charge-erreur {
    padding: var(--e5) var(--e6);
  }
  /* Le mail au centre, la colonne « Réponse » à droite (fond sauge). */
  .mail-b {
    flex: 1;
    display: flex;
    min-height: 0;
    background: var(--white);
  }
  .mail-l {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: var(--e8);
    display: flex;
    flex-direction: column;
    gap: var(--e6);
  }
  .mail-r {
    flex: none;
    width: 300px;
    overflow: auto;
    padding: var(--e8) var(--e6);
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    background: var(--pane);
    border-left: 1px solid var(--line);
    min-width: 0;
  }
  @media (min-width: 1400px) {
    .mail-b.redaction .mail-r {
      width: 380px;
    }
  }

  /* ---- en-tête du mail ---- */
  .tete {
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .tete h1 {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 500;
    overflow-wrap: anywhere;
    text-wrap: pretty;
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e2) var(--e4);
    font-size: 15px;
    color: var(--text2);
    overflow-wrap: anywhere;
  }
  .urgent {
    color: var(--err);
    font-weight: 500;
  }
  .masquer {
    display: inline-flex;
    align-items: center;
    gap: var(--e1);
    padding: 0;
    border: 0;
    background: none;
    color: var(--text2);
    font: inherit;
    cursor: pointer;
  }
  .masquer :global(svg) {
    width: 14px;
    height: 14px;
  }
  .masquer:hover {
    color: var(--text);
    text-decoration: underline;
  }

  /* ---- En une phrase ---- */
  .phrase {
    display: flex;
    flex-direction: column;
    gap: var(--e2);
    padding: var(--e5) var(--e6);
    border-radius: var(--r3);
    background: var(--pane);
    box-shadow: 0 0 0 1px var(--line);
  }
  .phrase .lab {
    font-size: 14px;
    font-weight: 500;
    color: var(--text2);
  }
  .phrase p {
    margin: 0;
    font-size: 19px;
    line-height: 1.45;
    text-wrap: pretty;
  }
  .phrase p.sans {
    font-size: 16px;
    color: var(--text2);
  }
  .classe {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--e3);
    padding-top: var(--e2);
  }
  .cl {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e2);
    font-size: 15px;
    color: var(--text2);
  }
  .cl b {
    color: var(--text);
    font-weight: 500;
  }
  .boutons {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e2);
  }
  .boutons .mal {
    font-size: 15px;
    color: var(--text2);
  }
  .boutons .btn {
    height: 36px;
    padding: 0 var(--e4);
    background: var(--white);
    font-size: 16px;
  }

  /* ---- De / À / Reçu ---- */
  .entetes {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--e2);
    font-size: 16px;
  }
  .entetes > div {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
    gap: var(--e3);
  }
  .entetes dt {
    color: var(--muted);
  }
  .entetes dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  /* ---- Pièces jointes ---- */
  .pieces {
    display: flex;
    flex-direction: column;
    gap: var(--e3);
    padding-top: var(--e4);
    border-top: 1px solid var(--line-faint);
  }
  .pieces-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--e3);
  }
  .pieces-h h3 {
    font-size: 17px;
    font-weight: 500;
  }
  .pieces-h h3 span {
    font-weight: 400;
    color: var(--text2);
  }
  .lien-doc {
    font-size: 15px;
    font-weight: 500;
    text-decoration: none;
  }
  .lien-doc:hover {
    text-decoration: underline;
  }
  .pj-liste {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e2);
  }
  .pj {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    min-width: 0;
    padding: 0 var(--e2);
    border: 0;
    border-radius: var(--r-etiq);
    background: var(--sunk);
    color: var(--text2);
    font: inherit;
    font-size: 14px;
    line-height: 24px;
    text-align: left;
    cursor: pointer;
  }
  .pj:hover {
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .pj[aria-pressed='true'] {
    background: var(--accent-weak);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .pj.detaillee {
    padding: 6px var(--e3) 6px var(--e2);
    border-radius: var(--r2);
    line-height: 1.35;
    background: var(--white);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text);
  }
  .pj.detaillee[aria-pressed='true'] {
    background: var(--accent-weak);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .pj :global(svg) {
    flex: none;
    width: 16px;
    height: 16px;
    color: var(--text2);
  }
  .pj .nom {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .pj.detaillee .nom {
    font-weight: 500;
  }
  .pj .etat {
    color: var(--text2);
  }
  .pj .etat::before {
    content: '· ';
    color: var(--muted);
  }
  .pj .etat-ok {
    color: var(--ok);
  }
  .pj .etat-alerte {
    color: var(--warn);
  }
  .pj .etat-erreur {
    color: var(--err);
  }
  .pj-apercu {
    display: flex;
    flex-direction: column;
    height: 70vh;
    min-height: 420px;
    border: 1px solid var(--line);
    border-radius: var(--r3);
    overflow: hidden;
    background: var(--pane);
  }

  /* ---- corps (affichage du webmail) ---- */
  .mailtext {
    font-size: 16px;
    line-height: 1.55;
    color: var(--text);
    max-width: 70ch;
    overflow-wrap: anywhere;
  }
  .mailtext p {
    margin: 0 0 var(--e3);
    white-space: pre-line;
  }
  .cite-bouton {
    font-size: 14px;
  }
  .cite {
    margin-top: var(--e2);
    padding-left: var(--e3);
    border-left: 2px solid var(--line);
    color: var(--text2);
    white-space: pre-wrap;
    font-size: 14px;
    line-height: 1.5;
  }

  /* ---- fournisseur, tri, détails ---- */
  .ctx {
    font-size: 15px;
    color: var(--text2);
  }
  .ctx h3 {
    margin: 0 0 var(--e1);
    font-size: 15px;
    font-weight: 500;
    color: var(--text);
  }
  .ctx h3 a {
    color: inherit;
  }
  .ctx div {
    display: flex;
    justify-content: space-between;
    padding: var(--e2) 0;
    border-bottom: 1px solid var(--line-faint);
  }
  .mail-l .replis {
    margin-top: 0;
  }
  .from {
    display: grid;
    grid-template-columns: 96px 1fr;
    row-gap: var(--e1);
    column-gap: var(--e3);
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .from > span:nth-child(odd) {
    color: var(--muted);
  }
  .ligne-tri {
    display: flex;
    align-items: center;
    gap: var(--e3);
    margin-bottom: var(--e3);
  }
  .ligne-tri select {
    width: auto;
    min-width: 220px;
  }
  .actions-tri {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-bottom: var(--e3);
  }

  /* ---- colonne Réponse ---- */
  .mail-r h2 {
    font-size: 20px;
    font-weight: 500;
  }
  .etat-rep {
    margin: 0;
    font-size: 16px;
    color: var(--text2);
  }
  .mail-r .btn.plein {
    width: 100%;
    justify-content: center;
  }
  .mail-r .btn.haut {
    height: 44px;
  }
  .mail-r .btn:not(.principal):not(.fantome) {
    background: var(--white);
  }
  .envoyer {
    height: auto;
    min-height: 44px;
    padding-top: var(--e2);
    padding-bottom: var(--e2);
    white-space: normal;
    flex-wrap: wrap;
    text-align: center;
    overflow-wrap: anywhere;
  }
  .note {
    margin: 0;
    font-size: 15px;
    color: var(--muted);
    text-wrap: pretty;
  }
  .depuis {
    display: flex;
    flex-direction: column;
    gap: var(--e1);
    font-size: 15px;
    color: var(--text2);
  }
  .editeur {
    flex: 1;
    min-height: 320px;
    padding: var(--e3) 14px;
    font-size: 16px;
    line-height: 1.5;
    resize: vertical;
    background: var(--white);
  }
  .rep-n {
    font-size: 14px;
    color: var(--warn);
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .rep-n :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .rep-autres {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e1);
    justify-content: center;
  }
  .attente {
    display: flex;
    align-items: flex-start;
    gap: var(--e3);
    font-size: 15px;
  }
  .roue {
    flex: none;
    width: 16px;
    height: 16px;
    margin-top: 3px;
    border: 1.5px solid var(--line);
    border-top-color: var(--text2);
    border-radius: 50%;
    animation: tourne 0.8s linear infinite;
  }
  .envoyes {
    margin-top: var(--e2);
  }
  .texte-envoye {
    white-space: pre-wrap;
    font-size: 15px;
    line-height: 1.5;
    color: var(--text2);
  }
  @keyframes tourne {
    to {
      transform: rotate(360deg);
    }
  }

  /* Écran moyen et téléphone : le mail, puis la réponse dessous. */
  @media (max-width: 1200px) {
    .mail-b {
      display: block;
      overflow: auto;
    }
    .mail-l {
      overflow: visible;
    }
    .mail-r {
      width: auto;
      overflow: visible;
      border-left: 0;
      border-top: 1px solid var(--line);
    }
    .editeur {
      min-height: 280px;
    }
  }
  @media (max-width: 760px) {
    .mail-b {
      overflow: visible;
    }
    .mail-l {
      padding: var(--e5) var(--e4);
      gap: var(--e5);
    }
    .mail-r {
      padding: var(--e5) var(--e4) var(--e8);
    }
    .phrase {
      padding: var(--e4);
    }
    .phrase p {
      font-size: 18px;
    }
    .entetes > div {
      grid-template-columns: 56px minmax(0, 1fr);
    }
    .pj .nom {
      max-width: 220px;
    }
    .envoyer {
      min-height: 48px;
    }
    .ligne-tri select,
    .depuis select {
      min-width: 0;
      flex: 1;
    }
  }
</style>
