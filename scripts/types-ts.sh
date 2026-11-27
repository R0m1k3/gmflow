#!/bin/sh
# Régénère les types TypeScript du front (web/src/lib/api-types/) depuis les structures Rust de
# l'API (ts-rs, fonction `ts`). À lancer après toute modification d'un type rendu ou reçu par
# l'API, puis commiter le dossier : la CI rejoue ce script et échoue si le résultat diffère.
set -eu
cd "$(dirname "$0")/.."
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

# Deux types de même nom écriraient le même fichier : chaque nom TypeScript doit être unique
# (nom de la structure Rust, ou `rename`).
doublons=$(
  {
    find crates -name '*.rs' -print0 | xargs -0 awk '
      FNR == 1 { attente = 0 }
      /derive\((ts_rs::)?TS\)/ { attente = 1; nom = "" }
      attente && match($0, /rename = "[A-Za-z0-9_]+"/) { nom = substr($0, RSTART + 10, RLENGTH - 11) }
      attente && match($0, /(struct|enum) [A-Za-z0-9_]+/) {
        split(substr($0, RSTART, RLENGTH), m, " ")
        print (nom != "" ? nom : m[2]); attente = 0
      }'
    sed -n 's/^    \([A-Z][A-Za-z0-9]*\) {.*/\1/p' crates/gmflow-core/src/vocabulaire.rs
  } | sort | uniq -d
)
if [ -n "$doublons" ]; then
  echo "Noms de types TypeScript en double (ajouter un rename) : $doublons" >&2
  exit 1
fi

rm -rf web/src/lib/api-types
cargo test --quiet -p gmflow-core -p gmflow-app --features gmflow-app/ts --lib export_bindings >/dev/null
# Point d'entrée unique, repris par web/src/lib/types.ts.
(
  cd web/src/lib/api-types
  echo "// Généré par scripts/types-ts.sh : ne pas modifier à la main."
  for f in *.ts; do echo "export type * from './$f';"; done
) > web/src/lib/api-types/index.ts.tmp
mv web/src/lib/api-types/index.ts.tmp web/src/lib/api-types/index.ts
echo "Types régénérés : $(find web/src/lib/api-types -name '*.ts' | wc -l) fichiers dans web/src/lib/api-types/"
