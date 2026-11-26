-- Export Phénix : colonnes d'identification remplies depuis le BL, par fournisseur
-- (« ref » : référence seule, « gencod » : gencod seul, null : les deux ou le réglage du groupe).
-- Règle : une migration ne fait qu'ajouter (voir CLAUDE.md).

alter table fournisseur add column if not exists colonnes_export text null
    check (colonnes_export in ('ref', 'gencod'));
