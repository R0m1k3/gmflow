-- Apprentissage du tri : la catégorie choisie par la règle ou Claude est gardée quand un humain
-- la corrige ou la confirme (justesse du tri, exemples donnés à Claude).
alter table mail
    add column categorie_auto     text null,
    add column categorie_auto_par text null check (categorie_auto_par in ('regle', 'claude'));

create index mail_tri_humain_expediteur on mail (lower(expediteur), trie_le desc) where categorie_par = 'humain';
create index mail_tri_humain on mail (trie_le desc) where categorie_par = 'humain';
