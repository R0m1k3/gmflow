# Refonte UI (direction B, « trois volets ») : plan de travail

Références : `docs/architecture.md` (organisation validée par Michael) et `docs/maquette-b.html` (direction visuelle validée : à reproduire fidèlement — typo Schibsted Grotesk + Geist Mono, neutres blanc cassé/gris froids, un seul accent bleu tampon #1a47d1, ambre = champ à contrôler, rouge = bloqué/urgent, filets plutôt que cartes, raccourcis clavier affichés en touches).

Branche `refonte-ui-b`, worktree `/root/dev/gmflow-ui-b`. Aucune route serveur existante n'est supprimée (on ajoute) ; le front est réécrit.

## Routes du front (SvelteKit, adapter-static)

| Route | Écran | Remplace |
|---|---|---|
| `/` | **À traiter** en volets : file à gauche (Bloqué / À valider / À répondre), élément ouvert à droite via `?doc=ID` ou `?mail=ID`. Raccourcis J/K, Entrée, Échap. | `/`, `/a-repondre`, `/depots` |
| `/documents/[id]` | Fiche document plein écran (aperçu + champs + BL liés + envoi + mail d'origine), bouton « Valider et envoyer à Dext/Yooz » puis suivant | `/documents/[id]`, `/rapprochement` |
| `/mails/[id]` | Fiche mail (mail + pièces à gauche, réponse à droite) | `/mails/[id]` |
| `/recherche?q=` | Recherche unique documents + mails | `/documents`, `/mails` |
| `/fournisseurs`, `/fournisseurs/[id]` | Liste par société + fiche (fil par mois, encadré À rapprocher) | `/fournisseurs`, `/rapprochement` |
| `/bilan` | Bilan du mois par société + qualité lecture/tri replié | Dépôts > suivi, bilans |
| `/reglages` | Réglages unique : mise en route 5 étapes + sections + Avancé | `/admin/*` |
| `/reglages/boites/nouvelle`, `/reglages/boites/[id]` | Assistant d'ajout (3 écrans) / fiche d'une boîte | `/admin/boites/*` |
| `/connexion`, `/premier-compte` | inchangées dans leur rôle, restylées | |

Les anciennes URL (`/a-repondre`, `/depots`, `/rapprochement`, `/documents`, `/mails`, `/admin/...`) redirigent vers la nouvelle (petit `+page.ts` avec `redirect`), pour les favoris et les liens dans les mails d'alerte.

Un responsable de magasin (rôle non admin) ne voit ni le sélecteur de société, ni la pastille d'état, ni Réglages.

## Découpage et propriété des fichiers

Phase 1 (en parallèle)
- **Serveur** : `crates/**`, `.sqlx/`, `migrations/` si besoin, et les ajouts de types dans `web/src/lib/types.ts` + fonctions d'appel dans `web/src/lib/api.ts` (seul à toucher ces deux fichiers). Contrat des nouveaux endpoints écrit dans `docs/api-refonte.md`.
- **Socle front** : `web/src/app.css`, `web/static/polices/`, `web/src/routes/+layout.svelte`, `web/src/lib/ui/**` (composants partagés), `web/src/lib/clavier.ts`, `web/src/lib/libelles.ts` (vocabulaire de l'architecture §4), `docs/ui.md` (guide d'usage des composants pour les pages).

Phase 2 (en parallèle, après la phase 1) : un agent par groupe de routes, chacun ne touche que ses routes (+ ajouts en fin de `app.css` si vraiment nécessaire, commentés).
1. `/` (À traiter) + `/documents/[id]`
2. `/mails/[id]` + `/recherche` + redirections de `/documents`, `/mails`, `/a-repondre`
3. `/fournisseurs/**` + `/bilan` + redirections de `/depots`, `/rapprochement`
4. `/reglages/**` + redirections `/admin/**` + `/connexion`, `/premier-compte`

Phase 3 : intégration, essai bout en bout sur base jetable avec captures, PR.
