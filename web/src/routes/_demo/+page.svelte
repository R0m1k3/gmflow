<script lang="ts">
  // Page de démonstration du socle (phase 1). À supprimer en phase 3.
  // /_demo : écran À traiter en volets ; /_demo?vue=catalogue : tous les composants.
  import { page } from '$app/state';
  import { deplacer, montrer, raccourcis } from '#lib/clavier.ts';
  import { libelleValider, titreAlerte, libelleStatutEnvoi, tonStatutEnvoi, libelleMethode } from '#lib/libelles.ts';
  import ApercuPiece from '#lib/ui/ApercuPiece.svelte';
  import Bandeau from '#lib/ui/Bandeau.svelte';
  import BarreAction from '#lib/ui/BarreAction.svelte';
  import Champ from '#lib/ui/Champ.svelte';
  import Chargement from '#lib/ui/Chargement.svelte';
  import Confirmation from '#lib/ui/Confirmation.svelte';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Kbd from '#lib/ui/Kbd.svelte';
  import Montant from '#lib/ui/Montant.svelte';
  import Repli from '#lib/ui/Repli.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import Volets from '#lib/ui/Volets.svelte';

  let catalogue = $derived(page.url.searchParams.get('vue') === 'catalogue');

  type Ligne = { id: number; nom: string; montant: number | null; colis?: number; type: string; numero: string; societe: string; alerte?: { texte: string; ton: 'w' | 'e' }; age: string };
  const aValider: Ligne[] = [
    { id: 1, nom: 'Vileda', montant: 2210.11, type: 'Facture', numero: 'F2611-08734', societe: 'Frouard', alerte: { texte: 'Montant incohérent', ton: 'w' }, age: '8 j' },
    { id: 2, nom: 'Sodistra', montant: 986.4, type: 'Facture', numero: '2026/11/0457', societe: 'Houdemont', alerte: { texte: 'Doublon possible', ton: 'e' }, age: '6 j' },
    { id: 3, nom: 'Comptoir Lorrain d’Emballage', montant: null, colis: 18, type: 'BL', numero: 'CLE-22871', societe: 'Frouard', alerte: { texte: 'Nouveau fournisseur', ton: 'w' }, age: '5 j' },
    { id: 4, nom: 'Arc France', montant: 4118.4, type: 'Facture', numero: '26FA02104', societe: 'Houdemont', age: '3 j' },
    { id: 5, nom: 'Spontex', montant: -312, type: 'Avoir', numero: 'AV-55203', societe: 'Frouard', age: '2 j' },
    { id: 6, nom: '5five Simply Smart', montant: 1506.72, type: 'Facture', numero: 'FC118842', societe: 'Frouard', age: 'hier' },
    { id: 7, nom: 'Duralex', montant: null, colis: 32, type: 'BL', numero: '7781104', societe: 'Houdemont', age: 'hier' }
  ];
  const aRepondre = [
    { id: 11, a: 'Arc France relance la facture 26FA01877', b: 'Demande la date de règlement, échue le 15/11', urgent: true, quand: '09:41' },
    { id: 12, a: 'Sodistra demande le RIB de Houdemont', b: 'Pour rembourser l’avoir AV-26-0412', urgent: false, quand: 'hier' },
    { id: 13, a: 'Vileda propose de décaler la livraison', b: 'Commande 0458-1131 livrable le 28/11 au lieu du 26', urgent: false, quand: 'hier' }
  ];

  let indice = $state(0);
  let courant = $derived(aValider[indice]);
  let suivant = $derived(aValider[indice + 1]);
  let listeEl = $state<HTMLElement>();
  let ecarter = $state(false);
  let valide = $state('');

  let ttc = $state('2 270,11');
  let societeDoc = $state('frouard');

  $effect(() =>
    raccourcis({
      j: () => {
        indice = deplacer(indice, 1, aValider.length);
        montrer(listeEl, `[data-indice="${indice}"]`);
      },
      k: () => {
        indice = deplacer(indice, -1, aValider.length);
        montrer(listeEl, `[data-indice="${indice}"]`);
      },
      Enter: () => valider(),
      e: () => (ecarter = true)
    })
  );

  function valider() {
    valide = `${courant.type} ${courant.nom} validée`;
    indice = deplacer(indice, 1, aValider.length);
  }

  // catalogue
  let nom = $state('compta@houdemont-distribution.fr');
  let mdp = $state('');
  let ouvrirConfirmation = $state(false);
  let enCours = $state(false);
</script>

{#snippet facture()}
  <div class="paper">
    <div class="ph">
      <div class="vl">vileda<small>Vileda France SAS, division Retail<br />ZA des Tilleuls, 2 rue Ampère, 60100 Creil</small></div>
      <div>
        <div class="ft">Facture</div>
        <div class="meta"><span>N°</span><span>F2611-08734</span><span>Date</span><span>14/11/2026</span><span>Commande</span><span>0458-1126</span></div>
      </div>
    </div>
    <div class="addr">
      <div><em>Livré à</em>La Foir’Fouille Frouard<br />54390 Frouard</div>
      <div><em>Facturé à</em><b>SAS Frouard Distribution</b><br />Service comptabilité</div>
    </div>
    <table>
      <thead><tr><th>Réf.</th><th>Désignation</th><th class="r">Qté</th><th class="r">Montant HT</th></tr></thead>
      <tbody>
        <tr><td>160553</td><td>Balai Ultramax Box XL</td><td class="r">24</td><td class="r">453,60</td></tr>
        <tr><td>158634</td><td>Serpillière microfibre, lot de 2</td><td class="r">120</td><td class="r">378,00</td></tr>
        <tr><td>167422</td><td>Éponge Pur Active, lot de 3</td><td class="r">144</td><td class="r">295,20</td></tr>
        <tr><td>141019</td><td>Seau essoreur Easy Wring</td><td class="r">18</td><td class="r">385,20</td></tr>
        <tr><td>152280</td><td>Gants multi-usages taille M</td><td class="r">96</td><td class="r">132,48</td></tr>
        <tr><td>170311</td><td>Lavette microfibre Colors, lot de 4</td><td class="r">72</td><td class="r">197,28</td></tr>
      </tbody>
    </table>
    <div class="tot">
      <table>
        <tbody>
          <tr><td>Total HT</td><td class="r">1 841,76</td></tr>
          <tr><td>TVA 20 %</td><td class="r">368,35</td></tr>
          <tr class="ttc"><td>Total TTC (EUR)</td><td class="r">2 210,11</td></tr>
        </tbody>
      </table>
    </div>
  </div>
{/snippet}

{#if !catalogue}
  <Volets ouvert={!!courant} libelleListe="File À traiter">
    {#snippet liste()}
      <div class="liste-h"><h1>À traiter</h1><span class="num">19</span><span class="sp"></span><span class="muted petit">Mis à jour à 10:12</span></div>
      <div class="liste-s" bind:this={listeEl}>
        <section class="sec" id="bloque">
          <div class="sec-h erreur"><span class="point"></span><h2>Bloqué</h2><span class="num">2</span></div>
          <div class="ligne bloc">
            <div class="a">Envoi à Dext échoué : facture Spontex 2026-55120</div>
            <div class="b">Dext a refusé le mail, pièce jointe de 14 Mo</div>
            <div class="r"><button class="btn sm">Renvoyer</button></div>
          </div>
          <div class="ligne bloc">
            <div class="a">Boîte factures@houdemont-distribution.fr arrêtée</div>
            <div class="b">Mot de passe refusé par Infomaniak depuis 06:40</div>
            <div class="r"><a class="btn sm" href="/_demo?vue=catalogue">Corriger</a></div>
          </div>
        </section>
        <section class="sec">
          <div class="sec-h"><h2>À valider</h2><span class="num">12</span><button class="btn sm" onclick={() => (indice = 0)}>Commencer <Kbd t="Enter" /></button></div>
          {#each aValider as l, i (l.id)}
            <div class="ligne" class:cur={i === indice} data-indice={i} role="button" tabindex="0" aria-current={i === indice ? 'true' : undefined} onclick={() => (indice = i)} onkeydown={(e) => e.key === ' ' && (indice = i)}>
              <div class="a">{l.nom}</div>
              <div class="r">{#if l.montant !== null}<Montant valeur={l.montant} />{:else}<span class="t2 petit">{l.colis} colis</span>{/if}</div>
              <div class="b">{l.type} <span class="num">{l.numero}</span>, {l.societe}{#if l.alerte}<span class="flag {l.alerte.ton}">&nbsp; {l.alerte.texte}</span>{/if}</div>
              <div class="r2">{l.age}</div>
            </div>
          {/each}
          <button class="plus">5 autres, reçus aujourd’hui</button>
        </section>
        <section class="sec">
          <div class="sec-h"><h2>À répondre</h2><span class="num">5</span></div>
          {#each aRepondre as m (m.id)}
            <div class="ligne" role="button" tabindex="0">
              <div class="a">{m.a}</div>
              <div class="r">{#if m.urgent}<span class="flag e">Urgent</span>{/if}</div>
              <div class="b">{m.b}</div>
              <div class="r2">{m.quand}</div>
            </div>
          {/each}
        </section>
      </div>
      <div class="liste-f"><span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span><span><Kbd t="Enter" /> valider</span><span><Kbd t="/" /> chercher</span></div>
    {/snippet}

    {#snippet detail()}
      <div class="volet-h">
        <a class="retour tel-seul" href="/_demo" onclick={(e) => { e.preventDefault(); indice = -1; }}><Icone nom="gauche" />À traiter</a>
        <span class="pos">À valider <span class="num">{indice + 1}</span> sur <span class="num">12</span></span>
        <span class="sp"></span>
        <button class="btn sm fantome" aria-label="Précédent" onclick={() => (indice = deplacer(indice, -1, aValider.length))}><Icone nom="haut" /><Kbd t="K" /></button>
        <button class="btn sm fantome" aria-label="Suivant" onclick={() => (indice = deplacer(indice, 1, aValider.length))}><Icone nom="bas" /><Kbd t="J" /></button>
        <span class="vsep"></span>
        <a class="btn sm fantome large-seul" href="/_demo?vue=catalogue"><Icone nom="agrandir" />Plein écran <Kbd t="F" /></a>
      </div>
      <div class="volet-b">
        <div class="apercu-zone"><div class="apercu-cadre"><div class="zoom">{@render facture()}</div></div></div>
        <div class="contenu-fiche avec-barre">
          {#if valide}<Bandeau ton="ok" role="status">{valide}. Document suivant ouvert.</Bandeau>{/if}
          <div class="titre-fiche"><h1>{courant.type} {courant.nom}</h1><span class="num">{courant.numero}</span></div>
          <div class="meta-fiche">Reçue le 14/11 par factures@frouard-distribution.fr, lue dans le scan</div>
          {#if courant.id === 1}
            <Bandeau titre="{titreAlerte({ code: 'montants', message: '' })}.">HT + TVA donnent 2 210,11 €, le total lu est 2 270,11 €. Vérifiez le total sur la facture.</Bandeau>
          {/if}
          <div class="lignes-champs">
            <Champ disposition="ligne" libelle="Type" valeur="Facture" modifiable />
            <Champ
              disposition="ligne"
              libelle="Société"
              type="select"
              bind:valeur={societeDoc}
              options={[{ valeur: 'frouard', libelle: 'SAS Frouard Distribution' }, { valeur: 'houdemont', libelle: 'SAS Houdemont Distribution' }]}
              source="comme les 14 dernières"
              modifiable
            />
            <Champ disposition="ligne" libelle="Fournisseur" valeur={courant.nom} modifiable />
            <Champ disposition="ligne" libelle="N°" valeur={courant.numero} mono modifiable />
            <Champ disposition="ligne" libelle="Date" valeur="14/11/2026" indice="échéance 14/12/2026" mono modifiable />
            <Champ disposition="ligne" libelle="Total HT" valeur="1 841,76 €" mono modifiable />
            <Champ disposition="ligne" libelle="TVA" valeur="368,35 €" indice="20 %" mono modifiable />
            <Champ disposition="ligne" libelle="Total TTC" bind:valeur={ttc} mono modifiable aControler={courant.id === 1}>
              {#snippet affichage()}<span class="num">{ttc} €</span>{/snippet}
              {#snippet apres()}<button class="btn sm" onclick={() => (ttc = '2 210,11')}>Mettre 2 210,11 €</button>{/snippet}
            </Champ>
          </div>
          <div class="replis">
            <Repli icone="camion" titre="BL liés" resume="2 liés, 1 à lier, écart de 24 serpillières">
              <table>
                <thead><tr><th>BL</th><th>Livré le</th><th>Lien</th><th class="n">Serpillières</th><th></th></tr></thead>
                <tbody>
                  <tr><td class="num">4471203</td><td class="num">07/11</td><td class="t2">{libelleMethode('numero_bl')}</td><td class="n num">48</td><td class="n"><button class="btn sm fantome">Délier</button></td></tr>
                  <tr><td class="num">4471288</td><td class="num">12/11</td><td class="t2">{libelleMethode('numero_bl')}</td><td class="n num">48</td><td class="n"><button class="btn sm fantome">Délier</button></td></tr>
                  <tr class="cand"><td class="num">4471310</td><td class="num">13/11</td><td class="t2">même commande 0458-1126</td><td class="n num">24</td><td class="n"><button class="btn sm">Lier <Kbd t="L" /></button></td></tr>
                </tbody>
              </table>
              <div class="encadre" style="margin-top: 12px"><Icone nom="alerte" class="ambre" /><div><b>Serpillière microfibre : 120 facturées, 96 livrées</b> sur les deux BL liés. Le BL 4471310 en apporte 24 : le lier règle l’écart.</div></div>
            </Repli>
            <Repli icone="envoyer" titre="Envoi à Dext" resume="Dext Frouard, pas encore envoyée"><p class="t2">Aucun envoi pour l’instant.</p></Repli>
            <Repli icone="mail" titre="Mail d’origine" resume="factures@vileda-retail.fr, 14/11 08:57"><p class="t2">Objet : Facture F2611-08734</p></Repli>
          </div>
        </div>
      </div>
      <BarreAction
        principal={{ libelle: libelleValider('facture', 'dext'), touche: 'Enter', onclick: valider }}
        secondaires={[{ libelle: 'Écarter', touche: 'E', onclick: () => (ecarter = true) }]}
        ensuite={suivant ? `${suivant.nom}, ${suivant.type.toLowerCase()} ${suivant.numero}` : undefined}
      />
    {/snippet}

    {#snippet vide()}
      <Vide icone="coche" titre="Tout est à jour" texte="Aucun document à valider ni mail à répondre." />
    {/snippet}
  </Volets>

  <Confirmation bind:ouvert={ecarter} titre="Écarter ce document ?" libelle="Écarter" danger onconfirmer={() => { indice = deplacer(indice, 1, aValider.length); }}>
    <p>{courant?.type} {courant?.nom} ne sera pas envoyée à Dext. Elle reste visible dans la recherche.</p>
  </Confirmation>
{:else}
  <div class="page">
    <div class="page-h">
      <h1>Socle de l’interface</h1>
      <span class="sp"></span>
      <a class="btn" href="/_demo"><Icone nom="gauche" />Écran À traiter</a>
    </div>
    <p class="lead">Composants partagés et classes de app.css, avec des données fictives. Mode d’emploi : docs/ui.md.</p>

    <div class="grille-demo">
      <section>
        <h2>Boutons</h2>
        <div class="rang">
          <button class="btn principal">Valider et envoyer à Dext <Kbd t="Enter" /></button>
          <button class="btn">Écarter <Kbd t="E" /></button>
          <button class="btn fantome">Proposer une autre</button>
          <button class="btn danger">Supprimer la boîte</button>
        </div>
        <div class="rang">
          <button class="btn sm principal">Commencer</button>
          <button class="btn sm">Renvoyer</button>
          <button class="btn sm fantome"><Icone nom="telecharger" />PDF</button>
          <button class="btn sm" disabled>Désactivé</button>
          <button class="btn sm en-cours">Envoi</button>
          <button class="btn-icone" aria-label="Réglages"><Icone nom="reglages" /></button>
          <button class="btn-lien">Voir comment</button>
        </div>
        <div class="rang">
          <Kbd t="J" /><Kbd t="K" /><Kbd t="Enter" /><Kbd t="Escape" /><Kbd t="/" /><Kbd t={['Ctrl', 'Enter']} />
        </div>
      </section>

      <section>
        <h2>Étiquettes et montants</h2>
        <div class="rang">
          <Etiquette ton={tonStatutEnvoi('a_verifier')}>{libelleStatutEnvoi('a_verifier')}</Etiquette>
          <Etiquette ton={tonStatutEnvoi('pret')}>{libelleStatutEnvoi('pret')}</Etiquette>
          <Etiquette ton={tonStatutEnvoi('depose')} point>{libelleStatutEnvoi('depose', 'yooz')}</Etiquette>
          <Etiquette ton={tonStatutEnvoi('erreur')} point>{libelleStatutEnvoi('erreur')}</Etiquette>
          <Etiquette ton="info">Nouveau fournisseur</Etiquette>
          <Etiquette ton="pale">Écarté (pas une facture)</Etiquette>
        </div>
        <table>
          <thead><tr><th>Fournisseur</th><th>N°</th><th>Date</th><th class="n">TTC</th></tr></thead>
          <tbody>
            <tr><td>Vileda</td><td class="num">F2611-08734</td><td class="num">14/11/2026</td><td class="n"><Montant valeur={2210.11} /></td></tr>
            <tr><td>Arc France</td><td class="num">26FA02104</td><td class="num">17/11/2026</td><td class="n"><Montant valeur={4118.4} /></td></tr>
            <tr><td>Spontex</td><td class="num">AV-55203</td><td class="num">19/11/2026</td><td class="n"><Montant valeur={-312} /></td></tr>
            <tr><td>Duralex</td><td class="num">7781104</td><td class="num">20/11/2026</td><td class="n"><Montant valeur={null} /></td></tr>
          </tbody>
        </table>
      </section>

      <section>
        <h2>Bandeaux</h2>
        <Bandeau titre="Montant incohérent.">HT + TVA donnent 2 210,11 €, le total lu est 2 270,11 €.</Bandeau>
        <Bandeau ton="erreur" titre="Envoi échoué.">Dext a refusé le mail, pièce jointe de 14 Mo.
          {#snippet action()}<button class="btn sm">Renvoyer</button>{/snippet}
        </Bandeau>
        <Bandeau ton="ok" role="status">Boîte Infomaniak reconnue, serveurs préremplis.</Bandeau>
        <Bandeau ton="info">La relève reprend toutes les 5 minutes.</Bandeau>
      </section>

      <section>
        <h2>Formulaire</h2>
        <div class="formulaire">
          <Champ libelle="Adresse" bind:valeur={nom} type="email" aide="L’adresse où arrivent les factures et les mails des fournisseurs." />
          <Champ libelle="Mot de passe d’application" bind:valeur={mdp} type="password" aControler="Pas le mot de passe habituel de la boîte." />
          <div class="plein"><Champ libelle="Société" type="select" valeur="2" options={[{ valeur: '1', libelle: 'SAS Frouard Distribution' }, { valeur: '2', libelle: 'SAS Houdemont Distribution' }]} /></div>
          <label class="case plein"><input type="checkbox" checked /> Envoi automatique des factures sûres</label>
          <div class="rang plein">
            <button class="btn principal">Suivant : tester la connexion</button>
            <button class="btn fantome">Annuler</button>
          </div>
        </div>
      </section>

      <section>
        <h2>Volets, liste à filets</h2>
        <div class="filets">
          <a class="ligne" href="/_demo"><div class="a">Arc France</div><div class="r"><Montant valeur={4118.4} /></div><div class="b">3 factures ce mois, 1 BL en attente de facture</div><div class="r2">17/11</div></a>
          <a class="ligne" href="/_demo"><div class="a">Sodistra</div><div class="r"><Montant valeur={986.4} /></div><div class="b">1 facture ce mois</div><div class="r2">hier</div></a>
        </div>
      </section>

      <section>
        <h2>États</h2>
        <div class="etats">
          <Vide icone="coche" titre="Tout est à jour" texte="Aucun document à valider ni mail à répondre." />
          <Chargement texte="Chargement des documents" />
        </div>
        <div class="rang">
          <button class="btn danger" onclick={() => (ouvrirConfirmation = true)}>Ouvrir une confirmation</button>
        </div>
      </section>

      <section class="large">
        <h2>Aperçu d’une pièce (repli sans aperçu)</h2>
        <div class="apercu-demo"><ApercuPiece id={0} typeMime="text/csv" nom="releve-octobre.csv" /></div>
      </section>
    </div>

    <Confirmation
      bind:ouvert={ouvrirConfirmation}
      titre="Supprimer la boîte factures@frouard-distribution.fr ?"
      libelle="Supprimer"
      danger
      onconfirmer={async () => {
        enCours = true;
        await new Promise((r) => setTimeout(r, 400));
        enCours = false;
      }}
    >
      <p>Les mails déjà relevés restent dans GMFlow. La relève s’arrête tout de suite.</p>
    </Confirmation>
    {#if enCours}<Chargement compact />{/if}
  </div>
{/if}

<style>
  .zoom {
    zoom: 0.7;
    display: flex;
  }
  .grille-demo {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--e8) var(--e10);
  }
  .grille-demo section h2 {
    margin-bottom: var(--e3);
  }
  .grille-demo .large {
    grid-column: 1 / -1;
  }
  .rang {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--e2);
    margin-bottom: var(--e3);
  }
  .etats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border: 1px solid var(--line);
    border-radius: var(--r3);
    margin-bottom: var(--e3);
    background: var(--white);
  }
  .apercu-demo {
    height: 280px;
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--r3);
    background: var(--sunk);
    overflow: hidden;
  }
  :global(.ambre) {
    color: var(--warn);
    margin-top: 2px;
  }
  @media (max-width: 760px) {
    .grille-demo {
      grid-template-columns: 1fr;
    }
    .zoom {
      zoom: 0.56;
    }
  }

  /* fausse facture (comme la maquette) */
  .paper {
    width: 595px;
    flex: none;
    background: #fff;
    color: #222;
    font: 9.5px/1.45 'Helvetica Neue', Arial, sans-serif;
    padding: 40px 40px 32px;
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.08),
      0 4px 16px rgba(0, 0, 0, 0.06);
    align-self: flex-start;
  }
  .paper .ph {
    display: flex;
    justify-content: space-between;
  }
  .paper .vl {
    font: 700 30px/1 Arial, sans-serif;
    color: #d4001e;
  }
  .paper .vl small {
    display: block;
    font: 400 8.5px/1.4 Arial, sans-serif;
    color: #666;
    margin-top: 6px;
  }
  .paper .ft {
    font-size: 20px;
    font-weight: 700;
    text-align: right;
  }
  .paper .meta {
    display: grid;
    grid-template-columns: auto auto;
    gap: 1px 12px;
    margin-top: 6px;
    font-size: 9px;
  }
  .paper .addr {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
    margin: 28px 0 20px;
  }
  .paper .addr div {
    border: 1px solid #ddd;
    padding: 8px 10px;
  }
  .paper .addr em {
    font-style: normal;
    color: #777;
    display: block;
  }
  .paper table {
    font-size: 9px;
  }
  .paper th {
    background: #f1f1f1;
    font-weight: 700;
    color: #222;
    font-size: 9px;
    padding: 5px 6px;
    border-bottom: 1px solid #ccc;
  }
  .paper td {
    padding: 5px 6px;
    border-bottom: 1px solid #eee;
  }
  .paper .r {
    text-align: right;
  }
  .paper .tot {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }
  .paper .tot table {
    width: 220px;
  }
  .paper .tot td {
    border: 0;
    padding: 3px 6px;
  }
  .paper .tot tr.ttc td {
    font-weight: 700;
    font-size: 11px;
    border-top: 1.5px solid #222;
  }
</style>
