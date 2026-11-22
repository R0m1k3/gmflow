# GMFlow : socle de l'interface (mode d'emploi pour les pages)

Référence visuelle : `docs/maquette-b.html`. Styles : `web/src/app.css` ; composants : `web/src/lib/ui/` (import `#lib/ui/Nom.svelte`).

## Règles de mise en page

- **Un écran, une tâche, un bouton principal** (`.btn.principal`, bleu tampon). Le reste : `.btn` (secondaire), `.btn.fantome` (discret), `.btn.danger`. Taille `.sm` dans les listes et en-têtes.
- **Raccourcis affichés** dans les boutons (`<Kbd t="Enter" />`) et posés avec `raccourcis()`. Masqués sur téléphone, automatiquement.
- **Montants, numéros, dates, compteurs en mono** : `<Montant valeur={x} />` ou `<span class="num">`. Jamais `toFixed`.
- **Filets plutôt que cartes** : listes `.ligne` séparées par des filets, sections `.sec`, blocs repliés `<Repli>`. Pas de cartes empilées, pas d'ombre, pas de pastille colorée : un état = du texte coloré (`<Etiquette>`).
- **Couleurs à sens unique** : bleu = action principale et sélection ; ambre = à contrôler ; rouge = bloqué ou urgent ; vert = fait. Le reste en neutres (`--text`, `--text2`, `--muted`).
- **Vocabulaire** : tout libellé métier passe par `#lib/libelles.ts` (voir plus bas). Pas de point final sur titres, boutons, étiquettes ; pas de point d'exclamation.
- **Racine de la page** : `<Volets>` (file + détail), `<div class="pleine">` (fiche plein écran) ou `<div class="page">` (réglages, bilan, fournisseur ; `.page.etroite` pour un formulaire).
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

Ligne de liste : `.a` titre, `.b` sous-ligne, `.r` à droite (montant), `.r2` âge ; `.cur` (ou `aria-current="true"`) = sélection ; `.bloc` pour une ligne avec bouton. `.flag.w` ambre / `.flag.e` rouge pour l'alerte de la ligne, `.plus` pour « 5 autres ». Hors volets : `<div class="filets">` autour des `.ligne`. Fiche plein écran : `<div class="pleine fixe">` + `.volet-h`, `.volet-b`, et `<BarreAction aligne>` pour caler les boutons sur la colonne des champs. Téléphone (≤ 760 px) : un seul volet à la fois (`ouvert` montre le détail), l'aperçu passe au-dessus (268 px), la barre d'action se fixe en bas (mettre `.avec-barre` sur le contenu qui défile), les onglets du bas disparaissent. Utilitaires : `.tel-seul`, `.large-seul`, `.sp` (ressort), `.vsep`, `.t2`, `.muted`, `.petit`, `.sr`.

## Composants (`#lib/ui/`)

| Composant | Usage |
|---|---|
| `Volets` | `ouvert`, `largeur?` (px), snippets `liste`, `detail`, `vide`. |
| `BarreAction` | `principal: Action`, `secondaires?: Action[]`, `ensuite?`, `toucheEnsuite?`, `aligne?`. `Action = { libelle, touche?, onclick?, href?, desactive?, enCours?, danger? }`. |
| `Champ` | `disposition="bloc"` (formulaire : `libelle`, `bind:valeur`, `type` text/email/password/date/number/select/textarea, `options`, `aide`, `requis`) ou `"ligne"` (fiche : `modifiable` = clic pour saisir, Entrée valide via `onvalider(v)`, Échap annule ; `mono`, `indice` « échéance… », `source` « comme les 14 dernières », snippets `affichage` et `apres`). `aControler` (booléen ou phrase) = surlignage ambre. Formulaire sur deux colonnes : `<div class="formulaire">`, `.plein` pour une ligne entière ; case à cocher : `<label class="case"><input type="checkbox"> …</label>`. |
| `Montant` | `valeur`, `devise?` ('€', '' pour rien). `formatMontant()` exporté pour un texte (« 2 210,11 € », « −312,00 € », « — » si nul). |
| `Etiquette` | `ton`: neutre, ok, alerte, erreur, info, pale ; `point?`. Associer avec `ton*()` de `libelles.ts`. |
| `Bandeau` | `ton`: attention (défaut), erreur, ok, info, neutre ; `titre?` en gras ; snippet `action` (un `.btn.sm`). Une phrase, en tête de zone. |
| `Repli` | `titre`, `resume?` (une ligne à droite), `icone?`, `bind:ouvert`. Grouper dans `<div class="replis">`. |
| `ApercuPiece` | `id` (pièce), `typeMime`, `nom`, `outils?`. PDF en iframe, image en img, sinon phrase + Télécharger. Source `/api/pieces/{id}?inline=1`. |
| `Confirmation` | `<dialog>` natif : `bind:ouvert`, `titre`, `libelle`, `danger?`, `onconfirmer` (async : bouton occupé pendant l'appel), contenu en enfant. |
| `Vide` | `titre`, `texte?`, `icone?`, `compact?`, enfants = actions. Ex. « Tout est à jour ». |
| `Chargement` | `texte?`, `compact?` ; n'apparaît qu'après 300 ms. Erreur de chargement : `<Bandeau ton="erreur">` + bouton « Réessayer ». |
| `Kbd` | `t="J"`, `t="Enter"` (↵), `t={['Ctrl', 'Enter']}`. |
| `Icone` | `nom` : chercher, reglages, bas, droite, gauche, haut, alerte, erreur, info, coche, trombone, fichier, agrandir, zoom, telecharger, mail, boite, camion, graphique, batiment, prise, envoyer, utilisateurs, curseurs, drapeau, lien, menu, fermer, sortir, ouvrir, plus, rafraichir. 16 px, trait 1,5, couleur du texte. |

## Modules

- **`#lib/clavier.ts`** : `$effect(() => raccourcis({ j: suivant, k: precedent, Enter: valider, Escape: fermer, e: ecarter, l: lier, 'Ctrl+Enter': envoyer }))`. Ignoré quand on tape dans un champ (sauf `Escape` et `Ctrl+Enter`, réglable par `dansLesChamps`), quand un dialogue est ouvert, et pour Entrée sur un bouton ou lien qui a le focus. `deplacer(i, ±1, n)` et `montrer(conteneur, sélecteur)` pour J/K. `/` est déjà pris par la barre du haut (recherche).
- **`#lib/libelles.ts`** : vocabulaire de l'architecture §4. `nomOutil`, `titreEnvoi`, `libelleValider(type, outil)` ; `libelleType`, `libelleEtat`/`tonEtat`, `libelleStatutEnvoi(s, outil)`/`tonStatutEnvoi`, `libelleVoie` (admins), `titreAlerte`/`tonAlerte` (contrôles) ; `libelleMethode`, `titreLiens`, `BL_EN_ATTENTE`, `FACTURE_SANS_BL` ; `libelleCategorie`, `libelleStatutReponse`/`tonStatutReponse`, `libellePriorite` (« Urgent »), `libelleBrouillon`, `ACTIONS_REPONSE`, `libelleEnvoyerDepuis`, `libelleTriePar` ; `RUBRIQUES` (réglages), `libelleTache`, `libelleRole`, `nomCourtSociete`.
- **`#lib/societe.svelte.ts`** : `societe.id` (null = toutes), `societe.courante`, `societe.liste`, `societe.url('/api/…')` ajoute `societe=` ; lire `societe.id` dans un `$effect` recharge la page au changement. Responsable : toujours `null` (le serveur filtre).
- **`#lib/compteurs.svelte.ts`** : `compteurs.aTraiter`, `compteurs.problemes` (barre du haut) ; appeler `rafraichirCompteurs()` après une validation, un envoi ou une réponse.

Les ajouts propres à une page vont dans son `<style>` ; un style servant à deux pages ou plus va dans `app.css`.
