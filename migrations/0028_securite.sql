-- Sécurité (constat C10) : changer de mot de passe coupe les autres sessions de l'utilisateur.
-- Chaque session garde la génération qu'elle a connue à la connexion ; un changement de mot
-- de passe l'augmente, les sessions plus anciennes ne valent plus.
alter table utilisateur add column if not exists generation_session int not null default 0;
