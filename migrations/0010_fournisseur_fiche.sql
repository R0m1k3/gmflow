-- Fiche fournisseur plus complète (coordonnées, conditions) et paires de fiches marquées
-- « pas un doublon » (plus proposées à la fusion).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table fournisseur
    add column adresse             text null,
    add column telephone           text null,
    add column email               text null,
    add column interlocuteur       text null,
    add column conditions_paiement text null;

create table fournisseur_distinct (
    -- a < b : une seule ligne par paire.
    a       bigint not null references fournisseur on delete cascade,
    b       bigint not null references fournisseur on delete cascade,
    par     bigint null references utilisateur on delete set null,
    quand   timestamptz not null default now(),
    primary key (a, b),
    check (a < b)
);
