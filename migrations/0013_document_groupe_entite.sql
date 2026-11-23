-- Groupe et entité du fournisseur lus sur un document : l'enseigne ou la maison mère vue en
-- en-tête ou en logo (« U10 »), et la société, marque ou département qui a réellement vendu ou
-- livré (« Dép. : L3C »). Ils servent à ranger le document chez la marque du groupe.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table document
    add column fournisseur_groupe text null,
    add column fournisseur_entite text null;
