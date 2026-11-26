-- Lecture : essais de suite en échec technique (délai dépassé, panne de Claude). Au troisième,
-- le document passe en erreur au lieu d'être relancé toutes les 30 minutes ; remis à 0 à
-- chaque lecture enregistrée.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table document add column if not exists essais_lecture integer not null default 0;
