<script lang="ts">
  // Accueil (maquette Sauge P3) : la date et une phrase de synthèse ; ce qui est bloqué (#bloque :
  // boîte, Claude, envoi échoué, doublon, illisible, avec l'action qui débloque) ; la carte « À contrôler »
  // (vignette de la pièce choisie, J/K, Entrée ouvre, E écarte) ; le courrier du jour (avec pièce
  // jointe, pour information, notifications regroupées, publicités masquées) ; à droite les
  // chiffres du jour, ce qui attend une réponse et le réglage qui manque (admins).
  import { goto } from '$app/navigation';
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import { aTraiter, api, ecarterDocument, ErreurApi, message, miseEnRoute, requete, sonder, validerDocument } from '#lib/api.ts';
  import { deplacer, raccourcis } from '#lib/clavier.ts';
  import { compteurs, rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { libelleEtat, libellePriorite, nomCourtSociete, nomOutil } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type {
    Bloque,
    DetailRapprochement,
    DocumentAValider,
    EtapeMiseEnRoute,
    FicheDocument,
    FileATraiter,
    LigneDocument,
    LigneMail,
    MiseEnRoute,
    Page
  } from '#lib/types.ts';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import { age, drapeau, nomCourtDocument, nomExpediteur, quandCourt } from '#lib/ui/DocOutils.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import { formatMontant } from '#lib/ui/Montant.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  /** Pièces montrées dans la carte À contrôler ; au-delà, un lien vers la file. */
  const NB_PIECES = 6;
  /** Mails montrés par groupe du courrier du jour. */
  const NB_MAILS = 8;
  /** Pages de mails du jour lues au plus (50 par page). */
  const PAGES_MAILS = 4;

  interface Tableau {
    mails_du_jour: { societe_id: number; raison_sociale: string; n: number }[];
  }
  interface Donnees {
    file: FileATraiter;
    mailsDuJour: number;
    mails: LigneMail[];
    mailsTotal: number;
    facturesAValider: number;
    blAValider: number;
  }

  let admin = $derived(session.moi?.role === 'admin');
  let d = $state<Donnees | null>(null);
  let erreur = $state('');
  let reglages = $state<MiseEnRoute | null>(null);
  let tour = 0;

  const jourParis = new Intl.DateTimeFormat('fr-CA', { timeZone: 'Europe/Paris', year: 'numeric', month: '2-digit', day: '2-digit' });

  async function mailsDuJour(s: number | null): Promise<{ elements: LigneMail[]; total: number }> {
    const du = jourParis.format(new Date());
    const lire = (page: number) => api.get<Page<LigneMail>>(`/api/mails${requete({ societe: s, categorie: 'hors_publicite', du, page })}`);
    const p1 = await lire(1);
    const elements = [...p1.elements];
    const pages = Math.min(PAGES_MAILS, Math.ceil(p1.total / Math.max(1, p1.elements.length)));
    for (let p = 2; p <= pages && elements.length < p1.total; p++) elements.push(...(await lire(p)).elements);
    return { elements, total: p1.total };
  }

  async function charger() {
    const moi = ++tour;
    const s = societe.id;
    const doc = (p: Record<string, string>) => api.get<Page<LigneDocument>>(`/api/documents${requete({ societe: s, ...p })}`);
    const [file, t, mails, facV, blV] = await Promise.all([
      aTraiter.file(s),
      api.get<Tableau>(`/api/tableau-de-bord${requete({ societe: s })}`),
      mailsDuJour(s),
      doc({ type: 'facture', verifie: 'non' }),
      doc({ type: 'bl', verifie: 'non' })
    ]);
    if (moi !== tour) return;
    d = {
      file,
      mailsDuJour: t.mails_du_jour.reduce((n, l) => n + l.n, 0),
      mails: mails.elements,
      mailsTotal: mails.total,
      facturesAValider: facV.total,
      blAValider: blV.total
    };
    erreur = '';
  }

  const recharger = () => charger().catch((e) => (erreur = message(e)));

  $effect(() => {
    void societe.id;
    untrack(() => {
      d = null;
      recharger();
    });
    return sonder(async () => {
      if (document.visibilityState === 'visible') await recharger();
    }, 60_000);
  });

  // Mise en route (admins) : le réglage qui manque encore.
  $effect(() => {
    if (!admin) {
      reglages = null;
      return;
    }
    untrack(() => {
      miseEnRoute()
        .then((r) => (reglages = r))
        .catch(() => (reglages = null));
    });
  });

  // ---------- Synthèse ----------
  const aujourdhui = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', weekday: 'long', day: 'numeric', month: 'long' }).format(new Date());
  const jour = aujourdhui.charAt(0).toUpperCase() + aujourdhui.slice(1);

  const LETTRES = ['zéro', 'un', 'deux', 'trois', 'quatre', 'cinq', 'six', 'sept', 'huit', 'neuf', 'dix'];
  /** 3 → « trois », 1 → « une » (féminin), 12 → « 12 ». */
  function enLettres(n: number, feminin = false): string {
    if (n === 1 && feminin) return 'une';
    return n <= 10 ? LETTRES[n] : String(n);
  }
  const majuscule = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);

  let synthese = $derived.by(() => {
    if (!d) return '';
    const p = d.file.compteurs.a_valider;
    const r = d.file.compteurs.a_repondre;
    if (p === 0 && r === 0) return 'Tout est à jour : aucune pièce à contrôler, aucun mail en attente de réponse.';
    const pieces =
      p === 0
        ? 'Aucune pièce n’attend votre validation.'
        : `${majuscule(enLettres(p, true))} pièce${p > 1 ? 's attendent' : ' attend'} votre validation.`;
    const mails =
      r === 0 ? 'Aucun mail n’attend de réponse.' : `${majuscule(enLettres(r))} mail${r > 1 ? 's attendent' : ' attend'} une réponse.`;
    return `${pieces} ${mails}`;
  });

  // ---------- À contrôler ----------
  let pieces = $derived(d ? d.file.a_valider.slice(0, NB_PIECES) : []);
  let autres = $derived(d ? d.file.compteurs.a_valider - pieces.length : 0);
  let choixId = $state<number | null>(null);
  let choix = $derived(pieces.find((l) => l.id === choixId) ?? pieces[0] ?? null);
  let indice = $derived(choix ? pieces.indexOf(choix) : -1);

  /** La pièce s'ouvre dans sa liste, filtre « À contrôler » (suivant, précédent, puis la suivante après validation). */
  const listeType = (t: DocumentAValider['type']) => (t === 'bl' ? '/bl' : t === 'avoir' ? '/avoirs' : '/factures');
  const ouvrirHref = (l: DocumentAValider) => `${listeType(l.type)}?filtre=a-valider&doc=${l.id}`;
  /** Au-delà des pièces montrées : un lien par liste qui en a encore. */
  let suiteListes = $derived(
    [
      { href: '/factures?filtre=a-valider', n: compteurs.factures, l: 'facture' },
      { href: '/avoirs?filtre=a-valider', n: compteurs.avoirs, l: 'avoir' },
      { href: '/bl?filtre=a-valider', n: compteurs.bl, l: 'BL' }
    ].filter((x) => x.n > 0)
  );

  // ---------- Bloqué : déblocage en un clic ----------
  let bloques = $derived(d?.file.bloque ?? []);
  let occupeBloque = $state('');
  let erreurBloque = $state('');
  const cleBloque = (b: Bloque) => `${b.genre}-${b.document_id ?? b.boite_id ?? ''}`;

  async function debloquer(b: Bloque) {
    if (occupeBloque || b.document_id === null) return;
    occupeBloque = cleBloque(b);
    erreurBloque = '';
    try {
      if (b.genre === 'envoi_erreur') {
        await validerDocument(b.document_id, {}, societe.id);
        annonce = `Envoi relancé : ${nomCourtDocument(b)}`;
      } else {
        await ecarterDocument(b.document_id, societe.id);
        annonce = `Écarté : ${b.genre === 'illisible' ? (b.libelle ?? 'document illisible') : nomCourtDocument(b)}`;
      }
      void rafraichirCompteurs();
      await charger().catch((e) => (erreur = message(e)));
    } catch (e) {
      erreurBloque = e instanceof ErreurApi && e.code === 409 ? e.message : message(e);
    } finally {
      occupeBloque = '';
    }
  }

  function depuis(iso: string | null): string {
    if (!iso) return '';
    const q = quandCourt(iso);
    return /^\d\d:\d\d$/.test(q) || q === 'hier' ? `depuis ${q}` : `depuis le ${q}`;
  }

  function texteBloque(b: Bloque): { a: string; b: string } {
    const msg = b.message ?? '';
    switch (b.genre) {
      case 'envoi_erreur':
        return { a: `Envoi à Dext / Yooz échoué : ${nomCourtDocument(b)}`, b: msg };
      case 'doublon':
        return { a: `Doublon : ${nomCourtDocument(b)}`, b: msg };
      case 'illisible':
        return { a: `Document illisible : ${b.libelle ?? 'pièce jointe'}`, b: msg };
      case 'boite':
        return {
          a: `Boîte ${b.libelle ?? ''} ${b.motif === 'pause_auto' ? 'arrêtée' : 'en échec'}`,
          b: [msg, depuis(b.depuis)].filter(Boolean).join(' ')
        };
      case 'claude':
        if (b.motif === 'limite' || b.motif === 'jeton')
          return {
            a: msg || 'Claude refuse les appels',
            b:
              b.motif === 'limite'
                ? 'Les documents et les mails attendent : ils repartiront tout seuls.'
                : 'Les documents ne sont plus lus ni les mails triés.'
          };
        return {
          a: b.motif === 'absent' ? 'Claude n’est pas connecté' : 'Connexion à Claude en échec',
          b: msg || 'Les documents ne sont plus lus ni les mails triés'
        };
    }
  }

  // Arrivée par /#bloque (pastille d'état) : la carte n'existe qu'une fois les données chargées.
  let ancreVue = false;
  $effect(() => {
    if (!d || ancreVue || page.url.hash !== '#bloque') return;
    ancreVue = true;
    requestAnimationFrame(() => document.getElementById('bloque')?.scrollIntoView({ block: 'start' }));
  });

  function recu(iso: string): string {
    const a = age(iso);
    if (!a) return '';
    if (/^\d\d:\d\d$/.test(a)) return `reçu à ${a}`;
    return a === 'hier' ? 'reçu hier' : `reçu il y a ${a}`;
  }
  function libellePiece(l: DocumentAValider): string {
    const type = l.type === 'bl' ? 'BL' : l.type === 'avoir' ? 'Avoir' : 'Facture';
    return [l.numero ? `${type} ${l.numero}` : type, l.societe ? nomCourtSociete(l.societe) : '', recu(l.recu_le)].filter(Boolean).join(' · ');
  }

  // Fiche de la pièce choisie (vignette, outil d'envoi) et liens des BL (« En attente de facture »).
  let fiches = $state<Record<number, FicheDocument>>({});
  let liensBl = $state<Record<number, DetailRapprochement['liens'] | null>>({});

  $effect(() => {
    const id = choix?.id;
    if (!id || fiches[id]) return;
    untrack(() => {
      api
        .get<FicheDocument>(`/api/documents/${id}`)
        .then((f) => (fiches[id] = f))
        .catch(() => {});
    });
  });
  $effect(() => {
    const bls = pieces.filter((l) => l.type === 'bl' && !(l.id in liensBl)).map((l) => l.id);
    if (!bls.length) return;
    untrack(() => {
      for (const id of bls) {
        liensBl[id] = null;
        api
          .get<DetailRapprochement>(`/api/documents/${id}/rapprochement`)
          .then((r) => (liensBl[id] = r.liens))
          .catch(() => {});
      }
    });
  });

  function etatBl(l: DocumentAValider): string {
    const liens = liensBl[l.id];
    if (!liens) return '';
    if (!liens.length) return 'En attente de facture';
    const f = liens[0].autre;
    return `Rapproché de ${f.type === 'avoir' ? 'l’avoir' : 'la facture'}${f.numero ? ` ${f.numero}` : ''}`;
  }

  let fiche = $derived(choix ? (fiches[choix.id] ?? null) : null);
  let genreVignette = $derived.by(() => {
    if (!fiche) return 'simple';
    const t = (fiche.type_mime ?? '').toLowerCase();
    // PDF : la 1re page par la visionneuse du navigateur, s'il en a une (sinon vignette simplifiée).
    const pdf = t === 'application/pdf' || t === 'application/x-pdf' || fiche.nom_fichier.toLowerCase().endsWith('.pdf');
    if (pdf) return navigator.pdfViewerEnabled ? 'pdf' : 'simple';
    if (/^image\/(png|jpe?g|gif|webp)$/.test(t)) return 'image';
    return 'simple';
  });

  const telephone = () => matchMedia('(max-width: 760px)').matches;

  /** Premier clic : choisit la pièce (vignette) ; clic sur la pièce choisie, ou téléphone : ouvre. */
  function clicPiece(e: MouseEvent, l: DocumentAValider) {
    if (e.ctrlKey || e.metaKey || e.shiftKey || telephone() || choix?.id === l.id) return;
    e.preventDefault();
    choixId = l.id;
  }

  function deplacerDe(sens: 1 | -1) {
    const i = deplacer(indice, sens, pieces.length);
    if (i >= 0) choixId = pieces[i].id;
  }

  // Écarter : même action et même confirmation que la fiche document (DocFiche).
  let dlgEcarter = $state(false);
  let ecarte = $state<DocumentAValider | null>(null);
  let occupe = $state(false);
  let annonce = $state('');
  let erreurAction = $state('');

  function demanderEcart() {
    if (!choix || occupe) return;
    ecarte = choix;
    if (choix.alerte_code === 'doublon') void ecarter();
    else dlgEcarter = true;
  }

  async function ecarter() {
    const l = ecarte;
    if (!l || occupe) return;
    occupe = true;
    erreurAction = '';
    const avant = pieces.findIndex((p) => p.id === l.id);
    try {
      const r = await ecarterDocument(l.id, societe.id);
      annonce = `Écarté : ${nomCourtDocument(l)}`;
      void rafraichirCompteurs();
      await charger().catch((e) => (erreur = message(e)));
      choixId = r.suivant ?? pieces[Math.min(avant, pieces.length - 1)]?.id ?? null;
    } catch (e) {
      erreurAction = e instanceof ErreurApi && e.code === 409 ? e.message : message(e);
    } finally {
      occupe = false;
    }
  }

  $effect(() => {
    if (!pieces.length) return;
    return raccourcis({
      j: () => deplacerDe(1),
      k: () => deplacerDe(-1),
      Enter: () => {
        if (choix) void goto(ouvrirHref(choix));
      },
      e: () => demanderEcart()
    });
  });

  // ---------- Courrier du jour ----------
  const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
  const heureMail = (iso: string | null) => (iso ? heure.format(new Date(iso)) : '');

  let courrier = $derived.by(() => {
    const avecPiece: LigneMail[] = [];
    const information: LigneMail[] = [];
    const notifs = new Map<string, number>();
    for (const m of d?.mails ?? []) {
      if (m.categorie === 'publicite') continue;
      if (m.categorie === 'notification') {
        const nom = nomExpediteur(m.expediteur) || 'Un expéditeur';
        notifs.set(nom, (notifs.get(nom) ?? 0) + 1);
      } else if (m.nb_pieces > 0 || m.categorie === 'document_seul') avecPiece.push(m);
      else information.push(m);
    }
    const notifications = [...notifs].sort((a, b) => b[1] - a[1]).map(([nom, n]) => ({ nom, n }));
    return {
      groupes: [
        { titre: 'Avec pièce jointe', mails: avecPiece },
        { titre: 'Pour information', mails: information }
      ].filter((g) => g.mails.length),
      notifications
    };
  });
  let mailsNonLus = $derived(d ? d.mailsTotal - d.mails.length : 0);

  const phraseNotif = (nom: string, n: number, seul: boolean) =>
    `${nom} a ${seul ? '' : 'aussi '}envoyé ${n === 1 ? 'une notification automatique' : `${n} notifications automatiques`}`;

  // ---------- Réglage qui manque ----------
  type Manque = { texte: string; action: string; href: string };
  const ORDRE: EtapeMiseEnRoute['cle'][] = ['societes', 'envoi', 'claude', 'boites'];

  function manqueDe(e: EtapeMiseEnRoute): Manque {
    switch (e.cle) {
      case 'societes':
        return { texte: 'Aucune société n’est enregistrée : rien n’est relevé ni envoyé.', action: 'Ajouter une société', href: '/reglages#societes' };
      case 'envoi': {
        const sans = e.manques.map((m) => m.match(/^(.+) : adresse Dext\/Yooz manquante$/)?.[1]).filter((x): x is string => !!x);
        if (sans.length) {
          const noms = sans.length === 1 ? sans[0] : `${sans.slice(0, -1).join(', ')} et ${sans[sans.length - 1]}`;
          return {
            texte:
              sans.length === 1
                ? `${noms} n’a pas d’adresse de collecte Dext / Yooz, ses factures validées ne peuvent pas partir.`
                : `${noms} n’ont pas d’adresse de collecte Dext / Yooz, leurs factures validées ne peuvent pas partir.`,
            action: 'Indiquer l’adresse',
            href: '/reglages#envoi'
          };
        }
        if (e.manques.some((m) => m.includes('essai')))
          return { texte: 'Le dernier essai de l’adresse d’envoi vers Dext / Yooz a échoué : les factures validées risquent de ne pas partir.', action: 'Régler l’envoi', href: '/reglages#envoi' };
        return { texte: 'L’adresse d’envoi vers Dext / Yooz n’est pas réglée : les factures validées ne peuvent pas partir.', action: 'Régler l’envoi', href: '/reglages#envoi' };
      }
      case 'claude':
        return { texte: 'Claude n’est pas connecté : les pièces ne sont plus lues ni les mails triés.', action: 'Connecter Claude', href: '/reglages#claude' };
      default:
        return { texte: `${e.resume} : les mails de ces boîtes n’arrivent pas.`, action: 'Voir les boîtes', href: '/reglages#boites' };
    }
  }

  let manques = $derived.by(() => {
    if (!reglages || reglages.terminee) return [];
    return ORDRE.map((c) => reglages!.etapes.find((e) => e.cle === c)).filter((e): e is EtapeMiseEnRoute => !!e && !e.fait);
  });
  let manque = $derived(manques.length ? manqueDe(manques[0]) : null);
</script>

<svelte:head><title>Accueil · GMFlow</title></svelte:head>

<div class="tableau">
  <header class="tete">
    <h1>{jour}</h1>
    {#if synthese}<p>{synthese}</p>{/if}
  </header>

  {#if bloques.length}
    <section class="probleme bloque" id="bloque" role="alert" aria-labelledby="titre-bloque">
      <div class="b-tete">
        <Icone nom="erreur" />
        <h2 id="titre-bloque">{bloques.length === 1 ? 'Un problème à régler' : `${majuscule(enLettres(bloques.length))} problèmes à régler`}</h2>
      </div>
      {#if erreurBloque}<p class="b-err">{erreurBloque}</p>{/if}
      <ul>
        {#each bloques as b (cleBloque(b))}
          {@const t = texteBloque(b)}
          <li>
            <div class="b-t">
              {#if b.document_id !== null}<a href="/documents/{b.document_id}">{t.a}</a>{:else}<b>{t.a}</b>{/if}
              {#if t.b}<span>{t.b}</span>{/if}
            </div>
            {#if b.genre === 'boite' && b.boite_id !== null}
              <a class="btn-blanc" href="/reglages/boites/{b.boite_id}">Corriger</a>
            {:else if b.genre === 'claude' && b.motif === 'limite'}
              <a class="btn-blanc" href="/reglages#claude">Voir Claude</a>
            {:else if b.genre === 'claude'}
              <a class="btn-blanc" href="/reglages#claude">Se reconnecter</a>
            {:else if b.genre === 'envoi_erreur' && !admin}
              <a class="btn-blanc" href="/documents/{b.document_id}">Voir</a>
            {:else}
              <button type="button" class="btn-blanc" class:en-cours={occupeBloque === cleBloque(b)} disabled={!!occupeBloque} onclick={() => debloquer(b)}>
                {b.genre === 'envoi_erreur' ? 'Renvoyer' : b.genre === 'doublon' ? 'Écarter le doublon' : 'Écarter'}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if erreur}
    <div class="probleme" role="alert">
      <Icone nom="erreur" />
      <p><b>Tableau de bord non chargé.</b> {erreur}</p>
      <button type="button" class="btn-blanc" onclick={recharger}>Réessayer</button>
    </div>
  {/if}

  {#if !d}
    {#if !erreur}<Chargement texte="Chargement du tableau de bord" />{/if}
  {:else}
    <section class="carte-s valider" aria-labelledby="titre-valider">
      <div class="carte-h">
        <h2 id="titre-valider">À contrôler</h2>
        {#if pieces.length > 1}<span class="aide large-seul"><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>{/if}
      </div>

      {#if annonce}<p class="annonce" role="status"><Icone nom="coche" />{annonce}</p>{/if}
      {#if erreurAction}<p class="annonce ko" role="alert"><Icone nom="alerte" />{erreurAction}</p>{/if}

      {#if !pieces.length}
        <Vide compact icone="coche" titre="Tout est validé" texte="Les factures, avoirs et BL reçus arrivent ici pour être vérifiés avant l’envoi.">
          <a class="btn" href="/factures">Voir les factures</a>
        </Vide>
      {:else}
        <div class="valider-corps">
          {#if choix}
            <div class="choisie">
              <a class="vignette" href={ouvrirHref(choix)} tabindex="-1" aria-hidden="true">
                {#if genreVignette === 'pdf' && fiche}
                  <iframe src="/api/pieces/{fiche.piece_id}?inline=1#toolbar=0&navpanes=0&scrollbar=0&view=FitH" title="" tabindex="-1" loading="lazy"></iframe>
                {:else if genreVignette === 'image' && fiche}
                  <img src="/api/pieces/{fiche.piece_id}?inline=1" alt="" />
                {:else}
                  <div class="feuille">
                    <div class="f-h">
                      <span class="f-nom">{choix.fournisseur ?? 'Fournisseur'}</span>
                      <span class="f-type"><b>{choix.type === 'bl' ? 'BON DE LIVRAISON' : choix.type === 'avoir' ? 'AVOIR' : 'FACTURE'}</b>{#if choix.numero}<i>{choix.numero}</i>{/if}</span>
                    </div>
                    <span class="f-bande"></span>
                    {#each [78, 64, 82, 58, 74, 68, 80, 60, 72] as w, i (i)}<span class="f-l"><i style="width:{w}%"></i><i></i></span>{/each}
                    <span class="sp"></span>
                    <span class="f-bande fine"></span>
                    {#if choix.montant_ttc !== null}<span class="f-total"><span>Total TTC</span><b>{formatMontant(choix.montant_ttc)}</b></span>{/if}
                  </div>
                {/if}
              </a>
              <div class="boutons">
                <a class="btn principal" href={ouvrirHref(choix)}>Ouvrir <Kbd t="Enter" /></a>
                <button type="button" class="btn" class:en-cours={occupe} disabled={occupe} onclick={demanderEcart}>Écarter <Kbd t="E" /></button>
              </div>
            </div>
          {/if}

          <div class="pieces">
            {#each pieces as l (l.id)}
              {@const f = drapeau(l)}
              <a
                class="piece"
                class:cur={choix?.id === l.id}
                aria-current={choix?.id === l.id ? 'true' : undefined}
                href={ouvrirHref(l)}
                onclick={(e) => clicPiece(e, l)}
              >
                <span class="p-h">
                  <span class="p-nom">{l.fournisseur ?? 'Fournisseur non reconnu'}</span>
                  <span class="p-montant num">{l.montant_ttc !== null ? formatMontant(l.montant_ttc) : l.type === 'bl' ? '— HT' : '—'}</span>
                </span>
                <span class="p-b">{libellePiece(l)}</span>
                {#if f}
                  <span class="p-alerte" class:e={f.ton === 'e'}><Icone nom="alerte" />{f.texte}</span>
                {:else if l.type === 'bl' && etatBl(l)}
                  <span class="p-b">{etatBl(l)}</span>
                {/if}
                {#if l.articles_ecart || (l.remise_ht && l.remise_ht > 0.005)}
                  <span class="p-b">
                    {#if l.articles_ecart}<span class="flag w">{l.articles_ecart} écart{l.articles_ecart > 1 ? 's' : ''} BL</span>{/if}
                    {#if l.remise_ht && l.remise_ht > 0.005}{' '}<span class="flag i">Remise {formatMontant(l.remise_ht)}</span>{/if}
                  </span>
                {/if}
              </a>
            {/each}
            {#if autres > 0}
              <p class="plus-l">
                {autres === 1 ? 'Une autre pièce à contrôler.' : `${majuscule(enLettres(autres, true))} autres pièces à contrôler.`} Tout voir :
                {#each suiteListes as x, i (x.href)}{i ? ', ' : ''}<a href={x.href}>{x.n} {x.l}{x.n > 1 && x.l !== 'BL' ? 's' : ''}</a>{/each}
              </p>
            {/if}
          </div>
        </div>
      {/if}
    </section>

    <div class="bas">
      <section class="carte-s courrier" aria-labelledby="titre-courrier">
        <div class="carte-h">
          <h2 id="titre-courrier">Courrier du jour</h2>
          <a class="lien" href="/mails">Tous les mails</a>
        </div>
        {#if !courrier.groupes.length && !courrier.notifications.length}
          <Vide compact icone="mail" titre="Aucun mail reçu aujourd’hui" texte="Les mails des boîtes relevées arrivent ici ; les publicités sont rangées à part." />
        {:else}
          {#each courrier.groupes as g (g.titre)}
            <div class="groupe">
              <h3>{g.titre} <span>{g.mails.length}</span></h3>
              {#each g.mails.slice(0, NB_MAILS) as m (m.id)}
                <a class="mail" href="/mails?mail={m.id}">
                  <span class="m-heure num">{heureMail(m.date_mail)}</span>
                  <span class="m-corps">
                    <span class="m-de">
                      <span class="m-nom">{nomExpediteur(m.expediteur) || '(expéditeur inconnu)'}</span>
                      {#if m.statut_reponse === 'a_repondre'}<span class="etiquette att">À répondre</span>{/if}
                    </span>
                    <span class="m-objet">{m.objet || '(sans objet)'}</span>
                  </span>
                </a>
              {/each}
              {#if g.mails.length > NB_MAILS}
                <a class="mail plus-m" href="/mails">{g.mails.length - NB_MAILS} autre{g.mails.length - NB_MAILS > 1 ? 's' : ''}, dans Mails</a>
              {/if}
            </div>
          {/each}
          {#if courrier.notifications.length || mailsNonLus > 0}
            <div class="notifs">
              {#each courrier.notifications as n (n.nom)}
                <p>{phraseNotif(n.nom, n.n, !courrier.groupes.length)}</p>
              {/each}
              {#if mailsNonLus > 0}<p><a href="/mails">{mailsNonLus} autres mails aujourd’hui, dans Mails</a></p>{/if}
            </div>
          {/if}
        {/if}
      </section>

      <div class="droite">
        <section class="carte-s jour" aria-labelledby="titre-jour">
          <h2 id="titre-jour">Aujourd’hui</h2>
          <a class="chiffre" href="/mails"><span>Mails reçus</span><b class="num">{d.mailsDuJour}</b></a>
          <a class="chiffre" href="/a-repondre"><span>À répondre</span><b class="num" class:att={d.file.compteurs.a_repondre > 0}>{d.file.compteurs.a_repondre}</b></a>
          <a class="chiffre" href="/factures?filtre=a-valider"><span>Factures à contrôler</span><b class="num" class:att={d.facturesAValider > 0}>{d.facturesAValider}</b></a>
          <a class="chiffre" href="/bl?filtre=a-valider"><span>BL à contrôler</span><b class="num" class:att={d.blAValider > 0}>{d.blAValider}</b></a>
        </section>

        {#if d.file.a_repondre.length}
          <section class="carte-s repondre" aria-labelledby="titre-repondre">
            <div class="carte-h">
              <h2 id="titre-repondre">À répondre</h2>
              <a class="lien" href="/a-repondre">Tout voir</a>
            </div>
            {#each d.file.a_repondre.slice(0, 5) as m (m.id)}
              <a class="mail" href="/a-repondre?mail={m.id}">
                <span class="m-corps">
                  <span class="m-de">
                    <span class="m-nom">{m.fournisseur ?? (nomExpediteur(m.expediteur) || '(expéditeur inconnu)')}</span>
                    {#if libellePriorite(m.priorite)}<span class="etiquette ko">{libellePriorite(m.priorite)}</span>{/if}
                  </span>
                  <span class="m-objet">{m.resume || m.objet || '(sans objet)'}</span>
                </span>
              </a>
            {/each}
          </section>
        {/if}

        {#if manque}
          <section class="reglage" aria-labelledby="titre-reglage">
            <div class="r-h">
              <Icone nom="alerte" />
              <h2 id="titre-reglage">{manques.length > 1 ? `${majuscule(enLettres(manques.length))} réglages manquent` : 'Un réglage manque'}</h2>
            </div>
            <p>{manque.texte}</p>
            <div class="r-actions">
              <a class="btn-blanc" href={manque.href}>{manque.action}</a>
              {#if manques.length > 1}<a class="r-tout" href="/reglages#mise-en-route">Voir la mise en route</a>{/if}
            </div>
          </section>
        {/if}
      </div>
    </div>
  {/if}
</div>

{#if ecarte}
  {@const outil = fiches[ecarte.id]?.societe_outil_depot ?? null}
  <Confirmation bind:ouvert={dlgEcarter} titre="Écarter ce document ?" libelle="Écarter" danger onconfirmer={ecarter}>
    <p>
      {nomCourtDocument(ecarte)} sera marqué « {libelleEtat('ignore')} » : {ecarte.type !== 'bl' ? `pas d’envoi à ${nomOutil(outil)}, ` : ''}il quitte la file. Il reste
      visible dans la recherche.
    </p>
  </Confirmation>
{/if}

<style>
  .tableau {
    max-width: 1120px;
    padding: var(--e10) var(--e10) 48px;
    display: flex;
    flex-direction: column;
    gap: var(--e8);
  }
  .tete {
    display: flex;
    flex-direction: column;
    gap: var(--e2);
  }
  .tete h1 {
    font-size: 32px;
    font-weight: 500;
    line-height: 1.15;
  }
  .tete p {
    margin: 0;
    font-size: 19px;
    color: var(--text2);
    text-wrap: pretty;
  }

  /* cartes blanches (rayon 16, anneau #DDE2DA) */
  .carte-s {
    background: var(--white);
    border-radius: var(--r-carte);
    padding: var(--e6);
    box-shadow: var(--ombre-carte);
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .valider {
    gap: var(--e4);
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.04),
      0 0 0 1px var(--line);
  }
  .carte-s h2 {
    font-size: 22px;
    font-weight: 500;
    line-height: 1.3;
  }
  .carte-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--e4);
    flex-wrap: wrap;
  }
  .aide {
    display: flex;
    align-items: center;
    gap: var(--e1);
    font-size: 15px;
    color: var(--muted);
  }
  .lien {
    font-size: 16px;
    font-weight: 500;
    color: var(--accent);
    text-decoration: none;
  }
  .lien:hover {
    color: var(--accent-ink);
    text-decoration: underline;
  }
  .annonce {
    display: flex;
    align-items: center;
    gap: var(--e2);
    margin: 0;
    padding: 10px 14px;
    border-radius: var(--r2);
    background: var(--ok-bg);
    color: var(--ok);
    font-size: 15px;
  }
  .annonce.ko {
    background: var(--err-bg);
    color: var(--err-ink);
  }

  /* ---------- À contrôler ---------- */
  .valider-corps {
    display: flex;
    gap: var(--e6);
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .choisie {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    align-items: stretch;
    width: 214px;
  }
  .vignette {
    position: relative;
    display: block;
    width: 208px;
    height: 294px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--white);
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.07),
      0 1px 2px rgba(0, 0, 0, 0.05),
      0 8px 24px rgba(0, 0, 0, 0.07);
    text-decoration: none;
  }
  .vignette iframe {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
    pointer-events: none;
    background: var(--white);
  }
  .vignette img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top;
    display: block;
  }
  /* vignette simplifiée (pas de PDF ni d'image, ou fiche pas encore chargée) */
  .feuille {
    height: 100%;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-family: Arial, Helvetica, sans-serif;
    color: #1a1a1a;
  }
  .f-h {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 8px;
  }
  .f-nom {
    font-size: 12px;
    font-weight: 700;
    color: var(--accent);
    line-height: 1.2;
    max-width: 50%;
    overflow-wrap: anywhere;
  }
  .f-type {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    font-size: 10px;
    line-height: 1;
  }
  .f-type b {
    border: 1px solid #1a1a1a;
    padding: 2px 4px;
  }
  .f-type i {
    font-style: normal;
    color: #444;
  }
  .f-bande {
    height: 6px;
    background: #cfe3d6;
    flex: none;
  }
  .f-bande.fine {
    height: 4px;
  }
  .f-l {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .f-l i {
    height: 4px;
    border-radius: 1px;
    background: #dadde2;
  }
  .f-l i + i {
    width: 14%;
    background: #c9cdd3;
  }
  .f-total {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 11px;
    line-height: 1;
  }
  .f-total span {
    color: #444;
  }
  .boutons {
    display: flex;
    gap: var(--e2);
  }
  .boutons .btn {
    height: 44px;
    padding: 0 12px;
    font-size: 16px;
  }
  .boutons .btn.principal {
    flex: 1;
    padding: 0 16px;
  }

  .pieces {
    flex: 1 1 320px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--e1);
  }
  .piece {
    display: flex;
    flex-direction: column;
    gap: var(--e1);
    padding: var(--e4);
    border-radius: var(--r2);
    color: var(--text);
    text-decoration: none;
    transition: background-color var(--vite) var(--ease);
  }
  .piece:hover {
    background: var(--pane);
  }
  .piece.cur {
    background: var(--sunk);
  }
  .piece:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 0;
  }
  .p-h {
    display: flex;
    justify-content: space-between;
    gap: var(--e4);
  }
  .p-nom,
  .p-montant {
    font-size: 17px;
    font-weight: 500;
  }
  .p-nom {
    text-transform: uppercase;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .p-montant {
    flex: none;
    white-space: nowrap;
  }
  .p-b {
    font-size: 15px;
    color: var(--text2);
  }
  .p-alerte {
    display: flex;
    align-items: center;
    gap: var(--e2);
    font-size: 15px;
    color: var(--warn);
  }
  .p-alerte.e {
    color: var(--err);
  }
  .p-alerte :global(svg.i) {
    width: 16px;
    height: 16px;
  }
  .plus-l {
    margin: 0;
    padding: var(--e2) var(--e4);
    font-size: 15px;
    color: var(--text2);
  }
  .plus-l a {
    font-weight: 500;
    color: var(--accent);
    text-decoration: none;
  }
  .plus-l a:hover {
    text-decoration: underline;
  }

  /* ---------- Bas : courrier + colonne droite ---------- */
  .bas {
    display: flex;
    flex-wrap: wrap;
    gap: var(--e8);
    align-items: flex-start;
  }
  .courrier {
    flex: 999 1 460px;
    gap: var(--e6);
  }
  .droite {
    flex: 1 1 280px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--e8);
  }
  .groupe {
    display: flex;
    flex-direction: column;
  }
  .groupe h3 {
    margin: 0 0 var(--e2);
    font-size: 15px;
    font-weight: 500;
    color: var(--text2);
  }
  .groupe h3 span {
    font-weight: 400;
    color: var(--muted);
  }
  .mail {
    display: flex;
    gap: var(--e4);
    padding: var(--e3) 0;
    border-top: 1px solid var(--line-faint);
    color: var(--text);
    text-decoration: none;
  }
  .mail:hover .m-nom {
    color: var(--accent-ink);
    text-decoration: underline;
  }
  .m-heure {
    width: 48px;
    flex: none;
    font-size: 15px;
    color: var(--text2);
  }
  .m-corps {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--e1);
  }
  .m-de {
    display: flex;
    align-items: center;
    gap: var(--e2);
    min-width: 0;
  }
  .m-nom {
    font-size: 16px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .m-objet {
    font-size: 16px;
    color: var(--text2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .plus-m {
    font-size: 15px;
    font-weight: 500;
    color: var(--accent);
  }
  .notifs p {
    margin: 0;
    padding: var(--e3) 0 0;
    border-top: 1px solid var(--line-faint);
    font-size: 15px;
    color: var(--text2);
  }
  .notifs p + p {
    margin-top: var(--e3);
  }
  .notifs a {
    color: var(--accent);
    font-weight: 500;
    text-decoration: none;
  }

  .jour h2 {
    margin-bottom: var(--e2);
  }
  .chiffre {
    display: flex;
    justify-content: space-between;
    gap: var(--e3);
    padding: var(--e3) 0;
    border-bottom: 1px solid var(--line-faint);
    color: var(--text2);
    text-decoration: none;
  }
  .chiffre:last-child {
    border-bottom: 0;
  }
  .chiffre:hover span {
    color: var(--text);
    text-decoration: underline;
  }
  .chiffre b {
    font-weight: 500;
    color: var(--text);
  }
  .chiffre b.att {
    color: var(--warn);
  }
  .repondre {
    gap: var(--e2);
  }
  .repondre .carte-h {
    margin-bottom: var(--e2);
  }

  /* encadré ambre : un réglage manque */
  .reglage {
    background: var(--warn-bg);
    border-radius: var(--r-carte);
    padding: var(--e6);
    color: var(--warn);
    display: flex;
    flex-direction: column;
    gap: var(--e3);
  }
  .r-h {
    display: flex;
    align-items: center;
    gap: var(--e2);
  }
  .reglage h2 {
    font-size: 18px;
    font-weight: 500;
  }
  .reglage p {
    margin: 0;
    font-size: 16px;
    text-wrap: pretty;
  }
  .r-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--e3) var(--e4);
  }
  .r-tout {
    color: var(--warn);
    font-size: 15px;
  }
  .btn-blanc {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    height: 40px;
    padding: 0 16px;
    border: 0;
    border-radius: var(--r2);
    background: var(--white);
    color: inherit;
    font: inherit;
    font-size: 16px;
    font-weight: 500;
    text-decoration: none;
    cursor: pointer;
    flex: none;
  }
  .btn-blanc:hover {
    box-shadow: 0 0 0 1px currentColor;
  }

  /* bandeau de problème (boîte, Claude, envoi bloqués) ou de chargement */
  .probleme {
    display: flex;
    align-items: center;
    gap: var(--e3);
    padding: var(--e4) var(--e4) var(--e4) var(--e6);
    border-radius: var(--r-carte);
    background: var(--err-bg);
    color: var(--err);
  }
  .probleme p {
    flex: 1;
    min-width: 0;
    margin: 0;
    color: var(--text);
  }
  .probleme b {
    font-weight: 500;
    color: var(--err);
  }
  .bloque {
    scroll-margin-top: var(--e6);
    flex-direction: column;
    align-items: stretch;
    padding: var(--e4) var(--e6);
  }
  .b-tete {
    display: flex;
    align-items: center;
    gap: var(--e3);
  }
  .b-tete h2 {
    margin: 0;
    font-size: inherit;
    font-weight: 600;
    color: var(--err);
  }
  .b-err {
    margin: 0;
  }
  .bloque ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .bloque li {
    display: flex;
    align-items: center;
    gap: var(--e4);
    padding: var(--e3) 0;
    border-top: 1px solid color-mix(in srgb, var(--err) 18%, transparent);
  }
  .b-t {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    color: var(--text);
  }
  .b-t a,
  .b-t b {
    font-weight: 500;
    color: var(--text);
  }
  .b-t span {
    color: var(--text2);
    font-size: 14px;
  }

  /* < 1024 px : colonnes empilées */
  @media (max-width: 1023px) {
    .tableau {
      padding: var(--e8) var(--e6) var(--e10);
      gap: var(--e6);
    }
    .bas {
      flex-direction: column;
      align-items: stretch;
      gap: var(--e6);
    }
    .courrier,
    .droite {
      flex: none;
      width: 100%;
    }
    .droite {
      gap: var(--e6);
    }
  }

  /* téléphone : pas de vignette, chaque pièce s'ouvre au toucher */
  @media (max-width: 760px) {
    .tableau {
      padding: var(--e6) var(--e4) var(--e8);
    }
    .tete h1 {
      font-size: 26px;
    }
    .tete p {
      font-size: 17px;
    }
    .carte-s,
    .reglage {
      padding: var(--e5) var(--e4);
    }
    .choisie {
      display: none;
    }
    .piece {
      padding: var(--e3);
      margin: 0 calc(-1 * var(--e1));
    }
    .piece.cur {
      background: none;
    }
    .p-nom,
    .p-montant {
      font-size: 16px;
    }
    .probleme {
      padding: var(--e3) var(--e3) var(--e3) var(--e4);
      flex-wrap: wrap;
    }
  }
</style>
