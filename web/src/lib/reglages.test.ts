// Tests des sections des réglages : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GROUPES, SECTIONS, adresseAncre, estSection, sectionDe } from './reglages.ts';

test('groupes Connexions et Maintenance', () => {
  const g = Object.fromEntries(GROUPES.map((x) => [x.titre, x.sections]));
  assert.deepEqual(g['Connexions'], ['boites', 'claude', 'envoi', 'serveur-api', 'logiflow']);
  assert.deepEqual(g['Maintenance'], ['traitements', 'avance', 'journal', 'zone-dangereuse']);
  assert.equal(new Set(SECTIONS).size, SECTIONS.length);
});

test('anciennes ancres vers les vues', () => {
  assert.equal(adresseAncre('#boites'), '/reglages/boites');
  assert.equal(adresseAncre('#serveur-api'), '/reglages/serveur-api');
  assert.equal(adresseAncre('#mise-en-route'), '/reglages');
  assert.equal(adresseAncre('#historique'), '/reglages/journal');
  assert.equal(adresseAncre(''), null);
  assert.equal(adresseAncre('#inconnue'), null);
});

test('section de l’adresse', () => {
  assert.equal(sectionDe('/reglages/envoi'), 'envoi');
  assert.equal(sectionDe('/reglages'), null);
  assert.equal(sectionDe('/reglages/boites/nouvelle'), null);
  assert.equal(sectionDe('/reglages/nimporte'), null);
  assert.equal(estSection('compte'), false);
});
