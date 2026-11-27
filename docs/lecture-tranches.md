# Lecture par tranches des très gros PDF

Suite du constat L2 de l'audit du 2026-11-26. Code : `crates/gmflow-core/src/lecture/tranches.rs`
(plan, prompts, schémas, fusion, contrôle, sans appel) et `crates/gmflow-worker/src/lecture/tranches.rs`
(appels, avancement en base). Migration `0031_lecture_tranches.sql` (table `lecture_tranche`).

## Quand

Un PDF texte, ou un scan dont l'OCR local a lu les pages (`ocr::TexteOcr::textes_pages`), est lu
en plusieurs parties quand il a au moins deux pages et que son texte dépasse `TEXTE_MAX`
(20 000 caractères) ou compte plus de `LIGNES_MAX` (200) lignes chiffrées. Une page sans texte
(scan que l'OCR n'a pas lu) compte comme pleine. Les tableurs sont déjà lus en entier par le code ;
un scan sans OCR (outils absents) garde la lecture en un appel.

## Comment

1. **Plan** : tranches de pages entières, contiguës, au plus 9 000 caractères, 120 lignes de texte et
   4 pages chacune ; au plus `TRANCHES_MAX` (15) tranches. Au-delà, les pages restantes ne sont pas
   lues et la pièce est marquée incomplète.
2. **En-tête et totaux** (partie 1) : prompt de lecture habituel sur les 2 premières et les 2
   dernières pages, `lignes` vide, plus `nombre_lignes_annonce` et `quantite_totale_annoncee`. Un
   document qui n'est ni facture, ni avoir, ni BL s'arrête là (pas de lignes à payer).
3. **Lignes**, tranche par tranche, l'une après l'autre (tâche `lignes_tranche`, schéma `{lignes}`).
   Chaque tranche reçoit la fin de la page précédente (« AVANT ») et le début de la suivante
   (« APRÈS ») en contexte : une ligne à cheval appartient à la page où elle commence. En-têtes de
   tableau répétés, reports, sous-totaux : exclus par la consigne puis par le code (`parasite`).
4. **Fusion** par le code : lignes dans l'ordre, doublon de jonction retiré (`meme_ligne` : mêmes
   nombres et même référence, ou morceau sans référence).
5. **Contrôle de complétude** : pages non lues, somme des lignes = total HT lu (avec ou sans annexes,
   éco-participations des lignes, remise de pied, à 0,05 € près), nombre de lignes et quantité
   totale annoncés. Si quelque chose ne tombe pas juste et que toutes les pages sont lues, l'en-tête
   est relu une fois (modèle du nouvel essai, PDF joint) ; la lecture qui a le moins d'écarts est
   gardée. Pas complet : `lignes_tronquees` reste posé (totaux non calculés, pas de comparaison BL,
   alerte à l'export Phénix) et le message dit pourquoi (« lecture incomplète (document lu en 6
   parties) : pages 4 à 6 non lues »). Rien d'imprimé pour contrôler : la pièce n'est pas marquée
   incomplète mais « à vérifier » (« document lu en N parties, sans total ni nombre de lignes… »).

Bilan dans `document.confiance.tranches` : `parties`, `pages`, `lues`, `pages_non_lues`,
`doublons_retires`, `complet`, `verifie`, `raisons`.

## Coût et réessais

- Modèle : réglage `extraction_texte` (PDF texte) ou `ocr_scan` (scan), haiku par défaut ; chaque
  appel compte dans le quota du jour et dans `appel_claude` (`cout_usd`). « Relire en détail » relit
  tout avec le réglage `nouvel_essai`, PDF joint.
- PDF joint : toujours pour un scan (le fichier fait foi, Read limité aux pages de la tranche) ;
  pour un PDF texte seulement aux nouveaux essais.
- Tranche inexploitable (réponse non conforme, vide alors que les pages sont chiffrées, panne) :
  réessayée seule une fois (nouvel essai, PDF joint), puis notée `echec` ; les autres sont gardées.
- Quota du jour atteint ou refus (limite d'usage, jeton) : le document repasse « à lire » avec
  « Lecture en 6 parties : 3 déjà lues, gardées pour la suite » ; la tâche suivante ne relit que ce
  qui manque. `lecture_tranche` est effacée à la fin de la lecture et à chaque relecture demandée.
- Pendant la lecture, la fiche montre « Lecture en cours, partie 3 sur 6 » (`attente::expliquer`) ;
  chaque partie lue repousse `pris_le` (le document n'est pas tenu pour bloqué).

Ordre de grandeur, BL PDF de 400 lignes sur 14 pages : 1 appel d'en-tête + 5 tranches = 6 appels
haiku, sortie dominée par les lignes (environ 80 jetons par ligne).
