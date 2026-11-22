# GMFlow : brief de design

Outil de bureau pour un dirigeant et ses collaborateurs, non techniques. Chaque écran répond à une question : « qu'est-ce que je dois faire maintenant ? ». Ne modifiez ni `src/app.css` ni `+layout.svelte` sans accord : composez avec.

## Direction : registre comptable moderne
Papier chaud, encre noire, un stylo bleu-noir. Titres en serif (Source Serif 4), texte en IBM Plex Sans, chiffres tabulaires. Traits fins, rayon 4 px, aucun dégradé, aucune ombre, aucune icône décorative, aucun emoji, pas de majuscules espacées. Signature : le **double filet** sous le titre de page (`.entete`), comme un total de grand livre. Polices auto-hébergées dans `static/polices/` (licence OFL).

## Tokens (variables CSS, `:root`)
- Surfaces : `--fond` (page), `--surface` (cartes, tableaux), `--creux` (en-têtes de tableau, survol). Traits : `--trait`, `--trait-fort`.
- Texte : `--encre` (principal), `--gris` (secondaire), `--discret` (petit, 13 px minimum).
- Accent unique : `--accent` (+ `--accent-fonce`, `--accent-clair`) : action principale, lien, sélection, focus.
- Trois états : `--ok`, `--attention`, `--alerte` (+ `-clair`, `-trait`). La couleur ne sert qu'à dire un état.
- Espacement : `--e1..--e12` = 4, 8, 12, 16, 20, 24, 32, 40, 48 px. Aucune autre valeur.
- Type : `--t-petit` 13, `--t-base` 15, `--t-moyen` 17, `--t-titre` 24, `--t-grand` 36. Poids 400/500/600. `--serif` pour h1, h2 et grands chiffres, `--sans` pour le reste.
- Contrôles : `--cible` (36 px, 44 px au tactile). Mouvement : `--vite` + `--courbe`, uniquement couleur, fond, transform.

## Vocabulaire à employer
| Ne pas écrire | Écrire |
|---|---|
| structure | société |
| boîte surveillée | boîte mail |
| setup-token, jeton, OAuth | connexion à Claude |
| apalis, file, job, worker | travail en attente / en cours |
| statuts bruts `a_lire`, `lu`, `erreur`, `ignore` | À lire par Claude, Lu, Non lu, Sans intérêt |
| quota, appels | limite du jour, lectures aujourd'hui |
| rapprochement (titre de colonne ou de lien) | facture reliée à son bon de livraison |
| dépôt `bloque`, `doublon` | Refusé par Dext ou Yooz, Déjà déposée |
| SMTP | envoi des mails |
| payload, ID, null | ne jamais afficher ; « Non renseigné » |

Boutons : verbe à l'infinitif (« Vérifier », « Déposer »). Listes : nombre + objet + état (« 5 mails attendent une réponse »). Pas de point final sur un libellé, pas de `!`, pas de « Bienvenue ». Dates via `quand()` de `#lib/format.ts`.

## Composants CSS (`src/app.css`)
- `.entete` : tout haut de page. `<div class="entete"><div><h1>Titre</h1><p class="sous">contexte</p></div><div class="actions">…</div></div>`. Un seul h1 par page.
- `.section` + `.titre` : bloc de page avec h2 sur filet. `h2` seul pour un sous-bloc.
- `.liste` (`li > .principal` + `.droite`) : une ligne = une chose à traiter, action à droite. Premier choix pour toute file de travail.
- `table` : seulement pour comparer des colonnes de valeurs (montants, dates). `tr.cliquable` si la ligne ouvre une fiche, `.nombre` aligne à droite.
- `.kv` (`dl > dt + dd`) : fiche d'un objet (fournisseur, document).
- `.onglets` (liens ou boutons, `.actif` ou `aria-current`, `.n` pour un compteur) : filtres d'une même liste, jamais navigation entre pages.
- Boutons : défaut = primaire (un seul par écran ou par ligne), `.second`, `.discret` (tertiaire, comme un lien), `.danger` (suppression), `.petit` dans une ligne de tableau. `a.bouton` pour un lien d'action.
- `.etiquette ok|att|ko` (neutre sans classe) : état d'un objet, un mot. `.bandeau erreur|attention|ok|info` : message de page, avec la suite à donner.
- `.vide` (`strong` + `p` + `.actions`) : liste vide, dit pourquoi et que faire. `.charge` : attente d'une donnée.
- `.carte` : groupe de champs ou bloc isolé ; pas de cartes imbriquées ni de grille de cartes identiques. Aussi : `.champs` (grille de champs), `.actions` (rangée de boutons), `.aide` (texte sous un champ), `.pagination`, `.corps` (texte de mail), `.chiffre` (grand nombre serif), `.rouge`, `.vert`, `.gris`, `.discret` (texte secondaire).

## 5 règles UX pour les pages métier
1. **La tâche d'abord** : ce qui attend une action vient avant les chiffres et les réglages, le plus urgent en haut.
2. **Une action principale par écran ou par ligne**, nommée par un verbe, à droite de la ligne ; le reste en `.second` ou `.discret`.
3. **Aucun code brut** : tout statut, motif ou erreur devient une phrase ou une étiquette du glossaire, avec ce qu'il faut faire ensuite.
4. **Les filtres rangent, ils ne cachent pas** : onglets par état (À vérifier, Prêtes, Refusées) avec compteurs, vue par défaut = ce qui demande une action ; le détail technique se replie (`<details>`).
5. **Chaque état a son rendu** : vide (`.vide`), chargement (`.charge`), erreur (`.bandeau erreur` + réessayer), succès nommé comme le bouton (« Déposée »). Sur téléphone : une colonne, boutons à 44 px, tableaux qui défilent ou deviennent `.liste`.

Accessibilité : contraste AA, `label` lié à chaque champ, focus visible (jamais `outline: none`), jamais la couleur seule pour porter une information (l'étiquette a un mot), `prefers-reduced-motion` respecté.
