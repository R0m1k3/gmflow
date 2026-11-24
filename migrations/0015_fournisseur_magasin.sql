-- Fournisseurs du serveur du magasin (Apiflow, /api/fournisseurs), copiés par le worker :
-- sert à retrouver le code BO Phénix (`fournisseur.code_gestion`) d'une fiche.
create table if not exists fournisseur_magasin (
    code text primary key,
    nom text,
    adresse text,
    telephone text,
    email text,
    nb_articles bigint,
    nb_actifs bigint,
    vu_le timestamptz not null default now()
);
