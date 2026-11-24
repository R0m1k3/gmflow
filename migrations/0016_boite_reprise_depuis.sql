-- Date posée par la réinitialisation : la relève d'une boîte sans dossier relevé repart de ce
-- jour-là (mails du jour) au lieu de l'historique de `reprise_mois`.
alter table boite_mail add column if not exists reprise_depuis date;
