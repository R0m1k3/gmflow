# API de la refonte de l'interface (direction B)

Contrat des endpoints ajoutés pour la nouvelle interface (`docs/architecture.md` §5). Rien n'a été retiré : les anciennes routes marchent comme avant. Les types TypeScript sont dans `web/src/lib/types.ts` (section « Refonte »), et les fonctions d'appel dans `web/src/lib/api.ts` (`aTraiter`, `validerDocument`, `ecarterDocument`, `urlApercu`, `fournisseurs`, `filFournisseur`, `rechercher`, `bilanMois`, `miseEnRoute`, `essayerBoite`, `essayerSmtp`, `lireEssai`, `attendreEssai`, `modifierFournisseur`, `lireBoite`, `creerBoite`).

## Règles communes

- Session obligatoire (cookie `gmflow`), sinon `401 {"erreur": "non connecté"}`. Toute mutation exige l'en-tête `X-GMFlow: 1` (`api.ts` le pose), sinon `403`.
- Erreurs : `{"erreur": "phrase"}` avec `400` (entrée invalide), `403` (droit manquant), `404` (absent ou hors périmètre), `409` (refus métier). Certains `409` ajoutent un `code` stable : `{"erreur": "...", "code": "doublon"}`. Dans le front, `ErreurApi.raison` porte ce code. Certains `400` disent le champ en faute : `{"erreur": "...", "champ": "siret"}` (`ErreurApi.champ`), le formulaire affiche le message sous ce champ.
- Dates : `AAAA-MM-JJ` ; instants : RFC 3339 (UTC). Montants : nombres en euros (2 décimales).
- **Périmètre d'un responsable** (rôle non admin) : il voit les documents et mails reçus par une boîte de sa société. Pour les documents, s'y ajoute : rangés dans sa société ou pas encore rangés (`societe_id` nul). Un document de sa boîte rangé dans une autre société ne lui apparaît pas dans À traiter, la recherche, le fil fournisseur ni le bilan, et `valider` ou `ecarter` lui répondent `403`. Les fiches existantes (`GET /api/documents/{id}`, pièces) suivent toujours la règle du mail : visibles si le mail est visible. Fournisseurs : voir §4.
- **`?societe=ID`** (filtre « société choisie ») : un document compte pour la société où il est rangé ; s'il n'est pas encore rangé, il compte pour la société d'une boîte qui l'a reçu. Un mail compte pour la société de ses boîtes. Pour un responsable, ce filtre s'ajoute à son périmètre : une autre société ne rend rien (pas d'erreur), comme les endpoints existants.

## 1. Aperçu d'une pièce

`GET /api/pieces/{id}?inline=1`

- PDF (`application/pdf`) et images (`image/png`, `image/jpeg`, `image/gif`, `image/webp`) : `Content-Disposition: inline; filename=...`, à afficher dans un `<iframe>`, `<embed>` ou `<img>` de GMFlow.
- Tout autre type (HTML, SVG, XML, tableurs...) : `attachment` comme sans `inline` (pas d'aperçu : afficher un lien de téléchargement).
- En-têtes de sécurité de toute réponse de pièce : `X-Content-Type-Options: nosniff`, `Cross-Origin-Resource-Policy: same-origin`, `Cache-Control: private, no-store`, et une CSP :
  - PDF : `default-src 'none'; object-src 'self'; img-src 'self' data:; style-src 'unsafe-inline'; frame-ancestors 'self'` (pas de `sandbox` : les lecteurs PDF de Chrome et Firefox refusent de s'ouvrir isolés) ;
  - image : `default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'self'; sandbox` ;
  - téléchargement : `default-src 'none'; frame-ancestors 'none'; sandbox`.
- À vérifier en phase 3 dans les vrais navigateurs (Chrome, Firefox, Safari sur tablette) : l'aperçu PDF dans un `<iframe>` avec cette CSP.
- `404` si la pièce n'est pas visible (même règle que le téléchargement).

## 2. File « À traiter »

### `GET /api/a-traiter?societe=`

```json
{
  "compteurs": { "bloque": 5, "a_valider": 6, "a_repondre": 2, "total": 13 },
  "bloque": [Bloque],
  "a_valider": [DocumentAValider],
  "a_repondre": [MailARepondre],
  "suivant": 812
}
```

Au plus 200 éléments par section ; les compteurs restent exacts. `suivant` : premier document de `a_valider` (bouton « Commencer »), ou `null`.

**Bloqué**, dans cet ordre :
1. (admins) Claude déconnecté : `claude_etat` absent ou en erreur. `genre: "claude"`, `motif: "absent" | "erreur"`, `message` (erreur du dernier test), `depuis` (dernier test).
2. (admins) Boîtes en panne : en pause automatique, ou active et en échec depuis plus de 30 min (même règle que l'alerte par mail). `genre: "boite"`, `boite_id`, `libelle`, `motif: "pause_auto" | "erreur"`, `message` (dernière erreur), `depuis`, `societe_id`, `societe`. Filtrées par `?societe=`.
3. Envois en échec : `statut_depot = erreur`, sauf ceux relancés (validés après le dernier envoi) il y a moins de 30 min : le worker va les reprendre, ils ne doivent pas être renvoyés une seconde fois (la fiche document porte `envoi_relance: true`). Passé 30 min sans reprise, ils reviennent dans Bloqué. `genre: "envoi_erreur"`, `document_id`, `message` (message du dernier envoi), `depuis` (dernier envoi).
4. Doublons pas encore écartés : `statut_depot = doublon` et pas vérifiés. `genre: "doublon"`, `message` (« Même facture que le document 123 (déjà envoyée à Dext/Yooz) »), `depuis` (lu le).
5. Documents illisibles : `etat = erreur`. `genre: "illisible"`, `message` (erreur de lecture).

Champs de `Bloque` : `genre`, `document_id`, `boite_id`, `libelle` (boîte, ou nom du fichier), `motif`, `message`, `depuis`, `type`, `fournisseur`, `numero`, `societe_id`, `societe` (`null` quand sans objet). Les documents sont triés envoi en échec, puis doublon, puis illisible, puis du plus ancien au plus récent.

**À valider** : factures, avoirs et BL lus (`etat = lu`) qui ne sont ni en envoi, ni envoyés, ni en échec d'envoi, ni doublons, et
- pas encore vérifiés ;
- ou, pour une facture ou un avoir, vérifiés mais jamais validés pour l'envoi (`valide_le` nul, `statut_depot` `a_verifier` ou `pret`) : c'est le cas d'une vérification faite avec l'ancienne interface.

Tri : ceux qui ont une alerte d'abord (contrôle en échec, ou BL sans fournisseur identifié), puis du plus ancien au plus récent (date du mail), puis par id.

```json
{
  "id": 812, "mail_id": 4410, "type": "facture",
  "fournisseur_id": 7, "fournisseur": "Grossiste Durand", "fournisseur_a_confirmer": false,
  "numero": "F-2026-118", "societe_id": 1, "societe": "Frouard",
  "date_document": "2026-11-18", "recu_le": "2026-11-18T09:12:00Z", "montant_ttc": 1250.4,
  "alerte": "HT + TVA donnent 1 250,00 €, le TTC lu est 1 250,40 €", "alerte_code": "montants", "nb_alertes": 1,
  "statut_depot": "a_verifier", "verifie": false
}
```

`fournisseur` : nom de la fiche fournisseur, à défaut nom lu sur le document. `alerte` : première alerte en une phrase (`null` sinon) ; la liste complète est dans `controles` de la fiche document. `alerte_code` : code stable de cette alerte (codes des contrôles, §3 bis ; `fournisseur` pour un BL sans fournisseur identifié) : c'est lui que le front lit, jamais la phrase.

**À répondre** : mails `statut_reponse = a_repondre`, priorité `haute`, `normale`, `basse`, puis les plus anciens.

```json
{ "id": 4402, "expediteur": "compta@x.fr", "objet": "RIB",
  "date_mail": "2026-11-20T08:00:00Z", "resume": "Demande le RIB pour régler la facture 123",
  "priorite": "haute", "societes": ["Frouard"], "fournisseur": "Fournisseur X", "brouillon": "propose" }
```

`expediteur` : adresse seule (le nom affiché n'est pas gardé). `fournisseur` : fiche dont un domaine est celui de l'expéditeur, sinon `null`.

`brouillon` : réponse proposée en cours (`generation`, `propose`, `modifie`, `envoi`) ou `null`.

### `GET /api/a-traiter/compteurs?societe=`

`{"bloque": 5, "a_valider": 6, "a_repondre": 2, "total": 13}` : mêmes règles que la file (les tests vérifient l'égalité), en quelques requêtes `count`. Sert au badge du menu ; `bloque` sert à la pastille d'état des admins (« Tout fonctionne » si 0). Remplace le double appel à `/api/tableau-de-bord`.

## 3. Valider en un appel

### `POST /api/documents/{id}/valider?societe=`

Corps (tous les champs facultatifs) :

```json
{
  "corrections": {
    "type": "facture", "societe_id": 1, "fournisseur_id": 7, "numero": "F-2",
    "date_document": "2026-11-18", "numero_commande": null,
    "montant_ht": 100.0, "montant_tva": 20.0, "montant_ttc": 120.0
  },
  "forcer": false
}
```

- `corrections` : exactement le corps de `PUT /api/documents/{id}` (tous les champs : un champ absent ou `null` est effacé). Absent ou `null` : rien à corriger, les valeurs lues sont confirmées.
- `forcer` : administrateur seulement (`403` sinon) ; envoie un doublon quand même.
- `?societe=` : société choisie dans l'interface, pour que `suivant` reste dans la même file.

Dans une seule transaction (document verrouillé) :
1. corrections et vérification (mêmes règles et même historique `corrections` que le `PUT` ; journal `correction` ou `verification`) ;
2. nouveau calcul des contrôles et du doublon (`depot::evaluer_dans`) ;
3. facture ou avoir : refus éventuels ci-dessous, sinon `valide_le`/`valide_par` et journal `validation` (`"par": "valider"`) ;
4. BL, ou un autre type : vérification seule.

Si un refus arrive, **rien n'est gardé** (ni les corrections, ni la vérification). Après la transaction : demande d'envoi poussée dans la file `envoi` (facture/avoir), puis nouveau rapprochement. La demande d'envoi n'est pas dans la transaction (file apalis) : si la pousser échouait (`500`), le document resterait validé sans envoi, comme avec `deposer`.

Une validation passe outre les contrôles en échec (comme `deposer`), sauf doublon et adresse Dext/Yooz manquante. Le dépôt automatique d'une société n'est pas déclenché en plus : une seule demande d'envoi.

Réponse `200` :

```json
{ "id": 812, "statut_depot": "pret", "envoi": true, "suivant": 815 }
```

`statut_depot` : statut après les contrôles (`null` pour un BL). `envoi` : l'envoi à Dext/Yooz a été demandé. `suivant` : document qui suivait celui-ci dans la file À valider (même ordre que `GET /api/a-traiter`, place lue avant la validation) ; s'il n'y en a plus après, le premier de la file ; `null` si la file est vide : « Tout est validé ».

Erreurs :

| Code HTTP | `code` | Cas |
|---|---|---|
| 400 | | type inconnu, société hors périmètre, fournisseur inconnu, date invalide |
| 403 | | document d'une autre société (responsable), `forcer` sans être admin |
| 404 | | document absent ou invisible |
| 409 | `etat` | document pas lu (en attente, en lecture, illisible, écarté) |
| 409 | `deja_envoye` | déjà envoyé ou en cours d'envoi |
| 409 | `expedition` | adresse d'envoi vers Dext/Yooz pas configurée (Réglages) |
| 409 | `erreur_envoi` | envoi précédent en échec : seul un admin renvoie |
| 409 | `doublon` | doublon d'une facture déjà reçue ; le message est celui du contrôle (« Même facture que le document 123 (reçue avant) ») |
| 409 | `sans_adresse` | la société n'a pas d'adresse Dext/Yooz |

Parcours « Doublon » : sur `409 doublon`, le bouton devient « Écarter le doublon » (`ecarter`, ci-dessous) ; un admin voit aussi « Envoyer quand même » (`forcer: true`).

### `POST /api/documents/{id}/ecarter?societe=` (ajout)

Pas demandé au §5 mais nécessaire au parcours A.5 : sans lui, un doublon resterait dans Bloqué pour toujours.
- Doublon (lu) : marqué vérifié (jamais envoyé, statut `doublon` gardé) ; il quitte Bloqué. Journal `doublon_ecarte`.
- Tout autre document lu (facture, avoir, BL, autre type) : passe en `ignore` (« Écarté », pas une facture : jamais envoyé, contrôles effacés) et quitte la file. Journal `document_ecarte`. C'est le bouton « Écarter » de la fiche (plus besoin de le valider en type `autre`).
- Document illisible (`etat = erreur`) : passe en `ignore`. Journal `illisible_ecarte`. Un admin peut toujours le relire (`POST /api/admin/documents/{id}/relire`).
- Le rapprochement du document est recalculé (un BL écarté n'attend plus de facture).
- Refus : `409 {"code": "deja_envoye"}` (en envoi ou envoyé), `409 {"code": "erreur_envoi"}` (envoi en échec, sauf admin : il est peut-être arrivé chez Dext/Yooz), `409 {"code": "etat"}` (en attente, en lecture, déjà écarté). Mêmes droits que `valider` (`403`/`404`).

Réponse : même forme que `valider` (`envoi: false`).

## 3 bis. Fiche document (`GET /api/documents/{id}`, route existante, enrichie)

Ajouts :

```json
{ "recu_le": "2026-11-18T09:12:00Z", "envoi_relance": false,
  "boites": [ { "boite_id": 1, "libelle": "Compta Frouard", "adresse": "compta@frouard.fr",
                "societe_id": 1, "societe": "Frouard" } ] }
```

- `recu_le` : réception du mail d'origine (date du mail, à défaut date de relève). `boites` : boîtes de réception du mail (celles du périmètre), par libellé.
- `envoi_relance` : envoi en échec relancé il y a moins de 30 min (voir Bloqué) : afficher « Envoi relancé », pas de bouton Renvoyer.

**Contrôles** (`controles`, aussi dans `GET /api/documents`) : `{"code", "message", "champs"?, "proposition"?}`.
- `message` : phrase dans le vocabulaire de `docs/architecture.md` §4, majuscule, sans point final, montants à la française (« 1 250,40 € », espaces insécables), dates JJ/MM/AAAA. À afficher tel quel.
- `champs` : champs visés, noms de l'API (`numero`, `date_document`, `montant_ht`, `montant_tva`, `montant_ttc`, `societe_id`, `fournisseur_id`) : à surligner.
- `proposition` : `{"champ": "montant_ttc", "valeur": 1250.0}` : correction en un clic (HT + TVA = TTC ; HT ou TVA manquant déduit du TTC).

| `code` | Exemples de `message` |
|---|---|
| `doublon` | Même facture que le document 123 (déjà envoyée à Dext/Yooz) · (reçue avant) |
| `incomplet` | Champ manquant : numéro · Champs manquants : numéro, date |
| `montants` | HT + TVA donnent 1 200,00 €, le TTC lu est 1 250,40 € · Montant HT manquant · Montant de TVA manquant · Montants HT et TVA manquants |
| `identifiants` | Ni SIRET ni n° de TVA du fournisseur · SIRET du fournisseur « … » invalide · N° de TVA du fournisseur « … » invalide |
| `societe` | Société destinataire non reconnue · Société à vérifier : reconnue avec une confiance de 85 % |
| `depot` | La société n'a pas d'adresse d'envoi vers Dext ou Yooz |
| `fournisseur` | Nouveau fournisseur (à confirmer) · Fournisseur non identifié |
| `date` | Date du 21/11/2027 dans le futur · Date du 01/01/2025 : plus de 12 mois |

Les contrôles enregistrés avant ce changement sont recalculés une fois au démarrage de l'app (factures pas encore parties : `a_verifier`, `pret`, `doublon` ; paramètre `controles_version`). Ceux des envois en échec gardent leur ancienne phrase jusqu'au renvoi : `champs` y est absent.

## 4. Fournisseurs

### `GET /api/fournisseurs?societe=&q=&statut=&page=` (route existante, enrichie)

Sans `societe` : tous les fournisseurs pour un admin, 50 par page. **Un responsable ne voit que les fournisseurs de sa société** : au moins un document dans son périmètre, ou sa société par défaut (`societe_defaut_id`) ; avec ou sans `q`. Les autres lui répondent `404` (`GET /api/fournisseurs/{id}`, `fil`). Pour corriger le fournisseur d'un document, il choisit parmi ceux-là (un fournisseur nouveau pour sa société est rattaché par la lecture, ou par un admin). Avec `societe` : seulement ceux qui ont au moins un document lu dans cette société (et le périmètre). `nb_documents` et `a_verifier` (anciens champs) comptent aussi dans le périmètre.

Chaque ligne garde ses champs et gagne la ligne d'état, calculée sur les documents lus visibles (et de la société choisie) :

```json
{ "id": 7, "nom": "Grossiste Durand", "...": "...",
  "factures_mois": 3, "a_valider": 1, "bl_sans_facture": 1, "dernier_document": "2026-11-18" }
```

`factures_mois` : factures et avoirs datés du mois en cours (heure de Paris ; date du document, à défaut du mail). `a_valider` : mêmes règles que la file À valider. `bl_sans_facture` : BL sans facture rapprochée et pas écartés du rapprochement.

### `GET /api/fournisseurs/{id}/fil?societe=&mois=6`

Choix : un endpoint `fil` dédié plutôt que `GET /api/mails?fournisseur=` : la fiche a besoin des documents, des mails et de l'encadré À rapprocher en un appel, et le lien mail-fournisseur ne se résume pas à un filtre de la liste des mails.

`mois` : nombre de mois rendus, mois en cours compris (de 1 à 24, 6 par défaut ; `400` sinon). `404` si le fournisseur n'existe pas ou n'est pas visible. `fournisseur` porte aussi `societe_defaut` (nom de la société par défaut, comme `GET /api/fournisseurs/{id}` et les réponses de création et modification).

```json
{
  "fournisseur": Fournisseur,
  "etat": { "factures_mois": 3, "a_valider": 1, "bl_sans_facture": 1 },
  "mois": [
    { "mois": "2026-11", "elements": [
      { "genre": "document", "id": 812, "mail_id": 4410, "type": "facture", "numero": "F-118",
        "date": "2026-11-18", "date_document": "2026-11-18", "montant_ht": 1042.0,
        "montant_ttc": 1250.4, "statut_depot": "pret", "verifie": true,
        "societe_id": 1, "societe": "Frouard", "outil": "dext", "rapproche": true, "nb_alertes": 0 },
      { "genre": "mail", "id": 4402, "date": "2026-11-17", "date_mail": "2026-11-17T08:00:00Z",
        "expediteur": "compta@durand.fr", "objet": "Relance", "categorie": "question",
        "statut_reponse": "a_repondre", "resume": "...", "document_ids": [] }
    ] }
  ],
  "a_rapprocher": { "bl_sans_facture": [DocRap], "factures_sans_bl": [DocRap],
                    "avec_ecart": [Rapproche], "tronque": false }
}
```

- Documents : factures, avoirs et BL lus du fournisseur, dans le périmètre (et la société choisie).
- Mails : ceux qui portent un document du fournisseur, ou envoyés depuis un des domaines de sa fiche ; publicités exclues. `document_ids` : documents du fournisseur portés par ce mail (ils sont aussi dans le fil : le front peut masquer le mail).
- Mois du plus récent au plus ancien, éléments du plus récent au plus ancien (`date` : date du document, à défaut du mail, heure de Paris) ; un mois vide n'apparaît pas. 1000 documents et 1000 mails au plus.
- `a_rapprocher` : la vue `GET /api/rapprochement?fournisseur={id}&societe=` (types `DocRap`, `Rapproche` existants), sur tout l'historique, réduite aux trois listes utiles (300 éléments au plus chacune). `GET /api/rapprochement?societe=` suit maintenant la règle commune (documents pas encore rangés des boîtes de la société compris).
- `document_ids` d'un mail : seulement les documents du fil (périmètre compris).

### `PATCH /api/admin/fournisseurs/{id}` (admins)

Modification partielle : seuls les champs présents changent (`null` efface un champ facultatif) et **seuls eux sont vérifiés** : une ancienne valeur invalide déjà enregistrée (SIRET, TVA, e-mail) ne bloque pas l'enregistrement d'un autre champ. Même journal et même nouveau calcul des contrôles que `PUT` (qui, lui, vérifie toute la fiche). Pour « Confirmer la fiche » : `{"statut": "valide"}` ; « Toujours ranger dans X » : `{"societe_defaut_id": 1}`. Réponse : la fiche (`Fournisseur`, avec `societe_defaut`). `400` avec `champ` (`statut`, `siret`, `tva`, `domaines`, `email`, `code_gestion` déjà pris, `groupe_id`, `societe_defaut_id` inconnue…), `404` (absent ou fusionné).

## 5. Recherche unique


### Groupes de fournisseurs (U10 → L3C, Homea, Decor10)

- **Lecture** : Claude rend aussi `fournisseur.groupe` (enseigne ou maison mère vue en en-tête ou en logo) et `fournisseur.entite` (société, marque, département qui a vendu ou livré : « Dép. : L3C »), gardés dans `document.fournisseur_groupe` / `fournisseur_entite`.
- **Rattachement** (worker) : groupe et entité lus et distincts (noms comparés collés : « L 3 C » = « L3C ») → fiche du groupe (même nom, sinon celle de son SIREN/TVA, sinon créée « à confirmer » avec les identifiants lus) et fiche de l'entité dans ce groupe (marque de même nom, sinon fiche hors groupe de même nom rattachée, sinon créée « à confirmer ») ; le document va à l'entité. Sans entité lue, sur un groupe qui a des marques : mention « Dép. : X », « Département X », « Division X », « Marque : X » du texte (`fournisseurs::mention_entite`), sinon marque nommée, sinon une nouvelle marque X, sinon Claude. Quand un groupe gagne une marque, ses documents restés sur le groupe sont répartis tout de suite (sans Claude).
- `GET /api/admin/fournisseurs/doublons` : chaque paire a `meme_groupe` (même SIREN, domaine ou adresse, noms sans rapport : raison « même groupe probable », à regrouper, jamais à fusionner) ; les fiches ont `groupe_id`, `groupe`, `nb_marques`. « Regrouper » = `PATCH` de `groupe_id`, puis `POST /api/admin/fournisseurs/{groupe}/repartir`.

### `GET /api/recherche?q=&type=&du=&au=&societe=`

- `type` : `facture`, `avoir`, `bl` (documents de ce type, pas de mails), `document` (tous les documents, pas de mails) ou `mail` (mails seulement) ; autre valeur : `400`.
- `du`, `au` : période (date du document, à défaut du mail ; date du mail pour les mails).
- `q` est découpé :
  - **montant** : `1 250,40`, `1 250,40 €`, `1250.4`, `1.250,40`, `1,250.40`, `60 €`, `12 000`. Cherché dans le TTC, le HT ou la TVA des documents ; un mail sort s'il porte un document de ce montant ;
  - **date** : `18/11/2026`, `18/11/26`, `18.11.2026`, `2026-11-18`, et `18/11` (le 18 novembre de n'importe quelle année). Date du document ou date du mail ; `10.11` est lu comme une date (écrire `10,11` pour un montant) ;
  - **nombre entier seul** (`1250`) : un numéro ou un montant ;
  - **mots** : tous doivent se retrouver, dans le fournisseur (lu ou fiche), le numéro, le n° de commande, l'objet, l'expéditeur ou le nom du fichier (pour un mail : expéditeur, objet, nom d'une pièce, fournisseur ou numéro d'un de ses documents).
- Documents : sauf ceux écartés (`ignore`). Mails : publicités comprises. Les plus récents d'abord, 50 au plus par groupe (`total` exact). Requête vide : les derniers reçus.

```json
{
  "interpretation": { "mots": ["durand"], "montant": 1250.4, "nombres": [],
                      "date": { "jour": 18, "mois": 11, "annee": null } },
  "documents": { "total": 1, "elements": [ { "id": 812, "mail_id": 4410, "type": "facture",
      "etat": "lu", "statut_depot": "pret", "fournisseur_id": 7, "fournisseur": "Grossiste Durand",
      "numero": "F-118", "date_document": "2026-11-18", "montant_ht": 1042.0, "montant_ttc": 1250.4,
      "societe_id": 1, "societe": "Frouard", "outil": "dext", "nom_fichier": "F-118.pdf",
      "recu_le": "2026-11-18T09:12:00Z", "verifie": true } ] },
  "mails": { "total": 1, "elements": [ { "id": 4410, "expediteur": "...", "objet": "...",
      "date_mail": "...", "categorie": "document_seul", "statut_reponse": null, "resume": null,
      "nb_pieces": 1 } ] }
}
```

`outil` : outil de la société du document (`dext`, `yooz`, `null` si pas encore rangé ou pas réglé). Pas de filtre « état » (envoyée, à valider...) pour l'instant : à ajouter si l'écran en a besoin.

## 6. Mise en route et essais de boîte (admins, `403` sinon)

### `GET /api/admin/mise-en-route`

```json
{ "terminee": false, "etapes": [
  { "cle": "societes", "fait": true, "resume": "2 sociétés : Frouard, Houdemont", "manques": [] },
  { "cle": "boites", "fait": false, "resume": "1 boîte mail surveillée sur 2",
    "manques": ["Houdemont : aucune boîte mail surveillée"] },
  { "cle": "claude", "fait": false, "resume": "Pas connecté", "manques": ["Se connecter avec le compte Claude"] },
  { "cle": "envoi", "fait": false, "resume": "Adresse d'envoi pas configurée", "manques": ["..."] },
  { "cle": "utilisateurs", "fait": false, "resume": "1 administrateur, 1 responsable",
    "manques": ["Houdemont : aucun compte de responsable"] } ] }
```

Textes courts : noms de société sans forme juridique (« SAS Frouard » donne « Frouard »), pas de « 0 sur 0 » (« Aucune boîte mail », « 2 boîtes mail, aucune surveillée », « 2 boîtes mail surveillées », « 1 administrateur, aucun responsable »).

Une étape est faite quand `manques` est vide (sociétés non archivées) :
1. `societes` : au moins une société ;
2. `boites` : chaque société a une boîte surveillée (active), aucune n'est en pause automatique, et chaque boîte inactive a réussi son test (sinon « connexion pas encore réussie ») ;
3. `claude` : dernier test de la connexion réussi (`valide`) ; `en_test` : pas encore fait ;
4. `envoi` : adresse d'envoi configurée, dernier essai pas en échec, chaque société a son adresse Dext/Yooz ;
5. `utilisateurs` : chaque société a au moins un compte responsable actif.

### `POST /api/admin/boites/tester`

Essai IMAP (et SMTP si un serveur SMTP est donné) avec des paramètres **pas encore enregistrés**. L'app ne se connecte à aucun serveur de mail (règle du projet) : elle enregistre la demande (mot de passe chiffré, dans `parametre`, clé `essai_boite:<uuid>`) et le worker fait l'essai.

```json
{ "serveur": "mail.infomaniak.com", "port": 993, "identifiant": "compta@frouard.fr",
  "mot_de_passe": "...", "smtp_serveur": "mail.infomaniak.com", "smtp_securite": "tls" }
```

- `port` : 993 par défaut. `smtp_port` : 465 (`tls`) ou 587 (`starttls`) par défaut. `imap: false` : SMTP seulement.
- `boite_id` : boîte existante en cours de modification ; tout champ absent (mot de passe compris) est pris dans la boîte.
- `400` : identifiant, mot de passe (sans `boite_id`) ou serveur IMAP manquant, port ou sécurité invalides, rien à essayer. `404` : `boite_id` inconnu.

Réponse `202 {"id": "6f1c…"}`. Les serveurs d'après le domaine (Gmail, Infomaniak) sont à préremplir côté front.

### `GET /api/admin/boites/{id}`

La boîte (`Boite`, comme une ligne de `GET /api/admin/boites`) ; `404` si elle n'existe pas.

### `POST /api/admin/boites` (route existante, enrichie) : création depuis un essai

Deux champs facultatifs en plus du corps habituel :
- `essai_id` : essai réussi de **ces** paramètres (`POST /api/admin/boites/tester` sans `boite_id`, terminé, connexion IMAP réussie, moins d'une heure, même serveur, port, identifiant et mot de passe : le mot de passe est comparé à une empreinte salée gardée avec l'essai, jamais rendue). Son résultat devient le `dernier_test` de la boîte : pas de second essai.
- `activer: true` : la boîte est surveillée tout de suite (exige `essai_id`, sinon `400`). Journal `activation`.

Refus : `409 {"code": "essai"}` (essai introuvable ou expiré, pas terminé, raté, ou fait avec d'autres paramètres : « … : relancez l'essai de connexion »). Sans ces champs : comme avant (boîte en pause, sans test).

### `dernier_test` d'une boîte (`POST /api/admin/boites/{id}/tester`, existant)

Le test d'une boîte enregistrée rend maintenant la même chose que l'essai :

```json
{ "quand": "...", "ok": true, "message": "Connexion réussie : 7 dossiers trouvés.",
  "dossiers": ["INBOX", "Envoy&AOk-s"],
  "dossiers_detail": [ { "nom": "Envoy&AOk-s", "affiche": "Envoyés", "envoyes": true, "non_selectionnable": false } ],
  "envoyes": "Envoy&AOk-s" }
```

`dossiers` : noms IMAP (inchangé) ; `dossiers_detail` : noms décodés et attributs ; `envoyes` : dossier des envoyés repéré (ou `null`). En échec : `message` en clair (« Mot de passe refusé : … »), `detail` (erreur brute). Les tests faits avant ce changement n'ont que `dossiers`.

### `POST /api/admin/boites/{id}/tester-smtp`

Essai SMTP (envoi des réponses) d'une boîte enregistrée : connexion et authentification, **rien n'est envoyé**. `202 {"id": ...}` ; `409 {"code": "smtp_absent"}` si la boîte n'a pas de serveur SMTP ; `404` si la boîte n'existe pas.

### `GET /api/admin/boites/essais/{id}`

À relire toutes les 1 à 2 s tant que `etat` vaut `en_cours` (`attendreEssai` dans `api.ts`). Le mot de passe n'est jamais rendu, et le worker l'efface de la demande à la fin de l'essai. Les essais de plus d'une heure sont effacés au suivant. `404` si l'essai n'existe pas (ou plus).

```json
{
  "etat": "termine", "cree_le": "...", "termine_le": "...", "boite_id": null,
  "identifiant": "compta@frouard.fr",
  "imap": { "serveur": "mail.infomaniak.com", "port": 993 },
  "smtp": { "serveur": "mail.infomaniak.com", "port": 465, "securite": "tls" },
  "resultat_imap": {
    "ok": true, "message": "Connexion réussie : 7 dossiers trouvés.", "detail": null,
    "dossiers": [ { "nom": "Envoy&AOk-s", "affiche": "Envoyés", "envoyes": false, "non_selectionnable": false } ],
    "reception": "INBOX", "envoyes": "Envoy&AOk-s"
  },
  "resultat_smtp": { "ok": false, "message": "Mot de passe refusé par le serveur d'envoi : il faut un mot de passe d'application.", "detail": "..." }
}
```

- `dossiers[].nom` : nom IMAP à enregistrer tel quel (`dossiers`, `dossier_envoyes` de la boîte) ; `affiche` : nom décodé pour l'écran.
- `reception` : `INBOX` (à cocher par défaut) ; `envoyes` : dossier des envoyés repéré (attribut `\Sent`, sinon nom usuel : Sent, Envoyés, Éléments envoyés, `[Gmail]/Messages envoyés`…), `null` si rien de sûr.
- `message` : erreur dite en clair (« Mot de passe refusé : pour Gmail et Infomaniak, il faut un mot de passe d'application… », « Serveur IMAP introuvable… », « Le serveur IMAP refuse la connexion : vérifiez le port (993)… », « Connexion sécurisée impossible… », « Le serveur IMAP ne répond pas… ») ; `detail` : erreur brute (admins).
- Déploiement : un worker plus ancien que l'app ne connaît pas la tâche `Essai` ; elle échoue et l'essai reste `en_cours` jusqu'à la mise à jour du worker (Watchtower : quelques minutes).

## 7. Bilan du mois

### `GET /api/bilan?mois=AAAA-MM&societe=`

`mois` : mois en cours par défaut ; `400` si mal formé.

```json
{
  "mois": "2026-11",
  "societes": [LigneSuivi],
  "bl_en_attente": [ { "societe_id": 1, "societe": "Frouard", "fournisseur_id": 7,
                       "fournisseur": "Grossiste Durand", "bl": 2 } ],
  "qualite": { "lecture": Bilan, "tri": [SemaineTri] }
}
```

- `societes` : mêmes colonnes que `GET /api/depots/suivi?mois=` (reçues, déposées, prêtes, à vérifier, en erreur, doublons, montant déposé : factures moins avoirs), avec la règle commune : un responsable ne compte que son périmètre ; avec `?societe=` (ou pour un responsable), les factures pas encore rangées reçues par les boîtes de la société comptent dans sa ligne. Sans filtre, chez un admin, une ligne `societe_id: null` regroupe les factures pas encore rangées. « À valider » = `pretes + a_verifier`. Un responsable qui demande une autre société : listes vides.
- `bl_en_attente` : par société et fournisseur, BL datés du mois sans facture rapprochée (ni écartés), même règle.
- `bl_sans_lignes` : BL lus avant le rapprochement, sans leurs lignes (tout l'historique, même règle) : le front n'a plus à appeler `/api/rapprochement` pour ce nombre.
- `qualite` (partie repliée) : `GET /api/documents/bilan` et `GET /api/tri/semaines` tels quels (périmètre appliqué, mais pas `?societe=` ni `mois`).

## 8. Fiche mail et réponses (routes existantes, enrichies)

`GET /api/mails/{id}` :
- `boites[]` : `{ "boite_id", "libelle", "dossier", "adresse": "compta@frouard.fr", "societe_id", "societe", "smtp": true }` (une ligne par dossier ; `adresse` pour « Envoyer depuis compta@… », `smtp` : la boîte peut envoyer).
- `fournisseur_id`, `fournisseur` : fiche reconnue par le domaine de l'expéditeur (`null` sinon), pour le nom en tête de la fiche et l'encadré du fil fournisseur quand le mail n'a pas de pièce lue.
- `pieces[]` : en plus de `document_id`, `document_etat`, `document_type`, `document_voie` : `document_statut_depot`, `document_verifie`, `document_numero`, `document_montant_ttc`, `document_fournisseur_id`, `document_fournisseur` (fiche, à défaut nom lu), `document_societe_id`, `document_societe`, `document_outil` (`dext`/`yooz`), `document_envoye_le` (dernier envoi réussi) : la liste des pièces se lit sans ouvrir chaque fiche document.
- `brouillons[]` (et toute réponse proposée) : `boite_adresse` (adresse de la boîte d'envoi).

## 9. Réinitialisation (Réglages > Avancé > Zone dangereuse)

### `POST /api/admin/reinitialiser` (admin)

Corps `{ "confirmation": "RÉINITIALISER" }` (sinon `400`). Efface dans une transaction tout ce qui vient des mails : `mail`, `mail_boite`, `piece_jointe`, `document`, `ligne_document`, `rapprochement`, `envoi_depot`, `brouillon_reponse`, `boite_dossier`, `mail_hors_societe`, `fournisseur`, `fournisseur_distinct`, `journal`, `appel_claude`, et les tâches apalis pas encore prises (lecture, tri, rédaction, dépôts et réponses, relèves). Garde `societe`, `boite_mail` (relève remise à zéro : `derniere_releve`, `derniere_erreur`, `erreur_depuis`, `alerte_le`, `releve_jusqu_a` vides, `echecs_auth` 0, `pause_auto` levée), `utilisateur`, `parametre`, `seance_claude`, `expediteur_marque`, les sessions et les `.eml` sur disque. Écrit une ligne d'historique `reinitialisation` (qui, combien), puis pousse une relève par boîte active : sans `boite_dossier`, elle repart de `SINCE aujourd'hui - reprise_mois`.

Réponse : `{ "mails", "documents", "fournisseurs", "taches", "boites_relancees" }`. `409` si une relève tient son bail, si un envoi (Dext/Yooz ou réponse) a commencé depuis moins de 15 min, ou si la base reste verrouillée (message à afficher tel quel).
