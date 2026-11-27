// Tests des outils de la page Bilan du mois : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { lienExportBilan } from './BilanOutils.ts';

test('lien d’export : mois et société choisie, sans société pour « toutes »', () => {
  assert.equal(lienExportBilan('2026-11', null), '/api/bilan/export?mois=2026-11');
  assert.equal(lienExportBilan('2026-11', 4), '/api/bilan/export?mois=2026-11&societe=4');
});
