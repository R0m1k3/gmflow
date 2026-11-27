-- Notes internes sur une pièce (audit du 2026-11-26 : U23).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

-- Plusieurs notes par pièce (facture, avoir, BL), horodatées et signées. Jamais envoyées à
-- l'extérieur : ni à Dext/Yooz, ni dans les réponses aux fournisseurs (aucun code d'envoi ne lit
-- cette table). Ajoutées par tout utilisateur qui voit la pièce, supprimées par leur auteur ou
-- un administrateur ; ajout et suppression sont journalisés (`note_ajoutee`, `note_supprimee`).
-- `auteur_nom` garde l'identifiant à l'écriture (la note reste signée si le compte disparaît).
-- Effacées avec les documents par la réinitialisation (`reinitialisation::TABLES_EFFACEES`).
create table if not exists note_interne (
    id          bigint generated always as identity primary key,
    document_id bigint not null references document on delete cascade,
    auteur_id   bigint null references utilisateur on delete set null,
    auteur_nom  text not null,
    texte       text not null check (length(texte) between 1 and 2000),
    cree_le     timestamptz not null default now()
);
create index if not exists note_interne_document on note_interne (document_id, cree_le desc);
