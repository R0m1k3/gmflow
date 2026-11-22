-- Pièces PDF enregistrées sous un type générique (application/octet-stream, application/x-pdf...) :
-- l'interface n'en montrait pas l'aperçu. La relève corrige désormais le type d'après le contenu ;
-- ici, rattrapage des pièces déjà en base : nom en .pdf, type contenant « pdf », ou pièce lue
-- comme un PDF. Le serveur vérifie encore le contenu avant tout aperçu.
update piece_jointe p
   set type_mime = 'application/pdf'
 where p.type_mime <> 'application/pdf'
   and (lower(p.nom) like '%.pdf'
        or lower(p.type_mime) like '%pdf%'
        or exists (select 1 from document d
                    where d.piece_jointe_id = p.id
                      and d.voie in ('facturx', 'pdf_texte')));
