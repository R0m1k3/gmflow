-- Fournisseurs d'un groupe : une société (U10, un SIREN) facture pour plusieurs marques
-- (L3C, Homea, Decor10), chacune avec son code BO Phénix. Un document du groupe va à la
-- marque dont un nom figure dans son texte.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table fournisseur
    -- Fiche du groupe (un seul niveau : un groupe n'a pas lui-même de groupe).
    add column groupe_id bigint null references fournisseur on delete set null,
    -- Noms à chercher dans le texte des documents du groupe (« HOMEA », « Decor 10 »), en
    -- plus du nom de la fiche.
    add column noms_documents text[] not null default '{}';
create index fournisseur_groupe on fournisseur (groupe_id);
