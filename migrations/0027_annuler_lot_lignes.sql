-- Retour en arrière, validation par lot et lignes corrigées à la main (audit du 2026-11-26 :
-- U1, U2, U23, L16).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- U1 / U2 : envoi à Dext/Yooz demandé par une validation, parti seulement après un délai
-- pendant lequel la validation s'annule. Une ligne par document validé ; `lot` regroupe les
-- validations d'un même clic (une pièce, ou toutes celles d'une validation par lot).
--   attente   : envoi pas encore pris par le worker, annulable ;
--   pris      : le worker a pris l'envoi (statut `en_envoi` posé dans la même transaction) ;
--   annule    : validation annulée, la pièce a retrouvé son état d'avant (`avant`) ;
--   sans_suite: le worker l'a pris mais n'a rien envoyé (doublon, adresse manquante…).
-- Passages `attente` → autre par un `update … where etat = 'attente'`, document verrouillé :
-- l'annulation et le worker ne peuvent pas gagner tous les deux.
create table if not exists envoi_prevu (
    id          bigint generated always as identity primary key,
    document_id bigint not null references document on delete cascade,
    lot         uuid not null,
    auteur_id   bigint null references utilisateur on delete set null,
    forcer      bool not null default false,
    prevu_pour  timestamptz not null,
    etat        text not null default 'attente'
                check (etat in ('attente', 'pris', 'annule', 'sans_suite')),
    -- Champs du document avant la validation (corrections comprises), remis par l'annulation.
    avant       jsonb not null default '{}',
    cree_le     timestamptz not null default now(),
    fini_le     timestamptz null,
    annule_par  bigint null references utilisateur on delete set null
);
create index if not exists envoi_prevu_lot on envoi_prevu (lot);
create index if not exists envoi_prevu_document on envoi_prevu (document_id);
create index if not exists envoi_prevu_attente on envoi_prevu (prevu_pour) where etat = 'attente';

-- U1 / U23 : écartement d'une pièce, pour la « Remettre à contrôler » :
-- {"le", "par", "motif" (doublon | pas_pour_nous | autre | null), "texte", "avant": {etat,
-- statut_depot, controles, verifie_le, verifie_par}}. null : jamais écartée par un humain
-- depuis cette version.
alter table document add column if not exists ecartement jsonb null;

-- L16 : ligne corrigée (ou ajoutée) à la main, et par qui. Une relecture remplace les lignes.
alter table ligne_document add column if not exists corrigee_le timestamptz null;
alter table ligne_document add column if not exists corrigee_par bigint null
    references utilisateur on delete set null;
