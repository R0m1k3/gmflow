-- Remise lue sur une facture ou un avoir : total HT des remises déduites (remise globale,
-- commerciale, rabais, ristourne, escompte), en positif, et son taux quand il est imprimé.
-- Montants annexes compris dans le HT (éco-participation, DEEE, port, emballage) : absents des
-- BL, ils expliquent eux aussi l'écart. Le HT du document reste le net à comptabiliser.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table document
    add column remise_ht numeric(14, 2) null,
    add column remise_taux numeric(6, 3) null,
    add column annexes_ht numeric(14, 2) null;
