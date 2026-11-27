-- Lecture des tableurs, des scans et des PDF de plusieurs pièces (audit du 2026-11-26 : L15, L19,
-- L20). Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- L15 : correspondances colonnes → champs des tableurs de ce fournisseur, données une fois par
-- Claude quand les intitulés n'ont pas été reconnus. Clé : empreinte des intitulés de la ligne
-- d'en-tête (`tableau_articles::empreinte`), valeur : {"champ": colonne, ...}.
alter table fournisseur
    add column if not exists colonnes_tableur jsonb not null default '{}'::jsonb;

-- L19 : un PDF qui regroupe plusieurs pièces (5 BL, 3 factures) est découpé. Chaque pièce est
-- une pièce jointe « partie » du fichier d'origine (`source_id`), pages `page_debut` à
-- `page_fin` (à partir de 1), avec son propre document ; son contenu est extrait de la pièce
-- d'origine à la demande (aperçu, lecture, dépôt). Rang négatif : jamais celui d'une vraie
-- pièce du mail.
alter table piece_jointe
    add column if not exists source_id bigint null references piece_jointe on delete cascade,
    add column if not exists page_debut int null,
    add column if not exists page_fin int null;
create index if not exists piece_jointe_source on piece_jointe (source_id)
    where source_id is not null;
