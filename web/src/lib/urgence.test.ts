// Tests de la raison d'urgence de l'accueil : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { raisonUrgence } from './urgence.ts';

const J = '2026-11-27';

test('échéance : dépassée, aujourd’hui, demain, dans N jours', () => {
  assert.deepEqual(raisonUrgence({ rang: 1, date_echeance: '2026-11-24' }, J), { texte: 'Échéance dépassée depuis 3 jours', ton: 'e' });
  assert.equal(raisonUrgence({ rang: 1, date_echeance: '2026-11-26' }, J)?.texte, 'Échéance dépassée depuis 1 jour');
  assert.deepEqual(raisonUrgence({ rang: 1, date_echeance: J }, J), { texte: 'À payer aujourd’hui', ton: 'e' });
  assert.equal(raisonUrgence({ rang: 1, date_echeance: '2026-11-28' }, J)?.texte, 'À payer demain');
  // Passage au mois suivant.
  assert.equal(raisonUrgence({ rang: 1, date_echeance: '2026-12-02' }, J)?.texte, 'À payer dans 5 jours');
});

test('validable d’un clic, ou à corriger (son alerte suffit)', () => {
  assert.deepEqual(raisonUrgence({ rang: 2, date_echeance: null }, J), { texte: 'Validable d’un clic', ton: 'ok' });
  assert.equal(raisonUrgence({ rang: 3, date_echeance: '2026-12-30' }, J), null);
});
