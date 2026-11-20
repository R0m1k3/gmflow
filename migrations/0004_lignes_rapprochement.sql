-- Lot 4 (BL et rapprochement) : lignes articles des documents, rapprochement des bons de
-- livraison et des factures.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

create table ligne_document (
    id               bigint generated always as identity primary key,
    document_id      bigint not null references document on delete cascade,
    rang             int not null,
    reference        text null,
    designation      text null,
    quantite         numeric(14, 3) null,
    unite            text null,
    prix_unitaire_ht numeric(14, 4) null,
    montant_ht       numeric(14, 2) null,
    unique (document_id, rang)
);

alter table document
    -- Numéros des bons de livraison cités par une facture.
    add column numeros_bl text[] not null default '{}',
    -- Les lignes ont été demandées à la lecture (documents lus à partir du lot 4).
    add column lignes_lues bool not null default false,
    -- « Pas de correspondance attendue » : un humain a écarté ce BL ou cette facture du
    -- rapprochement.
    add column rapprochement_ignore bool not null default false;

create table rapprochement (
    id            bigint generated always as identity primary key,
    facture_id    bigint not null references document on delete cascade,
    bl_id         bigint not null references document on delete cascade,
    -- commande : même n° de commande ; numero_bl : la facture cite le n° du BL ;
    -- facture_citee : le BL cite le n° de la facture ; manuel : lien posé par un humain.
    methode       text not null check (methode in ('commande', 'numero_bl', 'facture_citee', 'manuel')),
    -- Lien automatique défait par un humain : gardé pour ne pas être recréé.
    rejete        bool not null default false,
    -- HT de la facture moins la somme des BL rapprochés (même valeur sur chaque lien de la
    -- facture), null si un montant manque.
    ecart_montant numeric(14, 2) null,
    -- Écarts de quantités par article : [{"reference", "designation", "qte_bl", "qte_facture"}].
    ecarts        jsonb not null default '[]',
    cree_le       timestamptz not null default now(),
    auteur_id     bigint null references utilisateur on delete set null,
    unique (facture_id, bl_id)
);
create index rapprochement_bl on rapprochement (bl_id);
create index document_numero_commande on document (fournisseur_id, numero_commande);
