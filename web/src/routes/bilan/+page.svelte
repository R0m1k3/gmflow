<script lang="ts">
  // Bilan du mois : par société, reçues / envoyées / restantes, avec liens vers ce qui reste à faire ;
  // BL en attente de facture par fournisseur ; replié en bas, qualité de la lecture et du tri.
  import Aide from '#lib/ui/Aide.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { api, bilanMois, message } from '#lib/api.ts';
  import { raccourcis } from '#lib/clavier.ts';
  import { dateSimple } from '#lib/format.ts';
  import { BL_EN_ATTENTE, libelleVoie, nomCourtSociete } from '#lib/libelles.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { BilanMois, Compte, LigneSuivi, SemaineTri, Voie } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import Montant from '#lib/ui/Montant.svelte';
  import QualiteLecture from '#lib/ui/QualiteLecture.svelte';
  import Repli from '#lib/ui/Repli.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  let admin = $derived(session.moi?.role === 'admin');

  // ---------- Mois (AAAA-MM, heure de Paris) ----------
  function moisCourant(): string {
    const p = new Intl.DateTimeFormat('en-GB', { timeZone: 'Europe/Paris', year: 'numeric', month: '2-digit' }).formatToParts(new Date());
    return `${p.find((x) => x.type === 'year')?.value}-${p.find((x) => x.type === 'month')?.value}`;
  }
  function decaler(m: string, n: number): string {
    const [a, mo] = m.split('-').map(Number);
    const d = new Date(Date.UTC(a, mo - 1 + n, 1));
    return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}`;
  }
  const fmtMois = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric', timeZone: 'UTC' });
  function libelleMois(m: string): string {
    const t = fmtMois.format(new Date(`${m}-01T12:00:00Z`));
    return t.charAt(0).toUpperCase() + t.slice(1);
  }

  const courant = moisCourant();
  let mois = $derived.by(() => {
    const m = page.url.searchParams.get('mois');
    return m && /^\d{4}-(0[1-9]|1[0-2])$/.test(m) && m <= courant ? m : courant;
  });
  let precedent = $derived(decaler(mois, -1));
  let suivant = $derived(mois < courant ? decaler(mois, 1) : null);
  const lienMois = (m: string) => (m === courant ? '/bilan' : `/bilan?mois=${m}`);

  $effect(() =>
    raccourcis({
      ArrowLeft: () => void goto(lienMois(precedent), { reset: false }),
      ArrowRight: () => {
        if (!suivant) return false;
        void goto(lienMois(suivant), { reset: false });
      }
    })
  );

  // ---------- Données ----------
  let bilan = $state<BilanMois | null>(null);
  let erreur = $state('');
  let jeton = 0;
  async function charger() {
    const moi = ++jeton;
    try {
      // Responsable : pas de `societe=` (on perdrait la ligne « pas encore rangées ») ; filtrée plus bas.
      const v = await bilanMois({ mois, societe: admin ? societe.id : null });
      if (moi !== jeton) return;
      bilan = v;
      erreur = '';
    } catch (e) {
      if (moi === jeton) erreur = message(e);
    }
  }
  $effect(() => {
    void mois;
    void societe.id;
    if (!session.moi) return;
    void charger();
  });

  // Responsable : uniquement sa société (et ce qui n'est pas encore rangé, reçu par ses boîtes).
  let lignes = $derived(
    (bilan?.societes ?? []).filter((l) => admin || l.societe_id === null || l.societe_id === session.moi?.societe_id)
  );
  let blEnAttente = $derived(
    (bilan?.bl_en_attente ?? []).filter((b) => admin || b.societe_id === null || b.societe_id === session.moi?.societe_id)
  );
  const aValider = (l: LigneSuivi) => l.pretes + l.a_verifier;
  let total = $derived(
    lignes.reduce(
      (t, l) => ({
        recues: t.recues + l.recues,
        deposees: t.deposees + l.deposees,
        a_valider: t.a_valider + aValider(l),
        en_erreur: t.en_erreur + l.en_erreur,
        doublons: t.doublons + l.doublons,
        montant: t.montant + l.montant_depose
      }),
      { recues: 0, deposees: 0, a_valider: 0, en_erreur: 0, doublons: 0, montant: 0 }
    )
  );
  let complet = $derived(total.recues > 0 && total.a_valider === 0 && total.en_erreur === 0 && total.doublons === 0);
  let plusieurs = $derived(lignes.length > 1);
  let restes = $derived.by(() => {
    const r: { n: number; texte: string }[] = [];
    if (total.a_valider) r.push({ n: total.a_valider, texte: 'à contrôler' });
    const b = total.en_erreur + total.doublons;
    if (b) r.push({ n: b, texte: b > 1 ? 'bloquées' : 'bloquée' });
    return r;
  });
  let blTotal = $derived(blEnAttente.reduce((t, b) => t + b.bl, 0));

  /** Lien « À contrôler » d'une société : la liste des factures à contrôler, avec cette société choisie (admin). */
  function versATraiter(e: MouseEvent, id: number | null) {
    if (!admin || id === null) return;
    e.preventDefault();
    societe.choisir(id);
    void goto('/factures?filtre=a-valider');
  }

  // ---------- Qualité ----------
  const compte = (l: Compte[], cle: string) => l.find((c) => c.cle === cle)?.n ?? 0;
  const pct = (n: number, sur: number) =>
    sur ? `${((100 * n) / sur).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} %` : '—';
  const bon = (n: number, sur: number) => sur > 0 && n / sur > 0.95;
  let lecture = $derived(bilan?.qualite.lecture ?? null);
  let tri = $derived(bilan?.qualite.tri ?? []);
  const nbMails = (s: SemaineTri) => s.par_categorie.reduce((t, c) => t + c.n, 0);
  const justesse = (s: SemaineTri) => (s.revus ? pct(s.revus - s.corriges, s.revus) : '—');
  let resumeQualite = $derived.by(() => {
    if (!lecture) return '';
    const m: string[] = [];
    if (lecture.verifies) m.push(`type juste à ${pct(lecture.type_justes, lecture.verifies)}`);
    const s = tri.find((x) => x.revus > 0);
    if (s) m.push(`tri juste à ${justesse(s)}`);
    return m.join(' · ') || 'Pas encore de document vérifié';
  });

  // BL lus avant le rapprochement (sans leurs lignes), donnés par le bilan ; remis à zéro après relecture.
  let qualiteOuverte = $state(false);
  let reluPour = $state<BilanMois | null>(null);
  let blSansLignes = $derived(bilan && reluPour !== bilan ? bilan.bl_sans_lignes : 0);
  let relecture = $state('');
  let relectureEnCours = $state(false);
  async function relireBl() {
    relectureEnCours = true;
    relecture = '';
    try {
      const r = await api.post<{ n: number }>('/api/admin/rapprochement/relire-bl');
      relecture = `${r.n} BL remis en lecture`;
      reluPour = bilan;
    } catch (e) {
      relecture = message(e);
    } finally {
      relectureEnCours = false;
    }
  }
</script>

<svelte:head><title>Bilan du mois · GMFlow</title></svelte:head>

{#snippet cellule(n: number, ton: 'alerte' | 'erreur', href: string, onclick?: (e: MouseEvent) => void)}
  {#if n > 0}<a class="num lien-reste {ton}" {href} {onclick}>{n}</a>{:else}<span class="num muted">0</span>{/if}
{/snippet}

<div class="page bilan">
  <div class="page-h">
    <h1>Bilan du mois <span class="imprime-seul">: {libelleMois(mois)}</span></h1>
    <span class="sp"></span>
    <button type="button" class="btn sm pas-imprime" onclick={() => window.print()}>Imprimer</button>
    <nav class="choix-mois pas-imprime" aria-label="Mois">
      <a class="btn sm fantome" href={lienMois(precedent)} data-sveltekit-reset="false" aria-label="Mois précédent" title="Mois précédent (←)"
        ><Icone nom="gauche" /></a
      >
      <span class="mois-l">{libelleMois(mois)}</span>
      {#if suivant}
        <a class="btn sm fantome" href={lienMois(suivant)} data-sveltekit-reset="false" aria-label="Mois suivant" title="Mois suivant (→)"
          ><Icone nom="droite" /></a
        >
      {:else}
        <span class="btn sm fantome" aria-disabled="true" aria-label="Mois suivant"><Icone nom="droite" /></span>
      {/if}
      <span class="large-seul touches-mois"><Kbd t="ArrowLeft" /><Kbd t="ArrowRight" /></span>
    </nav>
  </div>

  {#if erreur && !bilan}
    <Bandeau ton="erreur">
      {erreur}
      {#snippet action()}<button type="button" class="btn sm" onclick={charger}>Réessayer</button>{/snippet}
    </Bandeau>
  {:else if !bilan}
    <Chargement />
  {:else}
    {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}

    <section class="bloc" aria-labelledby="t-envois">
      <h2 id="t-envois">Factures et avoirs</h2>
      {#if lignes.length === 0}
        <Vide compact titre="Aucune facture en {libelleMois(mois).toLowerCase()}" texte="Le mois compte les factures et avoirs datés du mois (date du document, à défaut du mail)." />
      {:else}
        {#if complet}
          <Bandeau ton="ok" titre="Mois complet.">Tout ce qui a été reçu est envoyé à Dext ou Yooz, sans doublon en attente.</Bandeau>
        {:else}
          <p class="lead">
            <span class="num">{total.recues}</span> {total.recues > 1 ? 'reçues' : 'reçue'}, <span class="num">{total.deposees}</span>
            {total.deposees > 1 ? 'envoyées' : 'envoyée'}. Reste
            {#each restes as r, i (r.texte)}{i > 0 ? ', ' : ''}<span class="num">{r.n}</span> {r.texte}{/each}.
          </p>
        {/if}
        <div class="defile boite">
          <table>
            <thead>
              <tr>
                <th>Société</th>
                <th class="n">Reçues</th>
                <th class="n">Envoyées</th>
                <th class="n">À contrôler</th>
                <th class="n">Échec d’envoi</th>
                <th class="n">Doublons<Aide sujet="doublon_document" droite /></th>
                <th class="n">Montant envoyé</th>
              </tr>
            </thead>
            <tbody>
              {#each lignes as l (l.societe_id)}
                <tr>
                  <td>{l.societe ? nomCourtSociete(l.societe) : 'Pas encore rangées'}</td>
                  <td class="n"><span class="num">{l.recues}</span></td>
                  <td class="n"><span class="num">{l.deposees}</span></td>
                  <td class="n">{@render cellule(aValider(l), 'alerte', '/factures?filtre=a-valider', (e) => versATraiter(e, l.societe_id))}</td>
                  <td class="n">{@render cellule(l.en_erreur, 'erreur', '/#bloque')}</td>
                  <td class="n">{@render cellule(l.doublons, 'erreur', '/#bloque')}</td>
                  <td class="n"><Montant valeur={l.montant_depose} class={l.montant_depose ? '' : 'muted'} /></td>
                </tr>
              {/each}
            </tbody>
            {#if plusieurs}
              <tfoot>
                <tr>
                  <td>Total</td>
                  <td class="n"><span class="num">{total.recues}</span></td>
                  <td class="n"><span class="num">{total.deposees}</span></td>
                  <td class="n">{@render cellule(total.a_valider, 'alerte', '/factures?filtre=a-valider')}</td>
                  <td class="n">{@render cellule(total.en_erreur, 'erreur', '/#bloque')}</td>
                  <td class="n">{@render cellule(total.doublons, 'erreur', '/#bloque')}</td>
                  <td class="n"><Montant valeur={total.montant} /></td>
                </tr>
              </tfoot>
            {/if}
          </table>
        </div>
        <p class="petit muted note">
          Datées du mois (date du document, à défaut du mail). « À contrôler » compte aussi les factures validées qui attendent leur envoi.
        </p>
      {/if}
    </section>

    <section class="bloc" aria-labelledby="t-bl">
      <h2 id="t-bl">{BL_EN_ATTENTE} {#if blTotal}<span class="num muted">{blTotal}</span>{/if}</h2>
      {#if blEnAttente.length === 0}
        <p class="muted">Aucun BL du mois sans sa facture.</p>
      {:else}
        <div class="boite liste-bl">
          {#each blEnAttente as b (`${b.societe_id}-${b.fournisseur_id}-${b.fournisseur}`)}
            {#if b.fournisseur_id !== null}
              <a class="ligne" href="/fournisseurs?fiche={b.fournisseur_id}">
                <div class="a">{b.fournisseur ?? 'Fournisseur'}</div>
                <div class="r"><span class="num">{b.bl}</span> BL</div>
                <div class="b">{b.societe ? nomCourtSociete(b.societe) : 'Pas encore rangés'}</div>
              </a>
            {:else}
              <div class="ligne sans-lien">
                <div class="a">{b.fournisseur ?? 'Fournisseur non reconnu'}</div>
                <div class="r"><span class="num">{b.bl}</span> BL</div>
                <div class="b">{b.societe ? nomCourtSociete(b.societe) : 'Pas encore rangés'} · fournisseur non reconnu</div>
              </div>
            {/if}
          {/each}
        </div>
      {/if}
    </section>

    {#if lecture}
      <div class="replis bloc boite qualite-b">
        <Repli icone="graphique" titre="Qualité de la lecture et du tri" resume={resumeQualite} bind:ouvert={qualiteOuverte}>
          <p class="petit muted">Sur tout l'historique{admin ? ', toutes sociétés' : ''}, quel que soit le mois choisi.</p>

          <h3>Lecture des documents</h3>
          <table class="qualite">
            <tbody>
              <tr>
                <td>Documents lus</td>
                <td class="n"><span class="num">{compte(lecture.par_etat, 'lu')}</span></td>
                <td class="t2 petit">
                  <span class="num">{compte(lecture.par_etat, 'a_lire') + compte(lecture.par_etat, 'en_cours')}</span> en attente,
                  <span class="num">{compte(lecture.par_etat, 'erreur')}</span> illisibles,
                  <span class="num">{compte(lecture.par_etat, 'ignore')}</span> écartés
                </td>
              </tr>
              <tr>
                <td>Vérifiés à la main</td>
                <td class="n"><span class="num">{lecture.verifies}</span></td>
                <td class="t2 petit">objectif : 50 documents</td>
              </tr>
              <tr>
                <td>Type juste</td>
                <td class="n"><span class="num" class:ok={bon(lecture.type_justes, lecture.verifies)}>{pct(lecture.type_justes, lecture.verifies)}</span></td>
                <td class="t2 petit">des documents vérifiés, objectif : plus de 95 %</td>
              </tr>
              <tr>
                <td>Société juste</td>
                <td class="n"><span class="num" class:ok={bon(lecture.societe_justes, lecture.verifies)}>{pct(lecture.societe_justes, lecture.verifies)}</span></td>
                <td class="t2 petit">des documents vérifiés, objectif : plus de 95 %</td>
              </tr>
            </tbody>
          </table>
          {#if admin && lecture.par_voie.length}
            <p class="petit t2 voies">
              {#each lecture.par_voie.filter((v) => v.cle) as v, i (v.cle)}{i > 0 ? ' · ' : ''}{libelleVoie(v.cle as Voie)} :
                <span class="num">{v.n}</span>{/each}
            </p>
          {/if}
          {#if lecture.semaines.length}
            <table class="qualite">
              <thead><tr><th>Semaine du</th><th class="n">Validés</th><th class="n">Sans correction</th></tr></thead>
              <tbody>
                {#each lecture.semaines as s (s.semaine)}
                  <tr>
                    <td><span class="num">{dateSimple(s.semaine)}</span></td>
                    <td class="n"><span class="num">{s.valides}</span></td>
                    <td class="n"><span class="num">{pct(s.sans_correction, s.valides)}</span></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
          {#if admin && blSansLignes}
            <Bandeau titre="{blSansLignes} BL lus sans leurs lignes.">
              Lus avant le rapprochement : leurs quantités ne peuvent pas être comparées aux factures. Chaque relecture compte dans la limite du jour.
              {#snippet action()}
                <button type="button" class="btn sm" class:en-cours={relectureEnCours} disabled={relectureEnCours} onclick={relireBl}>Relire ces BL</button>
              {/snippet}
            </Bandeau>
          {/if}
          {#if relecture}<p class="petit t2">{relecture}</p>{/if}

          <h3>Tri des mails</h3>
          {#if tri.length}
            <table class="qualite">
              <thead>
                <tr>
                  <th>Semaine</th>
                  <th class="n">Mails</th>
                  <th class="n">Questions</th>
                  <th class="n">Répondu</th>
                  <th class="n">Pas besoin de réponse</th>
                  <th class="n">Tri juste</th>
                </tr>
              </thead>
              <tbody>
                {#each tri as s, i (s.debut)}
                  <tr>
                    <td>
                      {i === 0 ? 'Cette semaine' : 'Semaine dernière'}
                      <span class="muted petit">du <span class="num">{dateSimple(s.debut)}</span></span>
                    </td>
                    <td class="n"><span class="num">{nbMails(s)}</span></td>
                    <td class="n"><span class="num">{compte(s.par_categorie, 'question')}</span></td>
                    <td class="n"><span class="num">{s.repondu}</span></td>
                    <td class="n"><span class="num">{s.sans_suite}</span></td>
                    <td class="n" title={s.revus ? `${s.corriges} corrigés sur ${s.revus} revus à la main` : 'Aucun mail revu à la main'}>
                      <span class="num">{justesse(s)}</span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
            <p class="petit muted">« Tri juste » : part des mails revus à la main dont Claude ou une règle avait trouvé la bonne catégorie.</p>
          {:else}
            <p class="muted">Pas encore de mail trié.</p>
          {/if}
        </Repli>
        {#if admin}<QualiteLecture {mois} societe={societe.id} />{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  /* Bilan dans le langage Sauge (comme Réglages) : titre 26 px, sections à titre 22 px,
     tableaux et listes dans un encadré blanc à anneau #DDE2DA (rayon 12). */
  .bilan {
    max-width: 1120px;
  }
  .page-h {
    flex-wrap: wrap;
    margin-bottom: var(--e8);
  }
  .page-h h1 {
    white-space: nowrap;
    font-size: 26px;
    font-weight: 500;
  }
  .choix-mois {
    display: flex;
    align-items: center;
    gap: var(--e1);
    height: 44px;
    padding: 0 var(--e1);
    border-radius: var(--r2);
    box-shadow: 0 0 0 1px var(--line);
    background: var(--white);
  }
  .mois-l {
    min-width: 136px;
    text-align: center;
    font-weight: 500;
  }
  .touches-mois {
    display: inline-flex;
    gap: 2px;
    margin: 0 var(--e2);
  }
  .bloc {
    margin-top: var(--e10);
  }
  .bloc:first-of-type {
    margin-top: 0;
  }
  .bloc h2 {
    font-size: 22px;
    font-weight: 500;
    margin-bottom: var(--e2);
  }
  .bloc h2 .num {
    font-size: 18px;
    font-weight: 400;
  }
  .bloc .lead {
    margin: 0 0 var(--e4);
    font-size: 16px;
  }
  .boite {
    border-radius: var(--r3);
    box-shadow: 0 0 0 1px var(--line);
    background: var(--white);
  }
  .defile {
    overflow-x: auto;
    margin-top: var(--e4);
  }
  .defile :global(th) {
    padding-top: 12px;
    padding-bottom: 12px;
  }
  .defile :global(td) {
    height: 52px;
    font-size: 16px;
  }
  .defile :global(td:first-child),
  .defile :global(th:first-child) {
    padding-left: var(--e5);
  }
  .defile :global(td:last-child),
  .defile :global(th:last-child) {
    padding-right: var(--e5);
  }
  .defile :global(tbody tr:last-child td) {
    border-bottom: 0;
  }
  tfoot td {
    font-weight: 500;
    border-bottom: 0;
    border-top: 1px solid var(--line);
  }
  .lien-reste {
    text-decoration: none;
    font-weight: 500;
  }
  .lien-reste:hover {
    text-decoration: underline;
  }
  .lien-reste.alerte {
    color: var(--warn);
  }
  .lien-reste.erreur {
    color: var(--err);
  }
  .note {
    margin-top: var(--e3);
  }
  .liste-bl {
    margin-top: var(--e4);
    overflow: hidden;
  }
  .liste-bl > :global(.ligne) {
    padding: 14px var(--e5);
    border-radius: 0;
    border-bottom: 1px solid var(--line-faint);
  }
  .liste-bl > :global(.ligne:last-child) {
    border-bottom: 0;
  }
  .liste-bl :global(.ligne .b) {
    font-size: 15px;
  }
  .sans-lien {
    cursor: default;
  }
  .qualite-b {
    padding: 0 var(--e5);
  }
  .qualite-b :global(.repli) {
    border-bottom: 0;
  }
  .qualite-b :global(.repli > summary) {
    min-height: 56px;
  }
  .replis :global(h3) {
    font-size: 15px;
    font-weight: 500;
    color: var(--text2);
    margin: var(--e6) 0 var(--e2);
  }
  .qualite {
    margin-bottom: var(--e3);
  }
  .qualite td:nth-child(3) {
    padding-left: var(--e4);
  }
  .num.ok {
    color: var(--ok);
  }
  .voies {
    margin: var(--e2) 0 var(--e3);
  }
  @media (max-width: 760px) {
    .page-h {
      margin-bottom: var(--e6);
    }
    .bloc {
      margin-top: var(--e8);
    }
    .bloc h2 {
      font-size: 20px;
    }
    .qualite-b {
      padding: 0 var(--e3);
    }
    .liste-bl > :global(.ligne) {
      padding: 12px var(--e4);
    }
  }
</style>
