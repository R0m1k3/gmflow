<script lang="ts" module>
  // Fiche mail (maquette B, écran 3) : le mail et ses pièces à gauche, la réponse à droite.
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
    retour = { href: '/', libelle: 'À traiter' },
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
    ouvrir ? ouvrir(suivant) : goto(suivant ? `/mails/${suivant}` : '/');

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

  /** Encadré « fournisseur » : le fil du fournisseur de la première pièce lue qui en a un. */
  async function lireContexte(v: FicheMail) {
    const p = v.pieces.find(
      (x) => x.document_etat === 'lu' && x.document_fournisseur_id && (x.document_type === 'facture' || x.document_type === 'avoir' || x.document_type === 'bl')
    );
    if (!p) return;
    try {
      const fil = await filFournisseur(p.document_fournisseur_id!, { societe: societe.id, mois: 1 });
      if (v.id !== id) return;
      contexte = {
        fournisseurId: fil.fournisseur.id,
        nom: fil.fournisseur.nom,
        societe: nomCourtSociete(p.document_societe ?? societe.courante?.raison_sociale) || null,
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

  /** Envoi en un clic (parcours B) ; confirmation seulement s'il reste un « [à compléter] ». */
  function demanderEnvoi() {
    if (!peutEnvoyer || !actif) return;
    if (!aCompleter) {
      void envoyer();
      return;
    }
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

  let meta = $derived.by(() => {
    if (!m) return '';
    const morceaux = [nomExpediteur(m.expediteur) || 'Expéditeur inconnu'];
    const boites = adressesDuMail.join(', ');
    if (boites) morceaux.push(`à ${boites}${societesDuMail.length ? ` (${societesDuMail.join(', ')})` : ''}`);
    if (m.date_mail) morceaux.push(dateMail(m.date_mail));
    return morceaux.join(', ');
  });

  let etatTri = $derived.by(() => {
    if (!m) return '';
    if (m.categorie) return `${libelleCategorie(m.categorie)}${m.categorie_par ? `, ${libelleTriePar(m.categorie_par)}` : ''}`;
    if (m.tri_etat === 'erreur') return 'Tri en erreur';
    if (m.tri_etat === 'a_trier' || m.tri_etat === 'en_cours') return 'Pas encore trié';
    return 'Pas encore trié';
  });

  let boiteBrouillon = $derived(actif?.boite_adresse ?? m?.boites.find((b) => String(b.boite_id) === boiteEnvoi)?.adresse ?? '');
</script>

<div class="volet-h">
  {#if volet}
    <a class="retour tel-seul" href={retour.href}><Icone nom="gauche" />{retour.libelle}</a>
  {:else}
    <a class="retour" href={retour.href}><Icone nom="gauche" />{retour.libelle}<Kbd t="Escape" /></a>
  {/if}
  {#if indiceFile >= 0 && file}
    {#if !volet}<span class="vsep"></span>{/if}
    <span class="pos">À répondre <span class="num">{indiceFile + 1}</span> sur <span class="num">{file.length}</span></span>
  {/if}
  <span class="sp"></span>
  {#if indiceFile >= 0 && file}
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
  <div class="mail-b">
    <!-- ======== le mail ======== -->
    <section class="mail-l" aria-label="Mail reçu">
      {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
      {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}

      <div class="titre-fiche"><h1>{m.objet || '(sans objet)'}</h1></div>
      <div class="meta-fiche">
        {meta}
        {#if m.priorite === 'haute' && aRepondre}<span class="urgent">{libellePriorite(m.priorite)}</span>{/if}
      </div>

      {#if m.resume}
        <div class="resume"><div class="lab">En une phrase</div><p>{m.resume}</p></div>
      {/if}

      <div class="tri">
        {#if aRepondre}
          <span>Mal trié ?</span>
          <button type="button" class="btn sm" class:en-cours={occupe === 'statut:sans_suite'} disabled={!!occupe} onclick={() => changerStatut('sans_suite', 'Classé : pas besoin de réponse.')}>
            {ACTIONS_REPONSE.sansSuite}
          </button>
          <button type="button" class="btn sm" class:en-cours={occupe === 'categorie:information'} disabled={!!occupe} onclick={() => classer('information', 'Classé en Information : le tri s’en souviendra.')}>
            Information
          </button>
          <button type="button" class="btn sm" class:en-cours={occupe === 'categorie:publicite'} disabled={!!occupe} onclick={() => classer('publicite', 'Classé en Publicité : le tri s’en souviendra.')}>
            Publicité
          </button>
          {#if m.categorie === 'question' && m.categorie_par !== 'humain'}
            <span class="sp"></span>
            <button type="button" class="btn sm fantome" class:en-cours={occupe === 'categorie:question'} disabled={!!occupe} onclick={() => classer('question', 'Question confirmée : le tri s’en souviendra.')}>
              Vraie question
            </button>
          {:else if m.categorie_par === 'humain'}
            <span class="sp"></span><span>Question confirmée</span>
          {/if}
        {:else}
          <span>{etatTri}</span>
          {#if m.statut_reponse}
            <Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>
          {/if}
          <span class="sp"></span>
          {#if m.statut_reponse === 'repondu' || m.statut_reponse === 'sans_suite' || (m.categorie === 'question' && !m.statut_reponse)}
            <button type="button" class="btn sm" disabled={!!occupe} class:en-cours={occupe === 'statut:a_repondre'} onclick={() => changerStatut('a_repondre', 'Remis dans À répondre.')}>
              Remettre à répondre
            </button>
          {:else if m.categorie !== 'question'}
            <button type="button" class="btn sm" disabled={!!occupe} class:en-cours={occupe === 'categorie:question'} onclick={() => classer('question', 'Classé en Question : ajouté à À répondre.')}>
              C’est une question
            </button>
          {/if}
        {/if}
      </div>

      <div class="from">
        <span>De</span><span>{m.expediteur || '—'}</span>
        <span>À</span><span>{m.destinataires.join(', ') || '—'}</span>
        <span>Objet</span><span>{m.objet || '(sans objet)'}</span>
        <span>Reçu</span><span>{adressesDuMail.join(', ')}{societesDuMail.length ? `, ${societesDuMail.join(', ')}` : ''}</span>
      </div>

      <div class="mailtext">
        {#each corps.paragraphes as p, i (i)}<p>{p}</p>{:else}<p class="muted">(corps vide)</p>{/each}
        {#if corps.cite}
          <button type="button" class="btn-lien cite-bouton" aria-expanded={citeOuverte} onclick={() => (citeOuverte = !citeOuverte)}>
            {citeOuverte ? 'Masquer le message cité' : 'Afficher le message cité'}
          </button>
          {#if citeOuverte}<div class="cite">{corps.cite}</div>{/if}
        {/if}
      </div>

      {#each m.pieces as p (p.id)}
        {@const e = etatPiece(p)}
        {#if p.document_id}
          <a class="att" href="/documents/{p.document_id}">
            <Icone nom="fichier" />
            <div class="att-t">
              <div class="nom">{p.nom}</div>
              <div class="t2 etat-{e.ton}">{e.texte}</div>
            </div>
            <span class="go">Ouvrir la fiche</span>
          </a>
        {:else}
          <a class="att" href="/api/pieces/{p.id}" download>
            <Icone nom="trombone" />
            <div class="att-t">
              <div class="nom">{p.nom}</div>
              <div class="t2">{e.texte}</div>
            </div>
            <span class="go">Télécharger</span>
          </a>
        {/if}
      {/each}

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
            <span>Message-ID</span><span class="num">{m.message_id}</span>
            {#each m.boites as b (b.boite_id)}<span>Boîte</span><span>{b.adresse}{b.libelle !== b.adresse ? ` (${b.libelle})` : ''}, dossier {b.dossier}</span>{/each}
            {#each m.pieces as p (p.id)}<span>Pièce</span><span>{p.nom} <span class="muted">({p.type_mime}, {taille(p.taille)})</span></span>{/each}
          </div>
        </Repli>
      </div>
    </section>

    <!-- ======== la réponse ======== -->
    <section class="mail-r" aria-label="Réponse">
      {#if actif && editable}
        <div class="rep-h">
          <h2>{actif.statut === 'modifie' || modifieIci ? 'Réponse proposée, modifiée' : 'Réponse proposée'}</h2>
          <span class="muted">à relire</span>
          <span class="sp"></span>
          <button type="button" class="btn sm fantome" disabled={!!occupe} class:en-cours={occupe === 'proposer'} onclick={demanderAutre}>{ACTIONS_REPONSE.autre}</button>
          <button type="button" class="btn sm fantome" disabled={!!occupe} onclick={demanderEffacer}>{ACTIONS_REPONSE.effacer}</button>
        </div>
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
        <div class="rep-f">
          <button type="button" class="btn principal" class:en-cours={occupe === 'envoyer'} disabled={!peutEnvoyer} onclick={demanderEnvoi}>
            {libelleEnvoyerDepuis(actif.boite_adresse)}<Kbd t={['Ctrl', 'Enter']} />
          </button>
          {#if modifieIci}
            <button type="button" class="btn fantome" disabled={!!occupe} class:en-cours={occupe === 'enregistrer'} onclick={enregistrer}>Enregistrer</button>
          {/if}
        </div>
        <p class="petit muted note">Rien ne part sans votre clic. La réponse reste dans le fil du mail reçu.</p>
      {:else if actif && actif.statut === 'generation'}
        <div class="rep-h"><h2>Réponse</h2><span class="muted">{libelleBrouillon('generation')}</span></div>
        <div class="attente" role="status" aria-live="polite">
          <span class="roue" aria-hidden="true"></span>
          <div>
            <div>Claude rédige une réponse dans le ton de vos réponses précédentes.</div>
            <div class="petit muted">
              Depuis <span class="num">{depuisSecondes}</span> s, depuis {actif.boite_adresse}.
              {#if depuisSecondes > 90}C’est plus long que d’habitude : vous pouvez quitter la page, la réponse vous attendra ici.{/if}
            </div>
          </div>
        </div>
      {:else if actif && actif.statut === 'envoi'}
        <div class="rep-h"><h2>Réponse</h2><span class="muted">{libelleBrouillon('envoi')}</span></div>
        <textarea class="editeur" value={actif.texte} disabled aria-label="Texte de la réponse"></textarea>
        <div class="attente" role="status" aria-live="polite">
          <span class="roue" aria-hidden="true"></span>
          <div>
            <div>Envoi depuis {actif.boite_adresse}.</div>
            <div class="petit muted">Depuis <span class="num">{depuisSecondes}</span> s.</div>
          </div>
          {#if depuisSecondes > 20 && etaitDansFile}
            <span class="sp"></span>
            <button type="button" class="btn sm" onclick={passerAuSuivant}>Passer au suivant</button>
          {/if}
        </div>
      {:else}
        <div class="rep-h"><h2>Réponse</h2>{#if aRepondre}<span class="muted">pas encore de réponse</span>{/if}</div>
        <div class="proposer">
          {#if !aRepondre && m.categorie !== 'question'}
            <p class="t2">Pas de réponse attendue pour ce mail.</p>
          {/if}
          {#if m.boites.length > 1}
            <label class="depuis">
              <span>Répondre depuis</span>
              <select bind:value={boiteEnvoi} disabled={!!occupe}>
                {#each m.boites as b (b.boite_id)}<option value={String(b.boite_id)}>{b.adresse}</option>{/each}
              </select>
            </label>
          {/if}
          <div>
            <button
              type="button"
              class="btn"
              class:principal={aRepondre || m.categorie === 'question'}
              class:en-cours={occupe === 'proposer'}
              disabled={!!occupe || !m.boites.length}
              onclick={proposer}
            >
              {ACTIONS_REPONSE.proposer}
            </button>
          </div>
          <p class="petit muted">Claude propose un texte depuis {boiteBrouillon || 'la boîte du mail'} ; vous le relisez, le retouchez et l’envoyez vous-même.</p>
        </div>
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
    </section>
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

<style>
  .charge-erreur {
    padding: var(--e5) var(--e6);
  }
  .mail-b {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    min-height: 0;
  }
  .mail-l {
    overflow: auto;
    padding: var(--e5) var(--e6);
    border-right: 1px solid var(--line);
    min-width: 0;
  }
  .mail-r {
    overflow: auto;
    padding: var(--e5) var(--e6);
    display: flex;
    flex-direction: column;
    background: var(--white);
    min-width: 0;
  }
  .mail-l :global(.bandeau) {
    margin-bottom: var(--e4);
  }
  .urgent {
    margin-left: var(--e2);
    color: var(--err);
    font-weight: 500;
  }
  .resume {
    padding: var(--e3) 14px;
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    margin-bottom: var(--e4);
  }
  .resume .lab {
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .resume p {
    margin: 0;
    font-size: 14px;
    line-height: 21px;
  }
  .tri {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0 0 var(--e5);
    color: var(--muted);
    font-size: 12px;
  }
  .tri .btn {
    color: var(--text2);
  }
  .from {
    display: grid;
    grid-template-columns: 52px 1fr;
    row-gap: 2px;
    column-gap: var(--e2);
    font-size: 12px;
    padding-bottom: var(--e3);
    border-bottom: 1px solid var(--line);
    margin-bottom: var(--e4);
    overflow-wrap: anywhere;
  }
  .from > span:nth-child(odd) {
    color: var(--muted);
  }
  .from.details {
    grid-template-columns: 84px 1fr;
    border-bottom: 0;
    margin-bottom: 0;
  }
  .mailtext {
    font-size: 13px;
    line-height: 21px;
    color: #2a2a28;
    max-width: 62ch;
    overflow-wrap: anywhere;
  }
  .mailtext p {
    margin: 0 0 var(--e3);
    white-space: pre-line;
  }
  .cite-bouton {
    font-size: 12px;
  }
  .cite {
    margin-top: var(--e2);
    padding-left: var(--e3);
    border-left: 2px solid var(--line);
    color: var(--text2);
    white-space: pre-wrap;
    font-size: 12px;
    line-height: 18px;
  }
  .att {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: var(--e2) var(--e3);
    border: 1px solid var(--line);
    border-radius: var(--r2);
    background: var(--white);
    margin-top: var(--e4);
    text-decoration: none;
    color: var(--text);
  }
  .att + .att {
    margin-top: var(--e2);
  }
  .att:hover {
    border-color: var(--line2);
  }
  .att-t {
    min-width: 0;
  }
  .att .nom {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .att .t2 {
    font-size: 12px;
  }
  .att .etat-ok {
    color: var(--ok);
  }
  .att .etat-alerte {
    color: var(--warn);
  }
  .att .etat-erreur {
    color: var(--err);
  }
  .att .go {
    margin-left: auto;
    flex: none;
    color: var(--text2);
    font-size: 12px;
  }
  .ctx {
    margin-top: var(--e4);
    font-size: 12px;
    color: var(--text2);
  }
  .ctx h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }
  .ctx h3 a {
    color: inherit;
  }
  .ctx div {
    display: flex;
    justify-content: space-between;
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
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
  .rep-h {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin-bottom: var(--e3);
  }
  .rep-h h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .rep-h .muted {
    font-size: 12px;
  }
  .mail-r :global(.bandeau) {
    margin-bottom: var(--e3);
  }
  .editeur {
    flex: 1;
    min-height: 360px;
    padding: 14px var(--e4);
    font-size: 13px;
    line-height: 21px;
    resize: none;
  }
  .rep-n {
    margin-top: 10px;
    font-size: 12px;
    color: var(--warn);
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .rep-f {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin-top: var(--e3);
  }
  .note {
    margin: var(--e2) 0 0;
  }
  .attente {
    display: flex;
    align-items: center;
    gap: var(--e3);
    padding: 14px var(--e4);
    margin-top: var(--e3);
    border: 1px solid var(--line);
    border-radius: var(--r2);
    color: var(--text2);
  }
  .attente .roue {
    flex: none;
    width: 14px;
    height: 14px;
    border: 1.5px solid var(--line2);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: tourne 0.8s linear infinite;
  }
  .proposer {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--e3);
    padding: var(--e4) 0;
  }
  .proposer p {
    margin: 0;
    max-width: 52ch;
  }
  .depuis {
    display: flex;
    align-items: center;
    gap: var(--e3);
    color: var(--text2);
  }
  .depuis select {
    width: auto;
    min-width: 240px;
  }
  .envoyes {
    margin-top: var(--e6);
  }
  .texte-envoye {
    white-space: pre-wrap;
    font-size: 13px;
    line-height: 21px;
    color: var(--text2);
  }
  @keyframes tourne {
    to {
      transform: rotate(360deg);
    }
  }

  /* Tablette étroite et téléphone : le mail, puis la réponse. */
  @media (max-width: 960px) {
    .mail-b {
      display: block;
      overflow: auto;
    }
    .mail-l {
      overflow: visible;
      border-right: 0;
      border-bottom: 1px solid var(--line);
    }
    .mail-r {
      overflow: visible;
    }
    .editeur {
      min-height: 280px;
    }
  }
  @media (max-width: 760px) {
    .mail-b {
      overflow: visible;
    }
    .mail-l,
    .mail-r {
      padding: 14px var(--e4) var(--e4);
    }
    .rep-f .btn.principal {
      flex: 1;
      height: 48px;
      white-space: normal;
    }
    .ligne-tri select,
    .depuis select {
      min-width: 0;
      flex: 1;
    }
  }
</style>
