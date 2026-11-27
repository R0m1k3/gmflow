# GMFlow : socle de l'interface (mode d'emploi pour les pages)

Référence visuelle : `docs/maquette-sauge/` (piste « Sauge » : P3 accueil, S1 à S6 écrans, HTML + PNG). Styles : `web/src/app.css` ; composants : `web/src/lib/ui/` (import `#lib/ui/Nom.svelte`).

## Règles de mise en page

- **Un écran, une tâche, un bouton principal** (`.btn.principal`, vert sauge #2D6A4F, 44 px). Le reste : `.btn` (secondaire), `.btn.fantome` (discret), `.btn.danger`. Taille `.sm` dans les listes et en-têtes.
- **Raccourcis affichés** dans les boutons (`<Kbd t="Enter" />`) et posés avec `raccourcis()`. Masqués sur téléphone, automatiquement.
- **Montants, numéros, dates, compteurs en mono** : `<Montant valeur={x} />` ou `<span class="num">`. Jamais `toFixed`.
- **Filets plutôt que cartes** : listes `.ligne` séparées par des filets, sections `.sec`, blocs repliés `<Repli>`. Pas de cartes empilées, pas d'ombre, un état = une étiquette (`<Etiquette>`) : pastille saturée pour ok, à contrôler, erreur, info ; texte seul pour le neutre.
- **Couleurs à sens unique** : vert sauge = action principale et liens ; sélection sur fond `--sunk` (#E9EDE6) ; ambre = à contrôler ; rouge = bloqué ou urgent ; vert = fait. Le reste en neutres (`--text`, `--text2`, `--muted`).
- **Contraste et lisibilité** (refonte « lisibilité », 2026-11-27) : police Source Sans 3 variable (déclarée 200..900, comme le fichier), texte de base 16 px en **450** (15 px pour les métadonnées des listes), titre de ligne et montant en 600. `--text` #1C2520 (15.6:1), `--text2` #3A4540 (10:1), `--muted` #5E6962 (5.8:1) sur blanc ; pas d'autre gris pour du texte. Fond de l'application, barre latérale et aperçu sur `--bg`/`--pane` (#F4F5F3), colonne de travail blanche bordée de `--line` (#DDE2DA), filets entre lignes `--line-faint` (#E6EAE3). Listes en volet : tuiles pleine largeur séparées par un filet, tuile choisie sur `--sunk` avec une barre d'accent à gauche (`box-shadow: inset 3px 0 0 var(--accent)`) ; lien actif du menu = tuile blanche + même barre d'accent. Étiquettes 14 px en 500, rayon 6 (`--warn-p`/`--ok-p` + `*-ink`). Cartes blanches rayon 16 (`.carte`), encadrés rayon 12 (`.encadre`). Désactivé : `--desact` sur `--desact-bg`, jamais `opacity` sur du texte.
- **Vocabulaire** : tout libellé métier passe par `#lib/libelles.ts` (voir plus bas). Pas de point final sur titres, boutons, étiquettes ; pas de point d'exclamation.
- **Impression** : `@media print` d'`app.css` retire barre latérale, barre du haut, onglets, pieds de liste et tout `.pas-imprime` (boutons) ; `.imprime-seul` n'apparaît qu'à l'impression. Le bilan du mois ajoute ses règles (pleine largeur, filets, sans la partie Qualité) et s'exporte aussi en Excel (`GET /api/bilan/export`).
- **Racine de la page** : `<Volets>` (file + détail), `<div class="pleine">` (fiche plein écran) ou `<div class="page">` (réglages, bilan, fournisseur ; `.page.etroite` pour un formulaire).
- **La page ne défile pas, les volets oui** (ordinateur et tablette, > 760 px) : sous `<Volets>` ou `.pleine.fixe`, `main.contenu` a la hauteur de l'écran moins la barre du haut et coupe ce qui dépasse (`overflow: clip`, `+layout.svelte`) ; seuls la liste (`.liste-s`), la fiche et l'aperçu défilent, chacun dans sa colonne. Toute zone qui défile est `position: relative` : sinon un élément en absolu qu'elle contient (`.sr-seul`, `.sr`, ancrés en `top: 0; left: 0`) se place par rapport à la page et l'allonge (cause de l'ascenseur « sur du vide » de Factures et BL, corrigée le 2026-11-27). Pages longues (Accueil, Bilan, Recherche, fiche fournisseur) : la fenêtre défile, une seule fois. Téléphone : défilement normal de la page.
- Grille de 4 px : utiliser les jetons `--e1`… `--e10` (4 à 40 px), rayons `--r1/--r2/--r3` (4, 6, 8).

## Structure des écrans

```svelte
<Volets ouvert={!!courant}>
  {#snippet liste()}
    <div class="liste-h"><h1>À traiter</h1><span class="num">19</span></div>
    <div class="liste-s">
      <section class="sec" id="bloque">           <!-- id visé par la pastille d'état : /#bloque -->
        <div class="sec-h erreur"><span class="point"></span><h2>Bloqué</h2><span class="num">2</span></div>
        <div class="ligne bloc">…<div class="r"><button class="btn sm">Renvoyer</button></div></div>
      </section>
      <section class="sec">
        <div class="sec-h"><h2>À valider</h2><span class="num">12</span><button class="btn sm">Commencer <Kbd t="Enter" /></button></div>
        <a class="ligne" class:cur={…} href="/?doc=4"><div class="a">Vileda</div><div class="r"><Montant valeur={2210.11} /></div>
          <div class="b">Facture <span class="num">F2611-08734</span>, Frouard <span class="flag w">Montant incohérent</span></div><div class="r2">8 j</div></a>
      </section>
    </div>
    <div class="liste-f"><span><Kbd t="J" /><Kbd t="K" /> suivant, précédent</span></div>
  {/snippet}
  {#snippet detail()}
    <div class="volet-h"><a class="retour tel-seul" href="/"><Icone nom="gauche" />À traiter</a><span class="pos">À valider <span class="num">1</span> sur <span class="num">12</span></span><span class="sp"></span>…</div>
    <div class="volet-b">
      <div class="apercu-zone"><ApercuPiece id={d.piece_id} typeMime={d.type_mime} nom={d.nom_fichier} /></div>
      <div class="contenu-fiche avec-barre">
        <div class="titre-fiche"><h1>Facture Vileda</h1><span class="num">F2611-08734</span></div>
        <div class="meta-fiche">Reçue le 14/11 par …</div>
        <Bandeau titre="Montant incohérent.">HT + TVA donnent …</Bandeau>
        <div class="lignes-champs"><Champ disposition="ligne" … /></div>
        <div class="replis"><Repli icone="camion" titre="BL liés" resume="2 liés, 1 à lier">…</Repli></div>
      </div>
    </div>
    <BarreAction principal={{ libelle: libelleValider(d.type, d.societe_outil_depot), touche: 'Enter', onclick: valider }}
      secondaires={[{ libelle: 'Écarter', touche: 'E', onclick: … }]} ensuite="Sodistra, facture 2026/11/0457" />
  {/snippet}
  {#snippet vide()}<Vide icone="coche" titre="Tout est à jour" />{/snippet}
</Volets>
```

**Tableau pleine largeur** (Factures, Avoirs, BL, Envois via `ListeDocuments`, et Mails ; ordinateur ≥ 1024 px, rien d'ouvert) : `<Volets tableau={large}>` donne toute la largeur à la liste ; en tête `.liste-h.tete-tableau` = titre 26 px + filtres en sélecteur segmenté `.seg` (une rangée, défile en largeur s'il le faut) ; puis `.liste-s.t-defile` > `.tableau` (grille, `--gabarit` = colonnes) avec `<EnTetesTableau>` (en-têtes collants qui **sont** le tri : `#lib/tri.ts`, colonne active en `--accent-ink` avec ▾/▴), intertitres de jour `.t-jour` (fond `--bg`, 600), rangées `.t-ligne` > `a.t-lien` (sous-grille, ~50 px, filet `--line-faint`, première colonne en 600, montant à droite en 600 chiffres tabulaires) et case de validation par lot en première colonne (filtre « À contrôler »). L'étiquette d'état est tue quand elle répète le filtre (« À contrôler » sous « À contrôler »). Sous le tableau, `.t-fin` (fond `--pane`) remplit le bas : « N pièces · tout est affiché » ou « Afficher les plus anciens ». Pied `.liste-f` : J K se déplacer · ↵ ouvrir à gauche, ? raccourcis à droite. Pas de colonne BL n/m : la liste ne porte pas cette donnée. À répondre et Fournisseurs restent en liste + volet vide (tuiles sur plusieurs lignes : résumé, priorité, brouillon ; recherche et onglets propres).

**Pièce ouverte** : liste étroite (320 px, clé de largeur `gmflow.largeur.volet.<titre>`) | **fiche au milieu** (560 px, `fiche.milieu` ; 600 px en plein écran, `fiche.milieu.pleine`) | **aperçu PDF ou tableur à droite**, qui prend le reste ; poignées entre chaque colonne (l'aperçu garde 360 px au moins). Actions (Valider et envoyer…, Écarter) collées en bas de la fiche (`BarreAction`). Sous 1600 px de large, la liste se replie d'elle-même en un rail de 48 px (bouton pour la rouvrir, « ‹ » en tête de liste pour la replier ; choix gardé par page jusqu'au rechargement) ; J/K marchent toujours. Dans la liste étroite, le filtre est une liste déroulante sous le titre (le changer ferme la pièce). Sous 1200 px, l'aperçu passe au-dessus de la fiche.

Ligne de liste : `.a` titre, `.b` sous-ligne, `.r` à droite (montant), `.r2` âge ; `.cur` (ou `aria-current="true"`) = sélection ; `.bloc` pour une ligne avec bouton. `.flag.w` ambre / `.flag.e` rouge pour l'alerte de la ligne, `.plus` pour « 5 autres ». Hors volets : `<div class="filets">` autour des `.ligne`. Fiche plein écran : `<div class="pleine fixe">` + `.volet-h`, `.volet-b`, et `<BarreAction aligne>` pour caler les boutons sur la colonne des champs. Téléphone (≤ 760 px) : un seul volet à la fois (`ouvert` montre le détail), l'aperçu passe au-dessus (268 px), la barre d'action se fixe en bas (mettre `.avec-barre` sur le contenu qui défile), les onglets du bas disparaissent. Utilitaires : `.tel-seul`, `.large-seul`, `.sp` (ressort), `.vsep`, `.t2`, `.muted`, `.petit`, `.sr`.

## Composants (`#lib/ui/`)

| Composant | Usage |
|---|---|
| `Volets` | `ouvert`, `tableau?` (rien d'ouvert : la liste prend toute la largeur, sans volet vide), `largeur?` (px, 320 par défaut), snippets `liste`, `detail`, `vide`. Replie la liste sous 1600 px quand un détail est ouvert. |
| `EnTetesTableau` | En-têtes collants d'un `.tableau` qui trient : `colonnes` (`libelle`, `tri?`, `droite?`), `colonnesTri`, `tri`, `retirer`, `caseLot`. |
| `BarreAction` | `principal: Action`, `secondaires?: Action[]`, `ensuite?`, `toucheEnsuite?`, `aligne?`. `Action = { libelle, touche?, onclick?, href?, desactive?, enCours?, danger? }`. |
| `Champ` | `disposition="bloc"` (formulaire : `libelle`, `bind:valeur`, `type` text/email/password/date/number/select/textarea, `options`, `aide`, `requis`) ou `"ligne"` (fiche : `modifiable` = clic pour saisir, Entrée valide via `onvalider(v)`, Échap annule ; `mono`, `indice` « échéance… », `source` « comme les 14 dernières », snippets `affichage` et `apres`). `aControler` (booléen ou phrase) = surlignage ambre ; `erreur` (bloc) = valeur refusée, message rouge sous la saisie (ex. `ErreurApi.champ` d'un 400). Formulaire sur deux colonnes : `<div class="formulaire">`, `.plein` pour une ligne entière ; case à cocher : `<label class="case"><input type="checkbox"> …</label>`. |
| `Montant` | `valeur`, `devise?` ('€', '' pour rien). `formatMontant()` exporté pour un texte (« 2 210,11 € », « −312,00 € », « — » si nul). |
| `Etiquette` | `ton`: neutre, ok, alerte, erreur, info, pale ; `point?`. Associer avec `ton*()` de `libelles.ts`. |
| `Bandeau` | `ton`: attention (défaut), erreur, ok, info, neutre ; `titre?` en gras ; snippet `action` (un `.btn.sm`). Une phrase, en tête de zone. |
| `Repli` | `titre`, `resume?` (une ligne à droite), `icone?`, `bind:ouvert`. Grouper dans `<div class="replis">`. |
| `ApercuPiece` | `id` (pièce), `typeMime`, `nom`, `outils?`. PDF en iframe, image en img, sinon phrase + Télécharger. Source `/api/pieces/{id}?inline=1`. |
| `Confirmation` | `<dialog>` natif : `bind:ouvert`, `titre`, `libelle`, `danger?`, `desactive?` (bouton grisé), `onconfirmer` (async : bouton occupé pendant l'appel), contenu en enfant. |
| `Vide` | `titre`, `texte?`, `icone?`, `compact?`, enfants = actions. Ex. « Tout est à jour ». |
| `Chargement` | `texte?`, `compact?` ; n'apparaît qu'après 300 ms. Erreur de chargement : `<Bandeau ton="erreur">` + bouton « Réessayer ». |
| `CorpsHtml` | `id` (mail), `expediteur?`. Corps HTML d'origine en iframe sandbox ; images distantes masquées (bandeau « Images distantes masquées · Afficher · Toujours pour cet expéditeur », `imagesDistantes.ts`, localStorage `gmflow.images.expediteurs`). |
| `MonMotDePasse` | Formulaire autonome « changer mon mot de passe » (`POST /api/moi/mot-de-passe`), messages sous chaque champ ; page `/compte`. |
| `NotesInternes` | `documentId`, `onchange?(n)`. Carte « Notes internes » de la fiche (U23) : notes signées et datées, ajout (Ctrl+Entrée), « Supprimer » si `supprimable`. API dans `#lib/notes.ts`, outils testés dans `NotesOutils.ts` (`bulleNotes`, `resumeEcartLot`). |
| `ChoixMotifEcart` | `bind:motif`, `bind:texte`. Motif d'« Écarter » dans une `<Confirmation desactive={!motif}>` (accueil, lot ; la fiche a le même choix dans `DocFiche`) : `MOTIFS_ECARTEMENT` + précision facultative. |
| `Kbd` | `t="J"`, `t="Enter"` (↵), `t={['Ctrl', 'Enter']}`. |
| `Icone` | `nom` : chercher, reglages, bas, droite, gauche, haut, alerte, erreur, info, coche, trombone, fichier, agrandir, zoom, telecharger, mail, boite, camion, graphique, batiment, prise, envoyer, utilisateurs, curseurs, drapeau, lien, menu, fermer, sortir, ouvrir, plus, rafraichir. 16 px, trait 1,5, couleur du texte. |
| `ErreurAction` | `erreur` (attrapée), `titre?`, `onreessayer?`, `compact?`. Bandeau d'erreur traduit par `#lib/erreurs.ts` (code stable du serveur → message simple, « que faire », bouton Réessayer / Réglages / Se reconnecter). Une seule ligne de texte : `message(e)` de `api.ts` donne la même traduction. |
| `EnTetesTri` | colonnes `ColonneTri[]` (`#lib/tri.ts`) : « Trier par » en liens qui changent `?tri=&sens=` dans l'adresse (le tri par défaut n'y apparaît pas) ; le serveur trie. |
| `Origine` | `origine` (`origineChamp()` de `#lib/origine.ts`), `libelle`, `formater?`. Pastille à côté d'un champ de la fiche : ✓ lue sûre, ? lue avec doute (carré ambre), ✎ corrigée (pleine), = calculée (pointillée), • modifiée pas enregistrée ; au clic, qui a corrigé quoi et quand. Une forme par origine, jamais la couleur seule. |

## Modules

- **`#lib/clavier.ts`** : `$effect(() => raccourcis({ j: suivant, k: precedent, Enter: valider, Escape: fermer, e: ecarter, l: lier, 'Ctrl+Enter': envoyer }))`. Ignoré quand on tape dans un champ (sauf `Escape` et `Ctrl+Enter`, réglable par `dansLesChamps`), quand un dialogue est ouvert, et pour Entrée sur un bouton ou lien qui a le focus. `deplacer(i, ±1, n)` et `montrer(conteneur, sélecteur)` pour J/K. `/` est déjà pris par la recherche de la barre du haut.
- **`#lib/libelles.ts`** : vocabulaire de l'architecture §4. `nomOutil`, `titreEnvoi`, `libelleValider(type, outil)` ; `libelleType`, `libelleEtat`/`tonEtat`, `libelleStatutEnvoi(s, outil)`/`tonStatutEnvoi`, `libelleVoie` (admins), `titreAlerte`/`tonAlerte` (contrôles) ; `libelleMethode`, `titreLiens`, `BL_EN_ATTENTE`, `FACTURE_SANS_BL` ; `libelleCategorie`, `libelleStatutReponse`/`tonStatutReponse`, `libellePriorite` (« Urgent »), `libelleBrouillon`, `ACTIONS_REPONSE`, `libelleEnvoyerDepuis`, `libelleTriePar` ; `RUBRIQUES` (réglages), `libelleTache`, `libelleRole`, `nomCourtSociete`.
- **`#lib/societe.svelte.ts`** : `societe.id` (null = toutes), `societe.courante`, `societe.liste`, `societe.url('/api/…')` ajoute `societe=` ; lire `societe.id` dans un `$effect` recharge la page au changement. Responsable : toujours `null` (le serveur filtre).
- **`#lib/compteurs.svelte.ts`** : `compteurs.aTraiter`, `compteurs.problemes` (barre du haut) ; appeler `rafraichirCompteurs()` après une validation, un envoi ou une réponse.

Les ajouts propres à une page vont dans son `<style>` ; un style servant à deux pages ou plus va dans `app.css`.

## Navigation (2026-11-23, barre latérale)

Barre du haut (`--haut-barre` : 72 px sur ordinateur, 60 px sous 1024 px) : la recherche (`/`) seule. Ordinateur (≥ 1024 px) : barre latérale fixe à gauche (`--larg-lateral` : 256 px, 232 px sous 1400 px) : logo, société, puis les liens par groupes (pastille d'étape 22 px sur `--accent-weak`, titre de groupe 13 px en 600, liens en `--text` sans icône sous leur titre, compteurs en pastille, ambre sur la page ouverte), et en bas, collés : l'état (« Tout fonctionne » sur `--ok-p`, ou le problème sur `--err-p` ; admins), **Réglages** (admins) et le compte (initiales + prénom ; menu vers le haut : Mon compte `/compte`, Réglages, déconnexion). Les liens : **Accueil** (`/`, tableau de bord : chiffres du jour, derniers mails, à répondre, dernières factures et derniers BL) ; *Courrier* : **Mails** (`/mails?filtre=&mail=ID`), **À répondre** (`/a-repondre?mail=ID`) ; *Pièces* : **Factures** (`/factures?filtre=&doc=ID`), **Avoirs** (`/avoirs`, mêmes filtres ; l'ancien `/factures?filtre=avoirs` y redirige), **Bons de livraison** (`/bl`) ; badge du nombre à valider sur Factures, Avoirs et BL (même définition que le filtre « À valider », `verifie=non` ; avec un badge, le lien ouvre ce filtre), et sur À répondre ; plus de page À valider (`/a-traiter` redirige, 2026-11-25) : les blocages (boîte, Claude, envoi échoué, doublon, illisible) sont sur l'Accueil (`/#bloque`, lien de la pastille d'état) ; depuis l'étape F (2026-11-27) les groupes sont numérotés dans l'ordre du trajet d'une pièce, *1 Courrier*, *2 Pièces*, *3 Fournisseurs* : **Fournisseurs** (`/fournisseurs`), **À confirmer** (`/fournisseurs?onglet=nouveaux`, badge du nombre ; l'onglet de la page est dans l'adresse) ; *4 Compta* : **Envois Dext / Yooz** (`/envois`, factures et avoirs par `statut_depot` : échec, envoyées, doublons ; pastille rouge et lien vers `?filtre=echec` s'il y a des échecs, qui ne sont plus signalés sur Factures et Avoirs), **Export Phénix** (`/bl?filtre=valides` : l'export se fait sur la fiche d'un BL validé), **Bilan du mois**. Entre deux liens de la même page, celui qui porte des paramètres est le seul allumé (`actif` du layout). Écran de moins de 860 px de haut : liens de 32 px et écarts réduits pour que tout tienne sans défiler. Tablette (< 1024 px) : bouton menu et logo dans la barre du haut, loupe à la place du champ, pastille rouge de l'état s'il y a un problème, la barre latérale en tiroir (bouton menu, Échap ou voile pour fermer), avec l'état, Réglages et le compte en bas. Téléphone (≤ 760 px) : en plus, Accueil, Mails, Répondre, Factures, BL en onglets en bas. Seuils de largeur des volets (1600, 1400 et 1200 px) comptés barre latérale comprise. Chaque page de liste : `<Volets>`, liste groupée par jour (`parJour` de `#lib/liste.svelte.ts`), filtres en `.seg` (tableau) ou `.puces` (tablette, téléphone), fiche à droite. Listes paginées : classe `Liste<T>` (`lire(garder)`, `plus()`). Factures, Avoirs et BL partagent `#lib/ui/ListeDocuments.svelte`.

## Phrase d'état de la fiche

En tête de la fiche, une seule phrase : celle du problème le plus grave, dans la couleur de sa gravité
(rouge : bloque l'envoi ou risque d'argent ; orange : à vérifier ; gris bordé : en cours ; vert : tout va
bien ; gris : pièce sortie du circuit). Les autres alertes suivent dans une liste « Aussi : … », avec
leur bouton. Aucune alerte n'est retirée.

- `DocFiche.svelte` (`phrasesEtat`) dit quelles phrases s'appliquent, avec les conditions des anciens
  bandeaux ; `web/src/lib/ui/phraseEtat.ts` (`ORDRE`, `rangerPhrases`) les range. Testé par `npm test`.
- Une pièce qui n'est plus à contrôler (en lecture, validée, envoyée, écartée) annonce d'abord son état.
- Le « tout va bien » ne s'affiche que sans alerte rouge ou orange.
- « Aucun BL trouvé » est orange si la facture cite des numéros de BL, sinon simple information
  (« Aucun BL rattaché ») : une facture de service n'a pas de BL.
- Un refus du serveur (après un clic) reste un bandeau à part, au-dessus.
