-- Éco-participations déjà comprises dans le montant des BL d'une facture : celles des BL dont
-- le HT a été calculé depuis les lignes (`confiance.montants_calcules` contient « ht »). Elles
-- ne doivent pas être retirées une seconde fois des montants annexes de la facture quand on
-- explique l'écart. Même valeur sur chaque lien de la facture, comme `ecart_montant`.
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table rapprochement add column if not exists eco_bl numeric(14, 2) null;

-- Liens existants : même calcul que `rapprochement::actualiser_ecarts`.
update rapprochement r set eco_bl = e.eco
from (
    select r2.facture_id, round(sum(
               coalesce(l.quantite, l.colis * l.pcb, 0)
               * (coalesce(l.deee_ht, 0) + coalesce(l.ecomob_ht, 0))), 2) as eco
    from rapprochement r2
    join document b on b.id = r2.bl_id
    join ligne_document l on l.document_id = b.id
    where not r2.rejete and coalesce(b.confiance->'montants_calcules' ? 'ht', false)
    group by r2.facture_id
) e
where r.facture_id = e.facture_id and not r.rejete and e.eco > 0;
