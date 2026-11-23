-- Autres noms sous lesquels les fournisseurs écrivent à une société (nom complet, enseigne et
-- ville...) : donnés à Claude et comparés à l'acheteur des factures électroniques.
alter table societe add column autres_noms text[] not null default '{}';
