-- « Ce mail ne concerne pas cette société » : le mail disparaît des listes de la société (de
-- toutes s'il n'en concerne aucune). Les écrans lisent les boîtes d'un mail par la vue
-- mail_boite_vue, qui omet les boîtes des sociétés masquées.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

create table mail_hors_societe (
    mail_id    bigint not null references mail on delete cascade,
    societe_id bigint not null references societe on delete cascade,
    par        bigint null references utilisateur on delete set null,
    quand      timestamptz not null default now(),
    -- Documents écartés en même temps, et leur état d'avant (pour « Annuler ») :
    -- [{"id": 1, "etat": "lu", "statut_depot": "pret"}].
    documents  jsonb not null default '[]',
    primary key (mail_id, societe_id)
);

create view mail_boite_vue as
    select mb.*
    from mail_boite mb
    where not exists (
        select 1 from mail_hors_societe x join boite_mail b on b.id = mb.boite_id
        where x.mail_id = mb.mail_id and x.societe_id = b.societe_id
    );
