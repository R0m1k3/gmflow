-- Lecture des tableurs, des scans et des PDF de plusieurs pièces (audit du 2026-11-26 : L15, L19,
-- L20). Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- L15 : correspondances colonnes → champs des tableurs de ce fournisseur, données une fois par
-- Claude quand les intitulés n'ont pas été reconnus. Clé : empreinte des intitulés de la ligne
-- d'en-tête (`tableau_articles::empreinte`), valeur : {"champ": colonne, ...}.
alter table fournisseur
    add column if not exists colonnes_tableur jsonb not null default '{}'::jsonb;
