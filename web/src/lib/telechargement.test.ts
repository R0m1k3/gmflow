// Tests du nom de fichier téléchargé : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nomFichier } from './telechargement.ts';

test('le nom UTF-8 du serveur passe avant le nom ASCII', () => {
  const d = `attachment; filename="Bilan_2026-11_Soci_t_.xlsx"; filename*=UTF-8''Bilan%202026-11%20Soci%C3%A9t%C3%A9.xlsx`;
  assert.equal(nomFichier(d, 'x.xlsx'), 'Bilan 2026-11 Société.xlsx');
  assert.equal(nomFichier('attachment; filename="a.xlsx"', 'x.xlsx'), 'a.xlsx');
  assert.equal(nomFichier(`attachment; filename="a.xlsx"; filename*=UTF-8''%E9`, 'x.xlsx'), 'a.xlsx');
  assert.equal(nomFichier(null, 'x.xlsx'), 'x.xlsx');
  assert.equal(nomFichier('attachment', 'x.xlsx'), 'x.xlsx');
});
