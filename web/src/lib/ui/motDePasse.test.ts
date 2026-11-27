// Contrôles du changement de mot de passe : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { phrase, verifierChangement } from './motDePasse.ts';

test('saisie complète et correcte : aucune erreur', () => {
  assert.deepEqual(verifierChangement('ancien mot de passe', 'nouveau mot de passe', 'nouveau mot de passe'), {});
});

test('chaque champ a son message', () => {
  const e = verifierChangement('', 'court', 'autre');
  assert.equal(e.ancien, 'Saisissez votre mot de passe actuel.');
  assert.equal(e.nouveau, 'Le nouveau mot de passe doit faire 12 caractères au moins.');
  assert.equal(e.confirmation, 'Les deux mots de passe sont différents.');
  assert.equal(
    verifierChangement('même mot de passe', 'même mot de passe', 'même mot de passe').nouveau,
    'Le nouveau mot de passe doit être différent de l’actuel.'
  );
  // Les caractères accentués comptent pour un.
  assert.equal(verifierChangement('x', 'éééééééééééé', 'éééééééééééé').nouveau, undefined);
});

test('phrases du serveur mises en forme', () => {
  assert.equal(phrase("le mot de passe actuel n'est pas le bon"), 'Le mot de passe actuel n’est pas le bon.');
  assert.equal(phrase('Déjà propre.'), 'Déjà propre.');
});
