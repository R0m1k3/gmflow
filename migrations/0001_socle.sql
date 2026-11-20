-- Lot 1 (Socle) : structures, boîtes surveillées, mails, comptes, paramètres, journal.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

create table societe (
    id              bigint generated always as identity primary key,
    raison_sociale  text not null,
    siret           text null,
    tva             text null,
    adresses        jsonb not null default '[]',
    outil_depot     text null check (outil_depot in ('dext', 'yooz')),
    adresse_depot   text null,
    archivee        bool not null default false,
    cree_le         timestamptz not null default now()
);

create table boite_mail (
    id                    bigint generated always as identity primary key,
    societe_id            bigint not null references societe on delete restrict,
    libelle               text not null,
    serveur               text not null,
    port                  int not null default 993,
    identifiant           text not null,
    mot_de_passe_chiffre  bytea not null,
    dossiers              text[] not null default '{INBOX}',
    reprise_mois          int not null default 3 check (reprise_mois between 0 and 24),
    active                bool not null default false,
    pause_auto            bool not null default false,
    echecs_auth           int not null default 0,
    releve_jusqu_a        timestamptz null,
    derniere_releve       timestamptz null,
    derniere_erreur       text null,
    erreur_depuis         timestamptz null,
    dernier_test          jsonb null,
    cree_le               timestamptz not null default now()
);

create table boite_dossier (
    boite_id     bigint not null references boite_mail on delete cascade,
    nom          text not null,
    uidvalidity  bigint not null,
    dernier_uid  bigint not null default 0,
    primary key (boite_id, nom)
);

create table mail (
    id                 bigint generated always as identity primary key,
    message_id         text not null,
    empreinte_contenu  bytea not null,
    expediteur         text not null,
    destinataires      text[] not null,
    objet              text not null,
    date_mail          timestamptz null,
    corps_texte        text not null,
    chemin_eml         text not null,
    sha256             bytea not null,
    taille             bigint not null,
    recu_le            timestamptz not null default now(),
    unique (message_id, empreinte_contenu)
);
create index mail_date on mail (date_mail);

create table mail_boite (
    id        bigint generated always as identity primary key,
    mail_id   bigint not null references mail on delete cascade,
    boite_id  bigint not null references boite_mail on delete cascade,
    dossier   text not null,
    uid       bigint null,
    vu_le     timestamptz not null default now(),
    unique (boite_id, dossier, uid)
);
create index mail_boite_mail on mail_boite (mail_id);
create index mail_boite_dossier on mail_boite (boite_id, dossier);

create table piece_jointe (
    id         bigint generated always as identity primary key,
    mail_id    bigint not null references mail on delete cascade,
    rang       int not null,
    nom        text not null,
    type_mime  text not null,
    taille     bigint not null,
    sha256     bytea not null,
    unique (mail_id, rang)
);

create table utilisateur (
    id           bigint generated always as identity primary key,
    identifiant  text not null unique,
    hash_mdp     text not null,
    role         text not null check (role in ('admin', 'responsable')),
    societe_id   bigint null references societe on delete restrict,
    actif        bool not null default true,
    cree_le      timestamptz not null default now(),
    check (role = 'admin' or societe_id is not null)
);

create table parametre (
    cle              text primary key,
    valeur           jsonb null,
    valeur_chiffree  bytea null,
    modifie_le       timestamptz not null default now()
);

create table seance_claude (
    id            uuid primary key default gen_random_uuid(),
    etat          text not null check (etat in ('demarrage', 'attente_code', 'code_recu', 'terminee', 'erreur', 'expiree')),
    url           text null,
    code_chiffre  bytea null,
    message       text null,
    cree_le       timestamptz not null default now(),
    expire_le     timestamptz not null default now() + interval '10 minutes'
);
create unique index seance_ouverte on seance_claude ((true))
    where etat in ('demarrage', 'attente_code', 'code_recu');

create table journal (
    id          bigint generated always as identity primary key,
    quand       timestamptz not null default now(),
    auteur_id   bigint null references utilisateur on delete set null,
    action      text not null,
    objet_type  text not null,
    objet_id    text null,
    details     jsonb not null default '{}'
);
create index journal_quand on journal (quand);

create table appel_claude (
    id        bigint generated always as identity primary key,
    quand     timestamptz not null default now(),
    tache     text not null,
    modele    text not null,
    effort    text null,
    duree_ms  int not null,
    succes    bool not null,
    message   text null
);
