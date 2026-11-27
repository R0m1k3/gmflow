// Tests des outils des notes internes et de l'écartement par lot : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { apercuNote, bulleNotes, corpsEcart, resumeEcartLot } from './NotesOutils.ts';

test('aperçu : une ligne, coupé avec des points de suspension', () => {
  assert.equal(apercuNote('  Appeler\n  Durand  '), 'Appeler Durand');
  const long = 'a'.repeat(100);
  const a = apercuNote(long, 20);
  assert.equal(a.length, 20);
  assert.ok(a.endsWith('…'));
  assert.equal(apercuNote(null), '');
});

test('bulle de la pastille : nombre et début de la dernière note', () => {
  assert.equal(bulleNotes(0, 'x'), '');
  assert.equal(bulleNotes(undefined, null), '');
  assert.equal(bulleNotes(1, 'Remise oubliée'), '1 note interne. Dernière : « Remise oubliée »');
  assert.equal(bulleNotes(3, null), '3 notes internes.');
});

test('écartement : motif obligatoire, précision facultative', () => {
  assert.equal(corpsEcart(null, 'texte'), null);
  assert.deepEqual(corpsEcart('publicite', '  '), { motif: 'publicite', texte: null });
  assert.deepEqual(corpsEcart('autre', ' Mauvais magasin '), { motif: 'autre', texte: 'Mauvais magasin' });
});

test('résumé de l’écartement par lot', () => {
  assert.equal(resumeEcartLot({ ecartees: 1, laissees: 0, pieces: [] }), '1 écartée.');
  assert.equal(
    resumeEcartLot({
      ecartees: 2,
      laissees: 2,
      pieces: [
        { id: 1, ecartee: false, raison: 'Document déjà envoyé ou en cours d’envoi' },
        { id: 2, ecartee: false, raison: 'Document déjà envoyé ou en cours d’envoi' }
      ]
    }),
    '2 écartées, 2 laissées (document déjà envoyé ou en cours d’envoi).'
  );
});
