<script lang="ts">
  // À traiter (maquette B, écran 1) : la file à gauche (Bloqué, À valider, À répondre), l'élément
  // ouvert à droite (?doc=ID : fiche document ; ?mail=ID : aperçu du mail). J/K, Entrée, Échap.
  // Téléphone : la liste, puis l'élément en page (Volets).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { aTraiter, api, ecarterDocument, ErreurApi, message, sonder, validerDocument } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { libelleBrouillon, libellePriorite, libelleType, nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Bloque, FicheMail, FileATraiter } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import BarreAction from '#lib/ui/BarreAction.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import DocFiche, { type PositionFiche, type ResultatFiche } from '#lib/ui/DocFiche.svelte';
  import { age, drapeau, jourHeure, nomCourtDocument, nomExpediteur, quandCourt } from '#lib/ui/DocOutils.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import Montant from '#lib/ui/Montant.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import Volets from '#lib/ui/Volets.svelte';

  let admin = $derived(session.moi?.role === 'admin');

  // ---------- File ----------
  let file = $state<FileATraiter | null>(null);
  let erreurFile = $state('');
  let majA = $state<Date | null>(null);
  let erreur = $state('');
  let annonce = $state('');
  let tour = 0;

  async function chargerFile() {
    const moi = ++tour;
    const f = await aTraiter.file(societe.id);
    if (moi !== tour) return;
    file = f;
    majA = new Date();
    erreurFile = '';
  }

  // Au changement de société, puis toutes les minutes (page visible).
  $effect(() => {
    void societe.id;
    untrack(() => {
      file = null;
      chargerFile().catch((e) => (erreurFile = message(e)));
    });
    const arret = sonder(async () => {
      if (document.visibilityState === 'visible') await chargerFile();
    }, 60_000);
    return arret;
  });

  // ---------- Élément ouvert (URL) ----------
  let docId = $derived(Number(page.url.searchParams.get('doc')) || null);
  let mailId = $derived(docId ? null : Number(page.url.searchParams.get('mail')) || null);
  let toutValide = $derived(page.url.searchParams.get('fait') === 'valide');

  type Element = { cle: string; href: string; section: 'bloque' | 'a_valider' | 'a_repondre' };
  /** Lignes qu'on ouvre (J/K), dans l'ordre affiché. */
  let elements = $derived.by((): Element[] => {
    if (!file) return [];
    return [
      ...file.bloque
        .filter((b) => b.document_id !== null)
        .map((b) => ({ cle: `d${b.document_id}`, href: `/?doc=${b.document_id}`, section: 'bloque' as const })),
      ...file.a_valider.map((l) => ({ cle: `d${l.id}`, href: `/?doc=${l.id}`, section: 'a_valider' as const })),
      ...file.a_repondre.map((m) => ({ cle: `m${m.id}`, href: `/?mail=${m.id}`, section: 'a_repondre' as const }))
    ];
  });
  let cleCourante = $derived(docId ? `d${docId}` : mailId ? `m${mailId}` : '');
  let indice = $derived(elements.findIndex((e) => e.cle === cleCourante));
  let courant = $derived(indice >= 0 ? elements[indice] : null);

  // La phrase laissée par une action reste affichée sur l'élément ouvert juste après, pas au-delà.
  let annoncePour = '';
  function annoncer(texte: string, href: string) {
    annonce = texte;
    const u = new URL(href, location.origin);
    const d = Number(u.searchParams.get('doc')) || null;
    const m = Number(u.searchParams.get('mail')) || null;
    annoncePour = d ? `d${d}` : m ? `m${m}` : '';
  }
  $effect(() => {
    if (cleCourante !== annoncePour) annonce = '';
  });

  let listeEl = $state<HTMLElement>();
  function deplacerDe(sens: 1 | -1) {
    const i = deplacer(indice, sens, elements.length);
    if (i < 0) return;
    const cle = elements[i].cle;
    void goto(elements[i].href, { replace: true, reset: false }).then(() => montrer(listeEl, `[data-cle="${cle}"]`));
  }
  function fermer() {
    void goto('/', { reset: false });
  }
  function commencer() {
    const s = file?.suivant ?? file?.a_valider[0]?.id;
    if (s) void goto(`/?doc=${s}`, { reset: false });
    else if (elements[0]) void goto(elements[0].href, { reset: false });
  }

  $effect(() => {
    const ouvert = !!(docId || mailId);
    const carte: Record<string, () => void> = {
      j: () => deplacerDe(1),
      k: () => deplacerDe(-1)
    };
    if (ouvert) carte.Escape = fermer;
    // Entrée sur un document ouvert : la fiche valide ; sur un mail : « Ouvrir et répondre ».
    if (!ouvert) carte.Enter = commencer;
    else if (mailId) carte.Enter = () => void goto(`/mails/${mailId}`);
    return raccourcis(carte);
  });

  // Position et « Ensuite » de l'élément ouvert.
  const SECTIONS = { bloque: 'Bloqué', a_valider: 'À valider', a_repondre: 'À répondre' } as const;
  let position = $derived.by((): PositionFiche | null => {
    if (!file || !courant) return null;
    const de = elements.filter((e) => e.section === courant!.section);
    const total =
      courant.section === 'a_valider' ? file.compteurs.a_valider : courant.section === 'a_repondre' ? file.compteurs.a_repondre : de.length;
    return { section: SECTIONS[courant.section], n: de.indexOf(courant) + 1, total };
  });
  let ensuite = $derived.by(() => {
    if (!file || !courant) return undefined;
    if (courant.section === 'a_repondre') {
      const i = file.a_repondre.findIndex((m) => m.id === mailId);
      const m = file.a_repondre[i + 1];
      return m ? nomExpediteur(m.expediteur) : undefined;
    }
    if (courant.section === 'a_valider') {
      const i = file.a_valider.findIndex((l) => l.id === docId);
      const l = file.a_valider[i + 1] ?? (i > 0 ? file.a_valider[0] : undefined);
      return l ? nomCourtDocument(l) : undefined;
    }
    const docs = file.bloque.filter((b) => b.document_id !== null);
    const i = docs.findIndex((b) => b.document_id === docId);
    const b = docs[i + 1];
    return b ? (b.fournisseur || b.numero ? nomCourtDocument(b) : (b.libelle ?? 'Document')) : undefined;
  });

  /** Après une validation ou un écart : le suivant de la même section, sinon la file. */
  async function apres(r: ResultatFiche) {
    const section = courant?.section ?? 'a_valider';
    const avant = elements.filter((e) => e.section === section).findIndex((e) => e.cle === `d${r.id}`);
    await chargerFile().catch((e) => (erreurFile = message(e)));
    let cible = '/';
    let texte = r.info;
    if (section === 'a_valider') {
      if (r.suivant) cible = `/?doc=${r.suivant}`;
      else texte = `${r.info}. Tout est validé`;
    } else {
      const reste = elements.filter((e) => e.section === section);
      const e = reste[Math.max(0, avant)] ?? reste[reste.length - 1];
      if (e) cible = e.href;
      else if (file?.suivant) cible = `/?doc=${file.suivant}`;
    }
    annoncer(texte, cible);
    await goto(cible, { replace: true, reset: false });
  }

  // ---------- Bloqué : déblocage en un clic ----------
  let occupe = $state('');
  const cleBloque = (b: Bloque) => `${b.genre}-${b.document_id ?? b.boite_id ?? ''}`;

  async function debloquer(b: Bloque) {
    if (occupe || b.document_id === null) return;
    occupe = cleBloque(b);
    erreur = '';
    try {
      if (b.genre === 'envoi_erreur') {
        await validerDocument(b.document_id, {}, societe.id);
        annoncer(`Envoi relancé : ${nomCourtDocument(b)}`, '/');
      } else {
        await ecarterDocument(b.document_id, societe.id);
        annoncer(`Écarté : ${b.genre === 'illisible' ? (b.libelle ?? 'document illisible') : nomCourtDocument(b)}`, '/');
      }
      void rafraichirCompteurs();
      await chargerFile();
      if (docId) await goto('/', { replace: true, reset: false });
    } catch (e) {
      erreur = e instanceof ErreurApi && e.code === 409 ? e.message : message(e);
    } finally {
      occupe = '';
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
        return {
          a: b.motif === 'absent' ? 'Claude n’est pas connecté' : 'Connexion à Claude en échec',
          b: msg || 'Les documents ne sont plus lus ni les mails triés'
        };
    }
  }

  // ---------- Mail ouvert : aperçu court ----------
  let mail = $state<FicheMail | null>(null);
  let erreurMail = $state('');
  $effect(() => {
    const ident = mailId;
    mail = null;
    erreurMail = '';
    if (!ident) return;
    let vivant = true;
    api
      .get<FicheMail>(`/api/mails/${ident}`)
      .then((m) => vivant && (mail = m))
      .catch((e) => vivant && (erreurMail = message(e)));
    return () => {
      vivant = false;
    };
  });
  let ligneMail = $derived(file?.a_repondre.find((m) => m.id === mailId) ?? null);
  let extrait = $derived.by(() => {
    const t = (mail?.corps_texte ?? '').replace(/\r/g, '').replace(/\n{3,}/g, '\n\n').trim();
    return t.length > 700 ? `${t.slice(0, 700).replace(/\s+\S*$/, '')}…` : t;
  });

  /** « Demande le RIB » après « Sodistra : » devient « demande le RIB » (sauf sigle). */
  const minuscule = (t: string) => (/^\p{Lu}\p{Ll}/u.test(t) ? t.charAt(0).toLowerCase() + t.slice(1) : t);
  const majuscule = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);

  const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
  let horsLimite = $derived(file ? file.compteurs.a_valider - file.a_valider.length : 0);
  let ouvert = $derived(!!(docId || mailId));
</script>

<svelte:head><title>À traiter · GMFlow</title></svelte:head>

<Volets {ouvert} libelleListe="File À traiter">
  {#snippet liste()}
    <div class="liste-h">
      <h1>À traiter</h1>
      {#if file}<span class="num">{file.compteurs.total}</span>{/if}
      <span class="sp"></span>
      {#if majA}<span class="muted petit">Mis à jour à {heure.format(majA)}</span>{/if}
    </div>
    <div class="liste-s" bind:this={listeEl}>
      {#if toutValide && !annonce}<div class="doc-annonce"><Bandeau ton="ok" role="status">Tout est validé</Bandeau></div>{/if}
      {#if annonce && !ouvert}<div class="doc-annonce"><Bandeau ton="ok" role="status">{annonce}</Bandeau></div>{/if}
      {#if erreur}<div class="doc-annonce"><Bandeau ton="erreur">{erreur}</Bandeau></div>{/if}
      {#if erreurFile && !file}
        <div class="doc-annonce">
          <Bandeau ton="erreur">
            {erreurFile}
            {#snippet action()}<button type="button" class="btn sm" onclick={() => chargerFile().catch((e) => (erreurFile = message(e)))}>Réessayer</button>{/snippet}
          </Bandeau>
        </div>
      {:else if !file}
        <Chargement texte="Chargement de la file" />
      {:else if file.compteurs.total === 0}
        <Vide icone="coche" titre="Tout est à jour" texte="Aucun document à valider ni mail à répondre." compact />
      {:else}
        {#if file.bloque.length}
          <section class="sec" id="bloque">
            <div class="sec-h erreur"><span class="point"></span><h2>Bloqué</h2><span class="num">{file.compteurs.bloque}</span></div>
            {#each file.bloque as b (cleBloque(b))}
              {@const t = texteBloque(b)}
              {@const cle = b.document_id !== null ? `d${b.document_id}` : ''}
              <div class="ligne bloc doc-bloc" class:cur={cle !== '' && cle === cleCourante} data-cle={cle || undefined}>
                <div class="a">
                  {#if b.document_id !== null}<a class="doc-lien" href="/?doc={b.document_id}">{t.a}</a>{:else}{t.a}{/if}
                </div>
                {#if t.b}<div class="b">{t.b}</div>{/if}
                <div class="r">
                  {#if b.genre === 'boite' && b.boite_id !== null}
                    <a class="btn sm" href="/reglages/boites/{b.boite_id}">Corriger</a>
                  {:else if b.genre === 'claude'}
                    <a class="btn sm" href="/reglages#claude">Se reconnecter</a>
                  {:else if b.genre === 'envoi_erreur'}
                    {#if admin}
                      <button type="button" class="btn sm" class:en-cours={occupe === cleBloque(b)} disabled={!!occupe} onclick={() => debloquer(b)}>Renvoyer</button>
                    {:else}
                      <a class="btn sm" href="/?doc={b.document_id}">Voir</a>
                    {/if}
                  {:else}
                    <button type="button" class="btn sm" class:en-cours={occupe === cleBloque(b)} disabled={!!occupe} onclick={() => debloquer(b)}>
                      {b.genre === 'doublon' ? 'Écarter le doublon' : 'Écarter'}
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </section>
        {/if}

        <section class="sec">
          <div class="sec-h">
            <h2>À valider</h2><span class="num">{file.compteurs.a_valider}</span>
            {#if file.a_valider.length}
              <button type="button" class="btn sm" onclick={commencer}>Commencer{#if !ouvert}<Kbd t="Enter" />{/if}</button>
            {/if}
          </div>
          {#each file.a_valider as l (l.id)}
            {@const f = drapeau(l)}
            <a class="ligne" class:cur={docId === l.id} aria-current={docId === l.id ? 'true' : undefined} href="/?doc={l.id}" data-cle="d{l.id}">
              <div class="a">{l.fournisseur ?? 'Fournisseur non reconnu'}</div>
              <div class="r">{#if l.montant_ttc !== null}<Montant valeur={l.montant_ttc} />{/if}</div>
              <div class="b">
                {libelleType(l.type)}{#if l.numero}{' '}<span class="num">{l.numero}</span>{/if}{l.societe ? `, ${nomCourtSociete(l.societe)}` : ''}{#if f}<span class="flag {f.ton}">&nbsp; {f.texte}</span>{/if}
              </div>
              <div class="r2">{age(l.recu_le)}</div>
            </a>
          {:else}
            <p class="muted petit doc-rien">Rien à valider</p>
          {/each}
          {#if horsLimite > 0}<p class="plus">{horsLimite} autres, à voir après ceux-ci</p>{/if}
        </section>

        <section class="sec">
          <div class="sec-h"><h2>À répondre</h2><span class="num">{file.compteurs.a_repondre}</span></div>
          {#each file.a_repondre as m (m.id)}
            <a class="ligne" class:cur={mailId === m.id} aria-current={mailId === m.id ? 'true' : undefined} href="/?mail={m.id}" data-cle="m{m.id}">
              <div class="a">{m.resume ? `${nomExpediteur(m.expediteur)} : ${minuscule(m.resume)}` : m.objet || '(sans objet)'}</div>
              <div class="r">{#if libellePriorite(m.priorite)}<span class="flag e">{libellePriorite(m.priorite)}</span>{/if}</div>
              <div class="b">
                {m.resume ? m.objet || '(sans objet)' : nomExpediteur(m.expediteur)}{#if m.brouillon}<span class="flag">&nbsp; {libelleBrouillon(m.brouillon)}</span>{/if}
              </div>
              <div class="r2">{quandCourt(m.date_mail)}</div>
            </a>
          {:else}
            <p class="muted petit doc-rien">Aucune question en attente</p>
          {/each}
        </section>
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      <span><Kbd t="Enter" /> {docId ? 'valider' : mailId ? 'répondre' : 'commencer'}</span>
      <span><Kbd t="/" /> chercher</span>
    </div>
  {/snippet}

  {#snippet detail()}
    {#if docId}
      <DocFiche
        id={docId}
        {position}
        {ensuite}
        {annonce}
        onfait={apres}
        onchange={() => void chargerFile().catch(() => {})}
        onprecedent={elements.length ? () => deplacerDe(-1) : undefined}
        onsuivant={elements.length ? () => deplacerDe(1) : undefined}
      />
    {:else if mailId}
      <div class="volet-h">
        <a class="retour tel-seul" href="/"><Icone nom="gauche" />À traiter</a>
        {#if position}<span class="pos">{position.section} <span class="num">{position.n}</span> sur <span class="num">{position.total}</span></span>{/if}
        <span class="sp"></span>
        <button type="button" class="btn sm fantome" aria-label="Précédent" onclick={() => deplacerDe(-1)}><Icone nom="haut" /><Kbd t="K" /></button>
        <button type="button" class="btn sm fantome" aria-label="Suivant" onclick={() => deplacerDe(1)}><Icone nom="bas" /><Kbd t="J" /></button>
      </div>
      <div class="volet-b">
        <div class="contenu-fiche avec-barre doc-mail">
          {#if erreurMail}
            <Bandeau ton="erreur">{erreurMail}</Bandeau>
          {:else if !mail}
            <Chargement texte="Chargement du mail" />
          {:else}
            <div class="titre-fiche"><h1>{mail.objet || '(sans objet)'}</h1></div>
            <div class="meta-fiche">
              {nomExpediteur(mail.expediteur)}{mail.boites.length ? `, à ${mail.boites.map((b) => b.libelle).join(', ')}` : ''}{mail.date_mail ? `, ${jourHeure(mail.date_mail)}` : ''}
              {#if libellePriorite(mail.priorite)}<span class="flag e">&nbsp; {libellePriorite(mail.priorite)}</span>{/if}
            </div>
            {#if mail.resume}
              <div class="encadre doc-resume">
                <div>
                  <div class="muted petit">En une phrase</div>
                  <p>{majuscule(mail.resume)}</p>
                </div>
              </div>
            {/if}
            {#if ligneMail?.brouillon}<p class="t2 petit">{libelleBrouillon(ligneMail.brouillon)}</p>{/if}
            <dl class="doc-de">
              <dt>De</dt><dd>{mail.expediteur}</dd>
              {#if mail.destinataires.length}<dt>À</dt><dd>{mail.destinataires.join(', ')}</dd>{/if}
            </dl>
            {#if extrait}<p class="doc-extrait">{extrait}</p>{/if}
            {#each mail.pieces as p (p.id)}
              <a class="encadre doc-piece" href={p.document_id ? `/?doc=${p.document_id}` : `/api/pieces/${p.id}`}>
                <Icone nom="fichier" />
                <span>{p.nom}</span>
                <span class="sp"></span>
                <span class="t2 petit">{p.document_id ? 'Ouvrir la fiche' : 'Télécharger'}</span>
              </a>
            {/each}
          {/if}
        </div>
      </div>
      <BarreAction principal={{ libelle: 'Ouvrir et répondre', touche: 'Enter', href: `/mails/${mailId}` }} {ensuite} />
    {/if}
  {/snippet}

  {#snippet vide()}
    {#if file && file.compteurs.total === 0}
      <Vide icone="coche" titre="Tout est à jour" texte="Aucun document à valider ni mail à répondre." />
    {:else if file && file.a_valider.length}
      <Vide icone="fichier" titre="{file.compteurs.a_valider} document{file.compteurs.a_valider > 1 ? 's' : ''} à valider" texte="Les plus anciens et ceux qui ont une alerte d’abord.">
        <button type="button" class="btn principal" onclick={commencer}>Commencer <Kbd t="Enter" /></button>
      </Vide>
    {:else if file}
      <Vide icone="mail" titre="Choisissez un élément de la file" texte="J et K passent d’une ligne à l’autre." />
    {/if}
  {/snippet}
</Volets>

<style>
  .doc-annonce {
    padding: var(--e3) var(--e4) 0;
  }
  .doc-rien {
    margin: 0;
    padding: var(--e1) var(--e4) var(--e2);
  }
  /* ligne de Bloqué : le titre ouvre la fiche, le bouton débloque */
  .doc-bloc {
    position: relative;
  }
  .doc-lien {
    color: inherit;
    text-decoration: none;
  }
  .doc-lien::after {
    content: '';
    position: absolute;
    inset: 0;
  }
  .doc-bloc .r {
    position: relative;
    z-index: 1;
  }
  .doc-resume {
    margin-bottom: var(--e4);
  }
  .doc-resume p {
    margin: 0;
    font-size: 14px;
    line-height: 21px;
    color: var(--text);
  }
  .doc-de {
    display: grid;
    grid-template-columns: 52px 1fr;
    row-gap: 2px;
    margin: 0 0 var(--e4);
    padding-bottom: var(--e3);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .doc-de dt {
    color: var(--muted);
  }
  .doc-de dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .doc-extrait {
    white-space: pre-line;
    line-height: 21px;
    color: var(--text);
    max-width: 62ch;
  }
  .doc-piece {
    align-items: center;
    margin-top: var(--e3);
    text-decoration: none;
    color: var(--text);
  }
  .doc-piece:hover {
    border-color: var(--line2);
  }
</style>
