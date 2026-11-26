-- Marque d'un groupe reconnue au début d'un code du document (U10 : « LC54023001 » = L3C,
-- « LD… » = Decor10, « LH… » = Homea), quand le BL ne nomme pas la marque.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table fournisseur add column if not exists prefixes_codes text[] not null default '{}';
