-- Audit du 2026-11-26 : IBAN lu et alerte « IBAN changé » (L12), mesure du coût et de la justesse
-- de la lecture (L18).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- L12 : IBAN (et BIC) du fournisseur à payer, lu sur la facture et dont la clé est juste (un IBAN
-- à la clé fausse reste vide). Rempli après chaque lecture depuis `resultat`.
alter table document add column if not exists iban text null;
alter table document add column if not exists bic text null;

-- L12 : IBAN connus d'un fournisseur. Un IBAN n'est connu qu'après la validation humaine d'une
-- facture qui le porte (`origine = 'validation'`, premier IBAN du fournisseur) ou la confirmation
-- explicite d'un administrateur (`'confirmation'` : « C'est le bon IBAN »). Appris au groupe
-- (comme SIRET et TVA), comparé à ceux de la fiche et de son groupe. Effacé par la
-- réinitialisation avec les fournisseurs.
create table if not exists fournisseur_iban (
    id              bigint generated always as identity primary key,
    fournisseur_id  bigint not null references fournisseur on delete cascade,
    -- Majuscules, sans espaces.
    iban            text not null,
    bic             text null,
    origine         text not null check (origine in ('validation', 'confirmation')),
    -- Document qui l'a fait connaître (date de première vue : sa date, sinon `ajoute_le`).
    document_id     bigint null references document on delete set null,
    premiere_vue_le date null,
    ajoute_par      bigint null references utilisateur on delete set null,
    ajoute_le       timestamptz not null default now(),
    unique (fournisseur_id, iban)
);

-- L18 : ce que coûte chaque appel à Claude, quand la sortie JSON du CLI le donne (`usage`,
-- `total_cost_usd` ; null sinon), et le document lu. Sans clé étrangère : `appel_claude` est
-- gardé par la réinitialisation, qui vide `document`.
alter table appel_claude add column if not exists document_id bigint null;
alter table appel_claude add column if not exists tokens_entree integer null;
alter table appel_claude add column if not exists tokens_sortie integer null;
alter table appel_claude add column if not exists tokens_cache_lecture integer null;
alter table appel_claude add column if not exists tokens_cache_ecriture integer null;
alter table appel_claude add column if not exists cout_usd numeric(12, 6) null;
create index if not exists appel_claude_document on appel_claude (document_id)
    where document_id is not null;
create index if not exists appel_claude_quand on appel_claude (quand);
