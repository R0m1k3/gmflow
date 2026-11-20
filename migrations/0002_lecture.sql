-- Lot 2 (Lecture) : un document par pièce jointe, lu par le code ou par Claude.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

create table document (
    id                bigint generated always as identity primary key,
    piece_jointe_id   bigint not null unique references piece_jointe on delete cascade,
    mail_id           bigint not null references mail on delete cascade,
    sha256            bytea not null,
    etat              text not null default 'a_lire'
                      check (etat in ('a_lire', 'en_cours', 'lu', 'ignore', 'erreur')),
    -- Voie de lecture : facturx, xml, pdf_texte, tableur, scan (null tant que non aiguillé).
    voie              text null,
    message           text null,
    type              text null check (type in ('facture', 'avoir', 'bl', 'relance', 'devis', 'autre')),
    societe_id        bigint null references societe on delete set null,
    fournisseur_nom   text null,
    fournisseur_siret text null,
    fournisseur_tva   text null,
    numero            text null,
    date_document     date null,
    numero_commande   text null,
    montant_ht        numeric(14, 2) null,
    montant_tva       numeric(14, 2) null,
    montant_ttc       numeric(14, 2) null,
    -- Confiance par champ (0 à 1) : {"type": .., "societe": .., "fournisseur": .., ...}.
    confiance         jsonb not null default '{}',
    -- Texte lu (couche texte, cellules ou transcription d'un scan) : jamais relu.
    texte             text null,
    -- Réponse brute de Claude, ou données structurées lues dans le XML.
    resultat          jsonb null,
    modele            text null,
    effort            text null,
    -- Le document reprend le résultat d'un autre document au même contenu.
    copie_de          bigint null references document on delete set null,
    planifie_le       timestamptz null,
    pris_le           timestamptz null,
    lu_le             timestamptz null,
    -- Vérification par un humain (critère du lot : type et société justes).
    verifie_le        timestamptz null,
    verifie_par       bigint null references utilisateur on delete set null,
    type_corrige      bool not null default false,
    societe_corrigee  bool not null default false,
    cree_le           timestamptz not null default now()
);
create index document_etat on document (etat);
create index document_mail on document (mail_id);
create index document_sha on document (sha256);
create index document_societe on document (societe_id);
