# GMFlow

Collecte des factures et bons de livraison des fournisseurs depuis les boîtes mail des structures
(magasins La Foir'Fouille de Frouard et Houdemont). Cahier des charges : 5 lots ; le lot 1 (socle)
relève les mails et les affiche.

## Organisation

- `crates/gmflow-core` : configuration, base (`db::initialiser`), chiffrement, file de tâches apalis
  (`file.rs`), lecture des `.eml` (`eml.rs`), IMAP (`imap.rs`), stockage des `.eml`, appels Claude CLI.
- `crates/gmflow-app` : API axum (`/api/*`) et frontend statique ; sessions en base.
- `crates/gmflow-worker` : relève IMAP, test des boîtes, séance `claude setup-token`, test du jeton,
  planification toutes les 5 min, `/health` sur 8081.
- `web/` : SvelteKit 3 (SPA, `adapter-static`). Imports internes par `#lib/xxx.ts` (`$lib` n'existe plus).
- `migrations/` : SQL de GMFlow. apalis et les sessions ont leurs propres migrations, lancées par
  `db::initialiser` sous un verrou consultatif.

L'app et le worker ne se parlent que par la base. Toute connexion IMAP et tout appel à Claude part du worker.

## Règles

- **Une migration ne fait qu'ajouter** (tables, colonnes nulles ou avec défaut, index). Retirer ou
  renommer se fait dans une version ultérieure, quand plus aucun code ne s'en sert : Watchtower peut
  mettre à jour l'app et le worker à quelques minutes d'écart.
- Toute tâche apalis est poussée avec `max_attempts` explicite (sinon 25 essais) : passer par
  `gmflow_core::file::pousser_*`.
- Secrets (mots de passe IMAP, jeton Claude, code de séance) : chiffrés avec `Chiffreur`, jamais
  renvoyés par l'API ni écrits dans le journal ou les logs.
- Lectures d'un responsable : toujours filtrer par `Acces::societe_filtre()` (motif
  `($n::bigint is null or b.societe_id = $n)` via `mail_boite`).
- Mutations : l'en-tête `X-GMFlow: 1` est exigé (`web/src/lib/api.ts` le pose).
- Claude CLI : jamais `--bare` (il ignore le jeton OAuth) ; passer par `gmflow_core::claude::appeler`.
- Noms en français dans le code et la base.

## Développement

Requêtes SQL vérifiées à la compilation : une base à jour est nécessaire (`.env` : `DATABASE_URL=...`),
ou `SQLX_OFFLINE=true` avec `.sqlx/`. Après toute modification de requête :

```sh
sqlx migrate run
cargo sqlx prepare --workspace   # puis commiter .sqlx/
```

Tests (PostgreSQL et un serveur IMAP de test avec les comptes `a@test.local` et `b@test.local`,
mot de passe `mdp`) :

```sh
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d postgres greenmail   # ou équivalent local
GMFLOW_TEST_DATABASE_URL=postgres://<rôle CREATEDB>@localhost:5432/postgres \
GMFLOW_TEST_IMAP=localhost:3143 cargo test --workspace --all-features
cargo fmt --all && cargo clippy --workspace --all-features --all-targets -- -D warnings
cd web && npm ci && npm run check && npm run build
```

Chaque test crée sa base `gmflow_test_<uuid>` et ses dossiers IMAP `T<uuid>` : les tests tournent en parallèle.
La fonction `test-utils` (IMAP en clair, outils de test) n'est jamais activée dans les images publiées.

## Déploiement

GitHub Actions publie `ghcr.io/r0m1k3/gmflow-app:latest` et `gmflow-worker:latest` (et `sha-<court>`)
à chaque push sur `main` et chaque lundi. Stack Portainer depuis ce dépôt (`docker-compose.yml`,
variables de `.env.example`), Watchtower pour les mises à jour, Nginx Proxy Manager vers `gmflow-app:8347`
(port réglable par `GMFLOW_PORT`, jamais publié sur l'hôte).
Premier démarrage : l'URL du premier compte administrateur est dans les logs de l'app (valable 1 h).
