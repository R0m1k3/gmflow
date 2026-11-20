-- Lot 5 : tri des mails, file « à répondre », brouillons de réponse et envoi depuis la boîte
-- d'origine. Migration qui ne fait qu'ajouter.

alter table mail
    add column categorie       text null check (categorie in ('question', 'information', 'document_seul', 'publicite', 'notification')),
    add column categorie_par   text null check (categorie_par in ('regle', 'claude', 'humain')),
    add column resume          text null,
    add column priorite        text null check (priorite in ('haute', 'normale', 'basse')),
    add column statut_reponse  text null check (statut_reponse in ('a_repondre', 'repondu', 'sans_suite')),
    -- null : hors période de tri (mail ancien), jamais trié.
    add column tri_etat        text null check (tri_etat in ('a_trier', 'en_cours', 'trie', 'erreur')),
    add column tri_message     text null,
    add column tri_planifie_le timestamptz null,
    add column tri_pris_le     timestamptz null,
    add column trie_le         timestamptz null;

create index mail_tri_etat on mail (tri_etat) where tri_etat is not null;
create index mail_statut_reponse on mail (statut_reponse) where statut_reponse is not null;
create index mail_categorie on mail (categorie);

-- Expéditeurs (adresse) ou domaines toujours classés en publicité, sans appel à Claude.
create table expediteur_marque (
    id        bigint generated always as identity primary key,
    motif     text not null default 'publicite' check (motif in ('publicite')),
    adresse   text null,
    domaine   text null,
    cree_par  bigint null references utilisateur on delete set null,
    cree_le   timestamptz not null default now(),
    check ((adresse is null) <> (domaine is null))
);
create unique index expediteur_marque_adresse on expediteur_marque (lower(adresse)) where adresse is not null;
create unique index expediteur_marque_domaine on expediteur_marque (lower(domaine)) where domaine is not null;

-- Brouillon de réponse : généré à la demande, relu, puis envoyé sur un clic.
-- generation -> propose -> (modifie) -> envoi -> envoye ; abandonne à tout moment avant l'envoi.
create table brouillon_reponse (
    id                bigint generated always as identity primary key,
    mail_id           bigint not null references mail on delete cascade,
    boite_id          bigint not null references boite_mail on delete cascade,
    texte             text not null default '',
    statut            text not null check (statut in ('generation', 'propose', 'modifie', 'envoi', 'envoye', 'abandonne')),
    modele            text null,
    effort            text null,
    erreur            text null,
    demande_par       bigint null references utilisateur on delete set null,
    cree_le           timestamptz not null default now(),
    modifie_le        timestamptz not null default now(),
    envoye_le         timestamptz null,
    envoye_par        bigint null references utilisateur on delete set null,
    message_id_envoye text null
);
create index brouillon_reponse_mail on brouillon_reponse (mail_id);
create index brouillon_reponse_boite_envoye on brouillon_reponse (boite_id, envoye_le) where statut = 'envoye';
-- Un seul brouillon en cours par mail.
create unique index brouillon_reponse_actif on brouillon_reponse (mail_id)
    where statut in ('generation', 'propose', 'modifie', 'envoi');

-- Envoi des réponses depuis la boîte d'origine (mêmes identifiant et mot de passe qu'en IMAP).
alter table boite_mail
    add column smtp_serveur    text null,
    add column smtp_port       int null check (smtp_port between 1 and 65535),
    -- 'clair' : serveur de test seulement (refusé par l'API hors build de test).
    add column smtp_securite   text null check (smtp_securite in ('tls', 'starttls', 'clair')),
    add column dossier_envoyes text null;
