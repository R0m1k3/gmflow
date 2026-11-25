-- Suivi Logiflow : échéance lue sur les factures, et livraison Logiflow retrouvée pour chaque
-- BL de GMFlow par la vérification régulière (toutes les heures).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table document add column if not exists date_echeance date null;

create table if not exists logiflow_livraison (
    bl_id          bigint primary key references document on delete cascade,
    vu_le          timestamptz not null default now(),
    -- trouve : une livraison ; ambigu : plusieurs possibles (livraison_id null).
    etat           text not null,
    possibles      int not null default 1,
    livraison_id   bigint null,
    magasin        text null,
    fournisseur    text null,
    bl             text null,
    livree_le      date null,
    montant_bl     numeric(14, 2) null,
    facture        text null,
    facture_ht     numeric(14, 2) null,
    facture_ttc    numeric(14, 2) null,
    echeance       date null,
    rapprochee     bool not null default false
);
