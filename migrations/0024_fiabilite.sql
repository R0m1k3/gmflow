-- Lot « fiabilité et sécurité » (audit du 2026-11-26).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- C1 : la réinitialisation garde la trace des dépôts Dext/Yooz. `envoi_depot` pointe vers
-- `document` (effacé) : ses lignes sont copiées ici, avec ce qu'il faut pour savoir quelle pièce
-- est partie, avant d'être effacées. Sans clé vers `document` : la copie survit aux
-- réinitialisations suivantes.
create table if not exists envoi_depot_archive (
    id                bigint generated always as identity primary key,
    envoi_id          bigint not null,
    document_id       bigint not null,
    societe_id        bigint not null,
    adresse           text not null,
    outil             text null,
    quand             timestamptz not null,
    etat              text not null,
    message           text null,
    message_id_smtp   text null,
    auteur_id         bigint null references utilisateur on delete set null,
    document_type     text null,
    document_numero   text null,
    document_date     date null,
    montant_ttc       numeric(14, 2) null,
    fournisseur_nom   text null,
    nom_fichier       text null,
    archive_le        timestamptz not null default now()
);
create index if not exists envoi_depot_archive_quand on envoi_depot_archive (quand);

-- C3 : alerte « boîte pas relevée depuis longtemps, sans erreur » (une par incident), et date
-- d'activation (une boîte réactivée n'est pas en retard tant qu'elle n'a pas eu sa chance).
alter table boite_mail add column if not exists alerte_silence_le timestamptz null;
alter table boite_mail add column if not exists activee_le timestamptz null;

-- C5 : réservation de l'envoi d'une réponse, posée juste avant le SMTP : une tâche rejouée
-- (worker tué, file relancée) ne renvoie jamais la même réponse.
alter table brouillon_reponse add column if not exists envoi_pris_le timestamptz null;

-- C15 : un seul compte par identifiant, sans tenir compte de la casse. Si deux comptes ne
-- diffèrent déjà que par la casse, l'index n'est pas créé (la migration ne doit pas échouer) :
-- l'application refuse de toute façon les nouveaux doublons.
do $$
begin
    if not exists (
        select 1 from utilisateur group by lower(identifiant) having count(*) > 1
    ) then
        create unique index if not exists utilisateur_identifiant_casse
            on utilisateur (lower(identifiant));
    end if;
end
$$;
