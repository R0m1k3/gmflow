# GMFlow : architecture de l'interface (22/11/2026)

Base : `main` (94a0472) pour les données et actions. Principe : on arrive pour **faire** quelque chose ; un écran = une tâche, un bouton principal.

## 1. Tâches réelles

| # | Tâche | Qui | Fréquence | Ce qu'il faut savoir en arrivant |
|---|---|---|---|---|
| 1 | Valider les factures, avoirs et BL reçus | tous | quotidien | combien attendent, lesquels ont une alerte (doublon, fournisseur inconnu, montant incohérent) |
| 2 | Répondre aux vraies questions | tous | quotidien | combien, lesquelles sont urgentes, de qui, sur quoi (résumé en une ligne) |
| 3 | Débloquer ce qui coince : envoi à Dext/Yooz échoué, document illisible, boîte en panne, Claude déconnecté | admins | quand ça arrive | « rien à faire », ou quoi exactement et quel bouton |
| 4 | Retrouver une facture, un BL ou un mail (fournisseur, n°, montant, date) | tous | plusieurs fois par semaine | une seule barre de recherche |
| 5 | Faire le point sur un fournisseur : factures du mois, BL sans facture, écarts | tous | hebdo | l'état du fournisseur en une ligne |
| 6 | Vérifier que le mois est complet (tout envoyé, pas de doublon) | admins | mensuel | par société : reçues / envoyées / restantes |
| 7 | Mettre en route ou changer un réglage (société, boîte, Claude, envoi, utilisateurs) | admins | rare | ce qui manque encore pour que tout tourne |

## 2. Architecture de l'information

**Menu** (barre du haut, 3 entrées) : **À traiter** (badge du nombre) · **Fournisseurs** · **Bilan du mois**. À droite : la barre de **recherche**, le **sélecteur de société** (« Toutes » par défaut chez un admin, absent chez un responsable), une pastille d'état (admins : « Tout fonctionne » ou « 2 problèmes », lien vers la section Bloqué) et l'icône **Réglages** (admins).

| Écran | Contenu | Remplace | Pourquoi |
|---|---|---|---|
| **À traiter** (accueil) | Une seule page, trois sections dans cet ordre : *Bloqué* (masquée si vide), *À valider* (documents, les plus anciens et ceux avec alerte d'abord), *À répondre* (par priorité). Une ligne = une phrase + un bouton. Vide : « Tout est à jour ». | Tableau de bord, Documents « à vérifier », Dépôts, À répondre | Un seul point d'entrée ; sections séparées car valider (rapide, en série) et répondre (écrire) ne se font pas au même rythme. |
| **Fiche document** (écran central) | Aperçu à gauche ; à droite : alertes en une phrase, champs (type, société, fournisseur, n°, date, HT/TVA/TTC) avec les champs douteux surlignés, puis trois blocs repliés : *BL liés / facture liée* (liens, candidats à lier en un clic, écarts), *Envoi à Dext/Yooz* (destination, historique), *Mail d'origine*. Bouton : « Valider et envoyer à Dext » (facture, avoir) ou « Valider » (BL), qui ouvre le document suivant. Téléphone : aperçu en haut, bouton fixé en bas. | Documents/[id], Rapprochement, Dépôts | Le rapprochement et l'envoi sont des questions sur *ce* document : on les traite là où on le regarde. |
| **Fiche mail** | Mail et pièces (liens vers leurs fiches) à gauche ; réponse à droite : « Proposer une réponse », texte modifiable, « Envoyer depuis compta@… ». Correction du tri en un clic : Pas besoin de réponse · Information · Publicité. | Mails/[id], À répondre | Lire et répondre sur le même écran, sans aller-retour. |
| **Fournisseurs** | Liste des fournisseurs de la société choisie, avec une ligne d'état (« 3 factures ce mois, 1 BL sans facture »). Fiche fournisseur : identité (admin : modifier, fusionner, confirmer), puis un fil par mois mêlant factures, BL et mails, et un encadré « À rapprocher » (BL sans facture, factures sans BL, écarts). | Fournisseurs, Rapprochement par fournisseur | C'est la navigation société, puis fournisseur, puis documents de F9 ; la société est un contexte (sélecteur), pas une page. |
| **Recherche** | Résultats de la barre du haut, groupés Documents / Mails, filtres type, période, état. Requête vide : derniers mails reçus (publicités comprises). | Documents (liste), Mails (liste) | Les deux listes complètes ne servent qu'à retrouver quelque chose. |
| **Bilan du mois** | Par société : reçues, envoyées, à valider, en erreur, doublons, montant envoyé ; fournisseurs avec BL en attente. En bas, replié : qualité de la lecture et du tri (justesse par semaine). | Dépôts > Suivi du mois, bilan Documents, semaines du tri | La clôture mensuelle est une tâche à part, pas un filtre. |
| **Réglages** (admins) | Une vue par section (`/reglages/<section>`, liste dans `web/src/lib/reglages.ts`), menu latéral groupé : *Mise en route* (`/reglages` : 1 Sociétés → 2 Boîtes mail → 3 Claude → 4 Envoi vers Dext/Yooz → 5 Utilisateurs, chaque étape cochée ou « à faire ») ; *Magasins et accès* (Sociétés, Utilisateurs) ; *Connexions* (Boîtes mail, Claude, Envoi, Serveur du magasin, Logiflow) ; *Maintenance* (Traitements, Avancé : modèles et effort, limites, appels Claude, expéditeurs publicitaires ; Journal ; Zone dangereuse) ; *Vous* : Mon compte (`/compte`). Un non-admin voit « Réservé aux administrateurs » (et Mon compte) ; les anciennes ancres `/reglages#boites` et pages `/admin/*` mènent à la vue de leur section. Formulaire modifié : « Modifications non enregistrées » et confirmation avant de quitter. | Les 6 pages admin | Rare : on veut savoir ce qui manque, puis ouvrir un seul sujet. |

Disparaissent : les pages Mails, Documents, Dépôts et Rapprochement en tant qu'entrées de menu ; le tableau de bord à chiffres. Le responsable voit À traiter, Fournisseurs, Recherche et le Bilan de sa société. Pas de validation en masse : au démarrage, le cahier demande un œil sur chaque facture ; plus tard, c'est l'envoi automatique par société qui allège.

## 3. Parcours clés

**A. Valider les factures du jour** (cible : 1 clic par facture juste, + 1 par champ corrigé)
1. Ouvrir GMFlow : À traiter, section À valider « 12 documents » (0 clic).
2. Clic sur « Commencer » : première fiche (1).
3. Regarder l'aperçu (champs douteux et alertes en tête), puis « Valider et envoyer à Dext » (1) : la fiche suivante s'ouvre.
4. Si un champ est faux : clic dessus, saisie, Entrée, puis valider. Si la société a été corrigée : « Toujours ranger ce fournisseur dans Magasin X ? » (Oui = 1).
5. Doublon : le bouton devient « Écarter le doublon » ; seul un admin voit « Envoyer quand même ».
6. Dernière fiche : « Tout est validé », retour à À traiter. Bilan : 12 factures justes = 13 clics.

**B. Répondre à un mail** (cible : 3 clics + la relecture)
1. À traiter, section À répondre : « Fournisseur X : demande le RIB pour régler la facture 123 · urgent » (0).
2. Clic : fiche mail (1).
3. « Proposer une réponse » (1) ; le texte arrive en quelques secondes, on le relit, on le retouche.
4. « Envoyer depuis compta@… » (1) : le mail quitte la file, le suivant s'ouvre.
Sinon, en 1 clic : « Pas besoin de réponse », ou « Information » / « Publicité » (le tri apprend).

**C. Mettre en route une nouvelle boîte** (cible : 5 clics + saisie, moins de 3 minutes)
1. Réglages, étape 2 « Boîtes mail », « Ajouter une boîte » (2).
2. Écran 1 : adresse, mot de passe d'application, société. Serveurs IMAP et SMTP préremplis d'après le domaine (Gmail, Infomaniak), visibles seulement sinon. « Suivant » (1).
3. Écran 2 : test lancé tout seul ; dossiers trouvés, Réception cochée, dossier Envoyés repéré pour les réponses. Erreur dite en clair (« Mot de passe refusé : il faut un mot de passe d'application »). « Suivant » (1).
4. Écran 3 : historique à relever (3 mois par défaut), « Démarrer la surveillance » (1).
5. Retour sur la carte de la boîte : « Relève en cours : 240 mails vus ».

## 4. Vocabulaire

| Actuel | Affiché |
|---|---|
| Structure | Société |
| Boîte surveillée, relever | Boîte mail, « Chercher les nouveaux mails » |
| Vérifier (type, société) + Déposer | un seul mot : **Valider** |
| Dépôt, dépôt auto | Envoi à Dext / Yooz (nom de l'outil de la société), « Envoi automatique des factures sûres » |
| `statut_depot` a_verifier / pret / en_envoi / depose / erreur / doublon | À valider / Prête / Envoi en cours / Envoyée à Dext / Envoi échoué / Doublon |
| `etat` a_lire / en_cours / lu / ignore / erreur | En attente / Lecture en cours / Lu / Écarté (pas une facture) / Illisible |
| Contrôles | Alertes |
| Confiance, voie (facturx, pdf_texte, scan…) | rien (surlignage des champs douteux) ; voie visible pour les admins seulement : « lu dans le Factur-X / le PDF / le scan » |
| Rapprochement, BL sans facture, facture sans BL | BL liés / Facture liée, « BL en attente de facture », « Facture sans BL » |
| Méthode numero_bl / facture_citee / commande / manuel | par n° de BL / citée dans la facture / par n° de commande / liée à la main |
| Catégorie question / information / document_seul / publicite / notification | Question / Information / Pièce jointe seule / Publicité / Notification automatique |
| `statut_reponse` repondu / sans_suite | Répondu / Pas besoin de réponse |
| Brouillon, générer, abandonner | Réponse proposée, « Proposer une réponse », « Effacer » |
| Fournisseur a_confirmer | Nouveau fournisseur (à confirmer) |
| Expédition (SMTP) | Adresse d'envoi vers Dext/Yooz |
| Setup token, jeton, séance, clé API | Connexion à Claude (« Se connecter avec le compte Claude » ; clé API dans Avancé) |
| Expéditeurs marqués, journal, quota | Expéditeurs publicitaires, Historique, Limite par jour |

## 5. Changements d'API nécessaires

1. **Aperçu inline** : `/api/pieces/{id}?inline=1` répond en `Content-Disposition: inline` (PDF et images seulement), sinon pas d'aperçu à gauche.
2. **File « À traiter » agrégée** : `GET /api/a-traiter` (documents à valider, mails à répondre, envois bloqués, documents illisibles, alertes admin), déjà triée, avec compteurs et un `suivant` pour enchaîner les fiches ; `GET /api/a-traiter/compteurs` léger pour le badge (remplace le double appel à `/api/tableau-de-bord`).
3. **Valider en un appel** : `POST /api/documents/{id}/valider` (corrections + vérification + envoi pour une facture, vérification seule pour un BL), atomique, renvoie le document suivant. Aujourd'hui il faut enchaîner `PUT /documents/{id}`, `verifier` et `deposer`.
4. **Fournisseurs par société** : `GET /api/fournisseurs?societe=` avec la ligne d'état (factures du mois, à valider, BL sans facture) ; `GET /api/mails?fournisseur=` pour le fil de la fiche fournisseur.
5. **Recherche unique** : `GET /api/recherche?q=` sur documents et mails, qui comprend un montant (« 1 250,40 ») et une date ; `q` ne cherche aujourd'hui que nom, numéro et objet.
6. **Mise en route** : `GET /api/admin/mise-en-route` (état des 5 étapes) ; test d'une boîte avant enregistrement et test SMTP de la boîte (envoi des réponses), absents aujourd'hui (`tester` exige une boîte créée et ne teste que l'IMAP).
