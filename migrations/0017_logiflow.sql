-- Logiflow (API externe /api/ext/v1) : après l'envoi d'une facture à Dext/Yooz, le worker
-- retrouve la livraison de chaque BL dans Logiflow et y écrit la référence et les montants
-- de la facture. Dernier résultat par facture, affiché sur sa fiche.
create table if not exists logiflow_rapprochement (
    document_id bigint primary key references document on delete cascade,
    quand       timestamptz not null default now(),
    -- Vrai si au moins une livraison a été complétée et aucune n'a échoué.
    ok          bool not null,
    message     text not null,
    -- [{"bl", "livraison_id", "magasin", "etat", "message"}] ; etat : complete, deja_fait,
    -- deja_rapproche, autre_facture, introuvable, ambigu, erreur.
    livraisons  jsonb not null default '[]'
);
