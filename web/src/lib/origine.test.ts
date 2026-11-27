// Tests de l'origine des valeurs de la fiche : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { origineChamp, quandCorrection, type SourceFiche } from './origine.ts';

const base: SourceFiche = { corrections: {}, historique: [], confiance: {}, voie: 'pdf_texte' };
const rien = { vide: false, douteuse: false, modifiee: false };

test('lue avec certitude, avec doute, ou vide', () => {
  assert.equal(origineChamp('numero', base, rien)?.genre, 'sur');
  assert.deepEqual(origineChamp('numero', base, { ...rien, douteuse: true })?.libelle, 'Lue avec doute : à vérifier sur la pièce');
  assert.equal(origineChamp('numero', base, { ...rien, vide: true }), null);
  // Doute relu par un humain à la validation.
  const v = { ...base, verification: { par: 'marie', quand: '2026-11-12T13:05:00Z' } };
  assert.equal(origineChamp('numero', v, { ...rien, douteuse: true })?.libelle, 'Lue avec doute, vérifiée par marie le 12/11/2026 à 14:05');
  // Facture électronique : jamais de doute.
  assert.equal(origineChamp('numero', { ...base, voie: 'facturx' }, { ...rien, douteuse: true })?.genre, 'sur');
});

test('corrigée : par qui et quand, historique du plus récent au plus ancien', () => {
  const d: SourceFiche = {
    ...base,
    corrections: { numero: { avant: 'F1', apres: 'F-2' } },
    historique: [
      { champ: 'numero', avant: 'F1', apres: 'F-1', par: 'marie', quand: '2026-11-12T13:05:00Z' },
      { champ: 'montant_ttc', avant: 1, apres: 2, par: 'paul', quand: '2026-11-12T13:06:00Z' },
      { champ: 'numero', avant: 'F-1', apres: 'F-2', par: 'paul', quand: '2026-11-13T08:30:00Z' }
    ]
  };
  const o = origineChamp('numero', d, rien)!;
  assert.equal(o.genre, 'corrige');
  assert.equal(o.libelle, 'Corrigée par paul le 13/11/2026 à 09:30');
  assert.deepEqual(o.historique.map((c) => c.apres), ['F-2', 'F-1']);
  // Un champ revenu à sa valeur lue n'est plus « corrigé », son historique reste.
  const t = origineChamp('montant_ttc', d, rien)!;
  assert.equal(t.genre, 'sur');
  assert.equal(t.historique.length, 1);
  // Corrigée puis effacée : la pastille reste (on sait qui l'a vidée).
  assert.equal(origineChamp('numero', d, { ...rien, vide: true })?.genre, 'corrige');
});

test('calculée et modifiée', () => {
  const d: SourceFiche = { ...base, confiance: { montants_calcules: ['ht', 'ttc'] } };
  assert.equal(origineChamp('montant_ht', d, rien)?.genre, 'calcule');
  assert.equal(origineChamp('montant_tva', d, rien)?.genre, 'sur');
  assert.equal(origineChamp('montant_ht', d, { ...rien, modifiee: true })?.genre, 'modifie');
  assert.equal(quandCorrection('2026-07-01T22:15:00Z'), '02/07/2026 à 00:15');
});
