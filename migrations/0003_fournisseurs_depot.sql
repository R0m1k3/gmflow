-- Lot 3 (Fournisseurs et dépôt) : référentiel des fournisseurs, contrôles avant dépôt, dépôt
-- par mail dans Dext ou Yooz, alertes par mail.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

create table fournisseur (
    id                bigint generated always as identity primary key,
    nom               text not null,
    -- Nom sans accents, formes juridiques ni ponctuation (rapprochement des noms).
    nom_normalise     text not null,
    -- SIRET en chiffres seuls, TVA en majuscules sans espaces.
    siret             text null,
    tva               text null,
    -- Domaines des adresses d'expédition (factures@durand.fr -> durand.fr).
    domaines          text[] not null default '{}',
    statut            text not null default 'a_confirmer' check (statut in ('a_confirmer', 'valide')),
    societe_defaut_id bigint null references societe on delete set null,
    format_bl         text null,
    notes             text null,
    -- Fiche fusionnée dans une autre (doublon « METRO » / « Metro Cash & Carry »).
    fusionne_dans     bigint null references fournisseur on delete set null,
    cree_le           timestamptz not null default now()
);
create index fournisseur_siret on fournisseur (siret);
create index fournisseur_tva on fournisseur (tva);
create index fournisseur_nom_normalise on fournisseur (nom_normalise);
create index fournisseur_domaines on fournisseur using gin (domaines);

alter table document
    add column fournisseur_id bigint null references fournisseur on delete set null,
    -- Factures et avoirs seulement : a_verifier (un contrôle a échoué), pret (contrôles passés,
    -- en attente de validation ou de dépôt automatique), en_envoi, depose, erreur (envoi en
    -- échec), doublon (bloqué).
    add column statut_depot text null
        check (statut_depot in ('a_verifier', 'pret', 'en_envoi', 'depose', 'erreur', 'doublon')),
    -- Contrôles en échec : [{"code": "montants", "message": "..."}].
    add column controles jsonb not null default '[]',
    add column valide_le timestamptz null,
    add column valide_par bigint null references utilisateur on delete set null,
    -- Corrections humaines champ par champ : {"numero": {"avant": "..", "apres": ".."}}.
    add column corrections jsonb not null default '{}',
    add column corrige_le timestamptz null;
create index document_fournisseur on document (fournisseur_id);
create index document_statut_depot on document (statut_depot);

create table envoi_depot (
    id              bigint generated always as identity primary key,
    document_id     bigint not null references document on delete cascade,
    societe_id      bigint not null references societe on delete restrict,
    adresse         text not null,
    outil           text null,
    quand           timestamptz not null default now(),
    etat            text not null default 'en_cours' check (etat in ('en_cours', 'envoye', 'erreur')),
    message         text null,
    message_id_smtp text null,
    auteur_id       bigint null references utilisateur on delete set null
);
create index envoi_depot_document on envoi_depot (document_id);
create index envoi_depot_quand on envoi_depot (quand);

-- Dépôt automatique des factures qui passent tous les contrôles (activé à la main, plus tard).
alter table societe add column depot_auto bool not null default false;

-- Alerte « relève en échec depuis plus de 30 min » envoyée pour l'incident en cours.
alter table boite_mail add column alerte_le timestamptz null;
