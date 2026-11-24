-- Export des BL vers le logiciel du magasin (matrice de commande et trame de fiches produits
-- BO Phénix) : détail des lignes lu sur le BL, et articles retrouvés sur le serveur du magasin.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table ligne_document
    add column gencod       text null,
    -- Colis (cartons) livrés et unités par colis : quantité en unités = colis × pcb.
    add column colis        numeric(14, 3) null,
    add column pcb          numeric(14, 3) null,
    -- Remise de la ligne en %.
    add column remise_taux  numeric(8, 4) null,
    add column taux_tva     numeric(6, 3) null,
    -- Éco-participations HT par unité : DEEE (électrique) et Ecomob (mobilier).
    add column deee_ht      numeric(14, 4) null,
    add column ecomob_ht    numeric(14, 4) null,
    add column pv_conseille numeric(14, 4) null,
    -- Article retrouvé sur le serveur du magasin (Apiflow), avec ses composants si c'est un lot :
    -- {"par": "gencod"|"reference", "no_id", "codein", "libelle", "gencod", "composants": [...]}.
    -- null : pas trouvé ou pas encore cherché (voir document.magasin_verifie_le).
    add column magasin      jsonb null;

alter table document
    -- Recherche des articles du BL sur le serveur du magasin : demandée le (en cours tant que
    -- magasin_verifie_le est plus ancien), faite le, et son résultat (null si réussie, sinon
    -- le message d'erreur).
    add column magasin_demande_le timestamptz null,
    add column magasin_verifie_le timestamptz null,
    add column magasin_erreur     text null;
