<script lang="ts">
  // À traiter (maquette Sauge S1) : la file à gauche (Bloqué, À valider, À répondre), l'élément
  // ouvert à droite (?doc=ID : fiche document ; ?mail=ID : aperçu du mail). J/K, Entrée, Échap.
  // Téléphone : la liste, puis l'élément en page (Volets).
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { aTraiter, ecarterDocument, ErreurApi, message, sonder, validerDocument } from '#lib/api.ts';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { rafraichirCompteurs } from '#lib/compteurs.svelte.ts';
  import { libelleBrouillon, libellePriorite, libelleType, nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { Bloque, FileATraiter } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import DocFiche, { type PositionFiche, type ResultatFiche } from '#lib/ui/DocFiche.svelte';
  import { age, blSansFacture, drapeau, montantTuile, nomCourtDocument, nomExpediteur, quandCourt, typeNumero } from '#lib/ui/DocOutils.ts';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import MailFiche from '#lib/ui/MailFiche.svelte';
  import Montant, { formatMontant } from '#lib/ui/Montant.svelte';
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

  // Téléphone : la section Bloqué tient en une bande, ouverte au toucher (ou par /#bloque).
  let bloqueOuvert = $state(false);

  // Arrivée par une ancre (/#bloque depuis la pastille d'état, /#a-repondre depuis l'ancienne page) :
  // la section n'existe qu'une fois la file chargée.
  let ancreVue = '';
  $effect(() => {
    if (!file) return;
    const ancre = page.url.hash.slice(1);
    if (!ancre || ancre === ancreVue) return;
    ancreVue = ancre;
    if (ancre === 'bloque') bloqueOuvert = true;
    requestAnimationFrame(() => document.getElementById(ancre)?.scrollIntoView({ block: 'start' }));
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
        .map((b) => ({ cle: `d${b.document_id}`, href: `/a-traiter?doc=${b.document_id}`, section: 'bloque' as const })),
      ...file.a_valider.map((l) => ({ cle: `d${l.id}`, href: `/a-traiter?doc=${l.id}`, section: 'a_valider' as const })),
      ...file.a_repondre.map((m) => ({ cle: `m${m.id}`, href: `/a-traiter?mail=${m.id}`, section: 'a_repondre' as const }))
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
    void goto('/a-traiter', { reset: false });
  }
  function commencer() {
    const s = file?.suivant ?? file?.a_valider[0]?.id;
    if (s) void goto(`/a-traiter?doc=${s}`, { reset: false });
    else if (file?.a_repondre[0]) void goto(`/a-traiter?mail=${file.a_repondre[0].id}`, { reset: false });
    else if (elements[0]) void goto(elements[0].href, { reset: false });
  }

  $effect(() => {
    const ouvert = !!(docId || mailId);
    const carte: Record<string, () => void> = {
      j: () => deplacerDe(1),
      k: () => deplacerDe(-1)
    };
    if (ouvert) carte.Escape = fermer;
    // Entrée sur un document ouvert : la fiche valide ; sur un mail, Ctrl+Entrée envoie la réponse.
    if (!ouvert) carte.Enter = commencer;
    // Échap ne ferme pas le volet pendant qu'on tape une réponse ou corrige un champ.
    return raccourcis(carte, { dansLesChamps: [] });
  });

  // Position et « Ensuite » de l'élément ouvert.
  const SECTIONS = { bloque: 'Bloqué', a_valider: 'Pièce', a_repondre: 'À répondre' } as const;
  let position = $derived.by((): PositionFiche | null => {
    if (!file || !courant) return null;
    const de = elements.filter((e) => e.section === courant!.section);
    const total =
      courant.section === 'a_valider' ? file.compteurs.a_valider : courant.section === 'a_repondre' ? file.compteurs.a_repondre : de.length;
    return { section: SECTIONS[courant.section], n: de.indexOf(courant) + 1, total };
  });
  let ensuite = $derived.by(() => {
    // « Ensuite » de la fiche document ; la fiche mail donne sa propre position dans À répondre.
    if (!file || !courant || courant.section === 'a_repondre') return undefined;
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
    let cible = '/a-traiter';
    if (section === 'a_valider') {
      // Après le dernier, le volet vide dit « Tout est validé ».
      if (r.suivant) cible = `/a-traiter?doc=${r.suivant}`;
    } else {
      const reste = elements.filter((e) => e.section === section);
      const e = reste[Math.max(0, avant)] ?? reste[reste.length - 1];
      if (e) cible = e.href;
      else if (file?.suivant) cible = `/a-traiter?doc=${file.suivant}`;
    }
    annoncer(r.info, cible);
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
        annoncer(`Envoi relancé : ${nomCourtDocument(b)}`, '/a-traiter');
      } else {
        await ecarterDocument(b.document_id, societe.id);
        annoncer(`Écarté : ${b.genre === 'illisible' ? (b.libelle ?? 'document illisible') : nomCourtDocument(b)}`, '/a-traiter');
      }
      void rafraichirCompteurs();
      await chargerFile();
      if (docId) await goto('/a-traiter', { replace: true, reset: false });
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
        // Refus noté pendant une lecture ou un tri : le message dit tout (« Claude refuse :
        // limite d’usage, reprise vers 15:00 »).
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

  // ---------- Mail ouvert : la fiche complète dans le volet ----------
  /** Après un envoi ou un classement : la file est relue, puis le mail suivant (ou la file). */
  async function apresMail(suivant: number | null) {
    await chargerFile().catch((e) => (erreurFile = message(e)));
    await goto(suivant ? `/a-traiter?mail=${suivant}` : file?.suivant ? `/a-traiter?doc=${file.suivant}` : '/a-traiter', { replace: true, reset: false });
  }

  /** « Demande le RIB » après « Sodistra : » devient « demande le RIB » (sauf sigle). */
  const minuscule = (t: string) => (/^\p{Lu}\p{Ll}/u.test(t) ? t.charAt(0).toLowerCase() + t.slice(1) : t);

  const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
  let horsLimite = $derived(file ? file.compteurs.a_valider - file.a_valider.length : 0);
  let ouvert = $derived(!!(docId || mailId));

  // BL sans facture liée : « En attente de facture » sous la tuile.
  let blAttente = $state(new Set<number>());
  $effect(() => {
    const ids = (file?.a_valider ?? []).filter((l) => l.type === 'bl').map((l) => l.id);
    if (!ids.length) {
      blAttente = new Set();
      return;
    }
    void blSansFacture(ids).then((r) => (blAttente = r));
  });
</script>

<svelte:head><title>À traiter · GMFlow</title></svelte:head>

<Volets {ouvert} libelleListe="File À traiter">
  {#snippet liste()}
    <div class="liste-h">
      <h1>À traiter{#if file}{' '}<span class="num">{file.compteurs.total}</span>{/if}</h1>
      <span class="sp"></span>
      {#if majA}<span class="muted petit">mis à jour à {heure.format(majA)}</span>{/if}
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
          {#if !bloqueOuvert}
            <button type="button" class="bande-bloque tel-seul" onclick={() => (bloqueOuvert = true)}>
              <span class="point"></span>
              {file.compteurs.bloque} problème{file.compteurs.bloque > 1 ? 's' : ''} à débloquer
              <span class="sp"></span><Icone nom="droite" />
            </button>
          {/if}
          <section class="sec" class:replie-tel={!bloqueOuvert} id="bloque">
            <div class="sec-h erreur"><span class="point"></span><h2>Bloqué</h2><span class="num">{file.compteurs.bloque}</span></div>
            {#each file.bloque as b (cleBloque(b))}
              {@const t = texteBloque(b)}
              {@const cle = b.document_id !== null ? `d${b.document_id}` : ''}
              <div class="ligne bloc doc-bloc" class:cur={cle !== '' && cle === cleCourante} data-cle={cle || undefined}>
                <div class="a">
                  {#if b.document_id !== null}<a class="doc-lien" href="/a-traiter?doc={b.document_id}">{t.a}</a>{:else}{t.a}{/if}
                </div>
                {#if t.b}<div class="b">{t.b}</div>{/if}
                <div class="r">
                  {#if b.genre === 'boite' && b.boite_id !== null}
                    <a class="btn sm" href="/reglages/boites/{b.boite_id}">Corriger</a>
                  {:else if b.genre === 'claude' && b.motif === 'limite'}
                    <a class="btn sm" href="/reglages#claude">Voir Claude</a>
                  {:else if b.genre === 'claude'}
                    <a class="btn sm" href="/reglages#claude">Se reconnecter</a>
                  {:else if b.genre === 'envoi_erreur'}
                    {#if admin}
                      <button type="button" class="btn sm" class:en-cours={occupe === cleBloque(b)} disabled={!!occupe} onclick={() => debloquer(b)}>Renvoyer</button>
                    {:else}
                      <a class="btn sm" href="/a-traiter?doc={b.document_id}">Voir</a>
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
            <h2>À valider</h2>
            {#if file.a_valider.length}
              <button type="button" class="btn sm tel-seul" onclick={commencer}>Commencer</button>
            {/if}
          </div>
          {#each file.a_valider as l (l.id)}
            {@const f = drapeau(l)}
            {@const m = montantTuile(l)}
            <a class="ligne" class:cur={docId === l.id} aria-current={docId === l.id ? 'true' : undefined} href="/a-traiter?doc={l.id}" data-cle="d{l.id}">
              <div class="a">{l.fournisseur ?? 'Fournisseur non reconnu'}</div>
              <div class="r">{#if m === 'sans'}— HT{:else if m !== null}<Montant valeur={m} avoir={l.type === 'avoir'} />{/if}</div>
              <div class="b"><span class="txt"><span class="large-seul">{typeNumero(l.type, l.numero)}</span><span class="tel-seul">{libelleType(l.type)}</span>{l.societe ? `, ${nomCourtSociete(l.societe)}` : ''}</span></div>
              <div class="r2">{age(l.recu_le)}</div>
              {#if f}
                <div class="c"><span class="alerte-l" class:doublon={f.ton === 'e'}><Icone nom="alerte" />{f.texte}</span></div>
              {:else if l.type === 'bl' && blAttente.has(l.id)}
                <div class="c attente">En attente de facture</div>
              {/if}
              {#if l.articles_ecart || (l.remise_ht && l.remise_ht > 0.005)}
                <div class="c">
                  {#if l.articles_ecart}<span class="etiquette att">{l.articles_ecart} écart{l.articles_ecart > 1 ? 's' : ''} BL</span>{/if}
                  {#if l.remise_ht && l.remise_ht > 0.005}<span class="etiquette info">Remise {formatMontant(l.remise_ht)}</span>{/if}
                </div>
              {/if}
            </a>
          {:else}
            <p class="doc-rien">Rien à valider</p>
          {/each}
          {#if horsLimite > 0}<p class="plus">{horsLimite} autres, à voir après ceux-ci</p>{/if}
        </section>

        <section class="sec" id="a-repondre">
          <div class="sec-h"><h2>À répondre <span class="num">{file.compteurs.a_repondre}</span></h2></div>
          {#each file.a_repondre as m (m.id)}
            <a class="ligne" class:cur={mailId === m.id} aria-current={mailId === m.id ? 'true' : undefined} href="/a-traiter?mail={m.id}" data-cle="m{m.id}">
              <div class="a">{m.resume ? `${m.fournisseur ?? nomExpediteur(m.expediteur)} : ${minuscule(m.resume)}` : m.objet || '(sans objet)'}</div>
              <div class="r">{#if libellePriorite(m.priorite)}<span class="flag e">{libellePriorite(m.priorite)}</span>{/if}</div>
              <div class="b">
                {m.resume ? m.objet || '(sans objet)' : (m.fournisseur ?? nomExpediteur(m.expediteur))}{#if m.brouillon}<span class="flag">&nbsp; {libelleBrouillon(m.brouillon)}</span>{/if}
              </div>
              <div class="r2">{quandCourt(m.date_mail)}</div>
            </a>
          {:else}
            <p class="doc-rien">Aucune question en attente</p>
          {/each}
        </section>
      {/if}
    </div>
    <div class="liste-f">
      <span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span>
      {#if mailId}<span><Kbd t={['Ctrl', 'Enter']} /> envoyer</span>{:else}<span><Kbd t="Enter" /> {docId ? 'valider' : file && !file.a_valider.length && file.a_repondre.length ? 'répondre' : 'commencer'}</span>{/if}
      {#if !mailId}<span><Kbd t="E" /> écarter</span>{/if}
    </div>
  {/snippet}

  {#snippet detail()}
    {#if docId}
      <DocFiche
        id={docId}
        {position}
        {ensuite}
        sansSuite="Aucune autre pièce à valider"
        {annonce}
        onfait={apres}
        onchange={() => void chargerFile().catch(() => {})}
        onprecedent={elements.length ? () => deplacerDe(-1) : undefined}
        onsuivant={elements.length ? () => deplacerDe(1) : undefined}
      />
    {:else if mailId}
      <MailFiche id={mailId} volet ouvrir={apresMail} />
    {/if}
  {/snippet}

  {#snippet vide()}
    {#if file && file.compteurs.total === 0}
      <Vide icone="coche" titre="Tout est à jour" texte="Aucun document à valider ni mail à répondre." />
    {:else if file && file.a_valider.length}
      <Vide icone="fichier" titre="{file.compteurs.a_valider} document{file.compteurs.a_valider > 1 ? 's' : ''} à valider" texte="Les plus anciens et ceux qui ont une alerte d’abord.">
        <button type="button" class="btn principal" onclick={commencer}>Commencer <Kbd t="Enter" /></button>
      </Vide>
    {:else if file && file.a_repondre.length}
      {@const n = file.compteurs.a_repondre}
      <Vide icone="coche" titre="Tout est validé" texte="{n} mail{n > 1 ? 's attendent' : ' attend'} une réponse.">
        <button type="button" class="btn principal" onclick={commencer}>Répondre <Kbd t="Enter" /></button>
      </Vide>
    {:else if file}
      {@const n = file.compteurs.bloque}
      <Vide icone="coche" titre="Tout est validé" texte="Reste {n} problème{n > 1 ? 's' : ''} à débloquer, dans la file." />
    {/if}
  {/snippet}
</Volets>

<style>
  .doc-annonce {
    padding: var(--e3) var(--e4) 0;
  }
  .doc-rien {
    margin: 0;
    padding: 0 4px var(--e2);
    font-size: 15px;
    color: var(--text2);
  }
  .txt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .attente {
    color: var(--text2);
  }
  .alerte-l.doublon {
    color: var(--err);
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
  /* Téléphone : Bloqué replié en une bande */
  .bande-bloque {
    display: flex;
    align-items: center;
    gap: var(--e2);
    width: 100%;
    padding: var(--e3) var(--e4);
    border: 0;
    background: var(--err-bg);
    color: var(--err);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
  }
  @media (max-width: 760px) {
    .replie-tel {
      display: none;
    }
  }
</style>
