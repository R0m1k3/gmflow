// Tests des modifications non enregistrées : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { FormulairesTouches, quitte } from './quitter.ts';

const u = (s: string) => new URL(`http://x${s}`);

test('quitter la fiche : autre page, autre pièce, fermeture', () => {
  assert.equal(quitte(u('/factures?doc=1'), u('/factures?doc=1&filtre=tous'), ['doc']), false);
  assert.equal(quitte(u('/factures?doc=1'), u('/factures?doc=2'), ['doc']), true);
  assert.equal(quitte(u('/factures?doc=1'), u('/factures'), ['doc']), true);
  assert.equal(quitte(u('/factures?doc=1'), u('/bl?doc=1'), ['doc']), true);
  assert.equal(quitte(u('/reglages/envoi'), u('/reglages/envoi#x')), false);
  assert.equal(quitte(u('/reglages/envoi'), null), true);
});

test('formulaires touchés : envoyés ou retirés de la page, ils sont oubliés', () => {
  const t = new FormulairesTouches<{ isConnected: boolean }>();
  const a = { isConnected: true };
  const b = { isConnected: true };
  assert.equal(t.modifie, false);
  t.toucher(a);
  t.toucher(b);
  assert.equal(t.modifie, true);
  t.oublier(a);
  assert.equal(t.modifie, true);
  // « Annuler » ferme le formulaire : il n'est plus dans la page.
  b.isConnected = false;
  assert.equal(t.modifie, false);
});
