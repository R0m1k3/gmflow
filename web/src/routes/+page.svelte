<script lang="ts">
  // Accueil : le tableau de bord. En haut, les chiffres du jour (mails reçus, à répondre,
  // factures et BL à valider) ; au centre, les derniers mails ; à droite, ce qui attend une
  // réponse, les dernières factures et les derniers BL. Chaque ligne ouvre l'onglet concerné.
  import { untrack } from 'svelte';
  import { api, message, requete, sonder } from '#lib/api.ts';
  import { compteurs } from '#lib/compteurs.svelte.ts';
  import { libelleCategorie, libellePriorite, libelleStatutReponse, libelleType, nomCourtSociete, tonStatutReponse } from '#lib/libelles.ts';
  import { parJour } from '#lib/liste.svelte.ts';
  import { session } from '#lib/session.svelte.ts';
  import { societe } from '#lib/societe.svelte.ts';
  import type { LigneARepondre, LigneDocument, LigneMail, Page } from '#lib/types.ts';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import { dateMail, nomExpediteur } from '#lib/ui/MailFiche.svelte';
  import Montant from '#lib/ui/Montant.svelte';
  import Vide from '#lib/ui/Vide.svelte';

  const NB_MAILS = 30;
  const NB_COTE = 6;

  interface Tableau {
    mails_du_jour: { societe_id: number; raison_sociale: string; n: number }[];
  }
  interface Donnees {
    mailsDuJour: number;
    mails: LigneMail[];
    aRepondre: Page<LigneARepondre>;
    factures: LigneDocument[];
    facturesAValider: number;
    bl: LigneDocument[];
    blAValider: number;
  }

  let d = $state<Donnees | null>(null);
  let erreur = $state('');
  let tour = 0;

  async function charger() {
    const moi = ++tour;
    const s = societe.id;
    const doc = (p: Record<string, string>) => api.get<Page<LigneDocument>>(`/api/documents${requete({ societe: s, ...p })}`);
    const [t, mails, rep, fac, facV, bl, blV] = await Promise.all([
      api.get<Tableau>(`/api/tableau-de-bord${requete({ societe: s })}`),
      api.get<Page<LigneMail>>(`/api/mails${requete({ societe: s, categorie: 'hors_publicite' })}`),
      api.get<Page<LigneARepondre>>(`/api/a-repondre${requete({ societe: s })}`),
      doc({ type: 'facture' }),
      doc({ type: 'facture', verifie: 'non' }),
      doc({ type: 'bl' }),
      doc({ type: 'bl', verifie: 'non' })
    ]);
    if (moi !== tour) return;
    d = {
      mailsDuJour: t.mails_du_jour.reduce((n, l) => n + l.n, 0),
      mails: mails.elements.slice(0, NB_MAILS),
      aRepondre: rep,
      factures: fac.elements.slice(0, NB_COTE),
      facturesAValider: facV.total,
      bl: bl.elements.slice(0, NB_COTE),
      blAValider: blV.total
    };
    erreur = '';
  }

  $effect(() => {
    void societe.id;
    untrack(() => {
      d = null;
      charger().catch((e) => (erreur = message(e)));
    });
    return sonder(async () => {
      if (document.visibilityState === 'visible') await charger().catch((e) => (erreur = message(e)));
    }, 60_000);
  });

  let groupes = $derived(d ? parJour(d.mails, (m) => m.date_mail) : []);
  let admin = $derived(session.moi?.role === 'admin');

  const aujourdhui = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', weekday: 'long', day: 'numeric', month: 'long' }).format(new Date());
  const jour = aujourdhui.charAt(0).toUpperCase() + aujourdhui.slice(1);

  const fournisseur = (x: LigneDocument) => x.fournisseur ?? x.fournisseur_nom ?? 'Fournisseur non reconnu';
</script>

<svelte:head><title>Accueil · GMFlow</title></svelte:head>

<div class="page accueil">
  <div class="page-h">
    <h1>{jour}</h1>
    {#if societe.courante}<span class="muted">{societe.courante.raison_sociale}</span>{/if}
  </div>

  {#if admin && compteurs.problemes}
    <Bandeau ton="erreur" titre="{compteurs.problemes} problème{compteurs.problemes > 1 ? 's' : ''} à régler.">
      Une boîte, Claude ou un envoi est bloqué.
      {#snippet action()}<a class="btn sm" href="/a-traiter#bloque">Voir</a>{/snippet}
    </Bandeau>
  {/if}

  {#if erreur}
    <Bandeau ton="erreur" titre="Tableau de bord non chargé.">
      {erreur}
      {#snippet action()}<button type="button" class="btn sm" onclick={() => charger().catch((e) => (erreur = message(e)))}>Réessayer</button>{/snippet}
    </Bandeau>
  {/if}

  {#if !d}
    {#if !erreur}<Chargement texte="Chargement du tableau de bord" />{/if}
  {:else}
    <div class="chiffres">
      <a class="chiffre" href="/mails">
        <span class="n num">{d.mailsDuJour}</span>
        <span class="l"><Icone nom="mail" />Mails reçus aujourd’hui</span>
      </a>
      <a class="chiffre" class:alerte={d.aRepondre.total > 0} href="/a-repondre">
        <span class="n num">{d.aRepondre.total}</span>
        <span class="l"><Icone nom="envoyer" />À répondre</span>
      </a>
      <a class="chiffre" class:alerte={d.facturesAValider > 0} href="/factures?filtre=a-valider">
        <span class="n num">{d.facturesAValider}</span>
        <span class="l"><Icone nom="fichier" />Factures à valider</span>
      </a>
      <a class="chiffre" class:alerte={d.blAValider > 0} href="/bl?filtre=a-valider">
        <span class="n num">{d.blAValider}</span>
        <span class="l"><Icone nom="camion" />BL à valider</span>
      </a>
    </div>

    <div class="grille">
      <section class="bloc mails">
        <div class="bloc-h">
          <h2>Derniers mails</h2>
          <span class="muted petit large-seul">publicités à part</span>
          <span class="sp"></span>
          <a class="btn sm fantome" href="/mails">Tous les mails <Icone nom="droite" /></a>
        </div>
        {#if !d.mails.length}
          <Vide compact icone="mail" titre="Aucun mail reçu" texte="Les mails des boîtes relevées arrivent ici." />
        {:else}
          {#each groupes as g (g.titre)}
            <div class="jour-h">{g.titre}</div>
            {#each g.elements as m (m.id)}
              <a class="ligne" href="/mails?mail={m.id}">
                <div class="a">{nomExpediteur(m.expediteur) || '(expéditeur inconnu)'}</div>
                <div class="r">
                  {#if m.statut_reponse === 'a_repondre'}
                    <Etiquette ton={tonStatutReponse(m.statut_reponse)}>{libelleStatutReponse(m.statut_reponse)}</Etiquette>
                  {:else if m.categorie}
                    <span class="muted petit">{libelleCategorie(m.categorie)}</span>
                  {/if}
                </div>
                <div class="b">
                  {#if m.nb_pieces}<span class="pj"><Icone nom="trombone" /></span>{/if}{m.objet || '(sans objet)'}
                </div>
                <div class="r2"><span class="num">{dateMail(m.date_mail, false)}</span></div>
              </a>
            {/each}
          {/each}
        {/if}
      </section>

      <div class="cote">
        <section class="bloc">
          <div class="bloc-h">
            <h2>À répondre</h2>
            <span class="num muted">{d.aRepondre.total}</span>
            <span class="sp"></span>
            <a class="btn sm fantome" href="/a-repondre">Tout voir <Icone nom="droite" /></a>
          </div>
          {#each d.aRepondre.elements.slice(0, NB_COTE) as m (m.id)}
            <a class="ligne" href="/a-repondre?mail={m.id}">
              <div class="a">{nomExpediteur(m.expediteur) || '(expéditeur inconnu)'}</div>
              <div class="r">{#if libellePriorite(m.priorite)}<span class="flag e">{libellePriorite(m.priorite)}</span>{/if}</div>
              <div class="b">{m.resume || m.objet || '(sans objet)'}</div>
              <div class="r2"><span class="num">{dateMail(m.date_mail, false)}</span></div>
            </a>
          {:else}
            <p class="rien">Aucune question en attente</p>
          {/each}
        </section>

        <section class="bloc">
          <div class="bloc-h">
            <h2>Dernières factures</h2>
            <span class="sp"></span>
            <a class="btn sm fantome" href="/factures">Toutes <Icone nom="droite" /></a>
          </div>
          {#each d.factures as x (x.id)}
            <a class="ligne" href="/factures?doc={x.id}">
              <div class="a">{fournisseur(x)}</div>
              <div class="r">{#if x.montant_ttc !== null}<Montant valeur={x.montant_ttc} />{/if}</div>
              <div class="b">{libelleType(x.type)}{#if x.numero}{' '}<span class="num">{x.numero}</span>{/if}{x.societe ? `, ${nomCourtSociete(x.societe)}` : ''}</div>
              <div class="r2">{#if !x.verifie}<span class="flag w">À valider</span>{:else}<span class="num">{dateMail(x.date_mail, false)}</span>{/if}</div>
            </a>
          {:else}
            <p class="rien">Aucune facture reçue</p>
          {/each}
        </section>

        <section class="bloc">
          <div class="bloc-h">
            <h2>Derniers BL</h2>
            <span class="sp"></span>
            <a class="btn sm fantome" href="/bl">Tous <Icone nom="droite" /></a>
          </div>
          {#each d.bl as x (x.id)}
            <a class="ligne" href="/bl?doc={x.id}">
              <div class="a">{fournisseur(x)}</div>
              <div class="r">{#if x.montant_ttc !== null}<Montant valeur={x.montant_ttc} />{/if}</div>
              <div class="b">BL{#if x.numero}{' '}<span class="num">{x.numero}</span>{/if}{x.societe ? `, ${nomCourtSociete(x.societe)}` : ''}</div>
              <div class="r2">{#if !x.verifie}<span class="flag w">À valider</span>{:else}<span class="num">{dateMail(x.date_mail, false)}</span>{/if}</div>
            </a>
          {:else}
            <p class="rien">Aucun BL reçu</p>
          {/each}
        </section>
      </div>
    </div>
  {/if}
</div>

<style>
  .accueil {
    max-width: 1400px;
  }
  .chiffres {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    border: 1px solid var(--line);
    border-radius: var(--r3);
    margin-bottom: var(--e6);
    background: var(--white);
  }
  .chiffre {
    display: flex;
    flex-direction: column;
    gap: var(--e1);
    padding: var(--e4) var(--e5);
    color: var(--text);
    text-decoration: none;
    transition: background-color var(--vite) var(--ease);
  }
  .chiffre + .chiffre {
    border-left: 1px solid var(--line);
  }
  .chiffre:hover {
    background: var(--pane);
  }
  .chiffre .n {
    font-size: 28px;
    line-height: 36px;
    font-weight: 600;
    color: var(--black);
  }
  .chiffre.alerte .n {
    color: var(--warn);
  }
  .chiffre .l {
    display: flex;
    align-items: center;
    gap: var(--e2);
    color: var(--text2);
  }
  .grille {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: var(--e6);
    align-items: start;
  }
  .cote {
    display: flex;
    flex-direction: column;
    gap: var(--e6);
  }
  .bloc {
    border: 1px solid var(--line);
    border-radius: var(--r3);
    background: var(--white);
    overflow: hidden;
  }
  .bloc-h {
    display: flex;
    align-items: center;
    gap: var(--e2);
    height: 44px;
    padding: 0 var(--e2) 0 var(--e4);
    border-bottom: 1px solid var(--line);
  }
  .bloc .ligne + .ligne {
    border-top: 1px solid var(--line-soft, var(--line));
  }
  .rien {
    margin: 0;
    padding: var(--e3) var(--e4);
    color: var(--muted);
    font-size: 13px;
  }
  .pj {
    display: inline-flex;
    vertical-align: -3px;
    margin-right: 4px;
    color: var(--muted);
  }
  .pj :global(svg) {
    width: 14px;
    height: 14px;
  }
  .jour-h {
    position: static;
  }

  @media (max-width: 1000px) {
    .grille {
      grid-template-columns: 1fr;
    }
  }
  @media (max-width: 760px) {
    .chiffres {
      grid-template-columns: repeat(2, 1fr);
    }
    .chiffre:nth-child(3) {
      border-left: 0;
    }
    .chiffre:nth-child(n + 3) {
      border-top: 1px solid var(--line);
    }
    .chiffre .n {
      font-size: 24px;
    }
  }
</style>
