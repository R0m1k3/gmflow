-- Remise lue sur une facture ou un avoir : total HT des remises déduites (remise globale,
-- commerciale, rabais, ristourne, escompte), en positif, et son taux quand il est imprimé.
-- Le HT du document est déjà net de la remise : elle explique l'écart avec les BL.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table document
    add column remise_ht numeric(14, 2) null,
    add column remise_taux numeric(6, 3) null;
