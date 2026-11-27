-- Recherche dans les listes (C19) : `ilike '%x%'` sur ces colonnes se sert d'un index GIN
-- trigramme au lieu de lire toute la table. pg_trgm est une extension « de confiance »
-- (PostgreSQL 13+) : le propriétaire de la base peut la créer sans être superutilisateur.
create extension if not exists pg_trgm;

create index if not exists document_fournisseur_nom_trgm on document using gin (fournisseur_nom gin_trgm_ops);
create index if not exists document_numero_trgm on document using gin (numero gin_trgm_ops);
create index if not exists document_numero_commande_trgm on document using gin (numero_commande gin_trgm_ops);
create index if not exists mail_objet_trgm on mail using gin (objet gin_trgm_ops);
create index if not exists mail_expediteur_trgm on mail using gin (expediteur gin_trgm_ops);
create index if not exists piece_jointe_nom_trgm on piece_jointe using gin (nom gin_trgm_ops);
create index if not exists fournisseur_nom_trgm on fournisseur using gin (nom gin_trgm_ops);
create index if not exists fournisseur_code_gestion_trgm on fournisseur using gin (code_gestion gin_trgm_ops);

-- Historique des corrections d'une fiche (U9) : le journal d'un document se lit par son objet.
create index if not exists journal_objet on journal (objet_type, objet_id);
