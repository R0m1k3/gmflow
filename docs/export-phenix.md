# Export des BL vers BO Phénix

Sur la fiche d'un BL lu, le bloc **Export Phénix** propose deux fichiers Excel, aux en-têtes des
modèles du magasin :

| Fichier | Sert à | Colonnes remplies |
|---|---|---|
| **Matrice d'intégration** (onglet `Feuil1`) | intégrer la commande ; le fournisseur se choisit dans Phénix | `CODEIN`, `GENCOD`, `REFERENCE`, `QTECDE`, `PA`, `REMISE ` (avec l'espace du modèle) |
| **Trame BO Phénix** (onglet `TRAME MODELE`, en premier) | créer ou mettre à jour les fiches produits | Codein, Fournisseur (code BO Phénix), réf., gencod, libellé, colisage, prix d'achat, DEEE HT, Ecomob HT, PV conseillé ; pour les lots : composants détaillés. N° de ligne et TVA laissés vides : le magasin les saisit |

## Règles

- **Pas de code article (Codein) hors lot** : seuls le gencod et la référence imprimés sur le BL
  identifient l'article. Le Codein n'est écrit que pour un lot et ses composants (Phénix n'écrase
  rien sur une fiche existante).
- **Référence et/ou gencod** : réglage par fournisseur (fiche fournisseur > « Export Phénix :
  colonnes reprises du BL » : les deux, référence seulement ou gencod seulement ; une marque sans
  réglage suit son groupe). Vaut pour la matrice et la trame, sauf les composants d'un lot (codes
  du magasin).
- **Tout vient du BL** : prix d'achat brut HT, remise de ligne, quantités, DEEE / Ecomob
  (selon ce que le BL imprime ; dans la trame, 0 quand le BL n'en donne pas), PV conseillé. Le prix de vente et la famille d'un nouveau produit restent vides : le magasin les fixe.
- **Quantité** en unités = colis × PCB quand le BL donne les deux, sinon la quantité imprimée.
- **Prix par colis** : ramené à l'unité quand le montant de la ligne montre que le prix imprimé
  est celui du colis. Sans prix unitaire : montant ÷ quantité (déjà net, sans remise en plus).
- **Lots présumés** : désignation avec le mot « lot », qui commence par « SET », ou un multiple
  (« X4 », « x 3 »). Chacun est vérifié sur le serveur du magasin : un article trouvé avec des
  composants est un lot ; trouvé sans composants, c'est un produit unique, traité comme les
  autres ; introuvable, une alerte le signale.
- **Lots** : une ligne dont l'article est un lot au magasin est remplacée par ses composants
  (codein, gencod, réf., quantité dans le lot, prix de vente, famille, éco-participation du
  magasin). Le prix d'achat des composants est rééquilibré pour que Σ prix × quantité redonne le
  prix du lot sur le BL (répartition du magasin : prix d'achat, sinon prix de vente ; calcul en
  décimal exact, prix au dix-millième, le reste d'arrondi sur le composant le plus lourd de quantité
  1, sinon le plus lourd). Dans la
  trame, c'est un **lot de vente** : ligne mère, puis composants avec `NUM LOT VENTE` = n° de la
  ligne mère, `QTE LOT VENTE` et `% LOT VENTE`. L'éco-participation des composants va dans la
  colonne (DEEE ou Ecomob) que le BL donne pour le lot, l'autre à 0.

## Recherche au magasin

Après la lecture d'un BL (et sur « Chercher de nouveau »), le worker cherche chaque ligne sur le
serveur du magasin (Apiflow, Réglages > Serveur du magasin) : par gencod (tous les GTIN de
l'article), sinon par référence fournisseur chez les codes BO Phénix de la fiche (puis groupe,
puis marques) — filtre `ref` d'Apiflow, chaque candidat revérifié sur sa fiche. Pour un article
trouvé, `GET /api/appro/article/:id` donne ses composants s'il est un lot, puis
`/api/articles/:id/referentiel` le détail de chacun. Le résultat est gardé par ligne
(`ligne_document.magasin`) ; `document.magasin_demande_le` / `magasin_verifie_le` /
`magasin_erreur` disent où en est la recherche.

Tant que la recherche n'est pas faite sans erreur, les deux boutons restent grisés (un lot non
détaillé serait importé comme un seul article). Sans serveur du magasin configuré, les fichiers
sont téléchargeables, sans codes article ni lots détaillés (alerte affichée).

API : `GET /api/documents/{id}/export-phenix` (état, alertes), `POST …/verifier`,
`GET …/matrice` et `GET …/trame` (fichiers).

Les BL lus avant cette version n'ont ni gencod ni colisage : « Relire en détail ».
