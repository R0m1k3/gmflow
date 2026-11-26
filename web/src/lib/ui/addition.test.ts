// Tests de l'addition de la chaîne : `npm test` (node --test, TypeScript lu par Node ≥ 22.18).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { EcartLigne, LigneArticle } from '#lib/types.ts';
import { additionRapprochement, ecartDeLigne, normaliser, type Addition, type FactureAddition } from './addition.ts';

const FACTURE: FactureAddition = { ht: 1825.39, remise_ht: 56.01, remise_taux: 3, annexes_ht: 14.4 };
const PRIX: EcartLigne = {
  genre: 'prix',
  reference: 'PL130',
  designation: 'Plaid sherpa 130x160 gris',
  qte_bl: 24,
  qte_facture: 24,
  prix_facture: 6.45,
  prix_bl: 6.2,
  montant_ht: 6
};

const montant = (a: Addition, genre: string) => a.lignes.find((l) => l.genre === genre)?.montant ?? null;
const proche = (x: number, y: number, tol = 0.01) => assert.ok(Math.abs(x - y) <= tol + 1e-9, `${x} ≠ ${y}`);

/** h. Somme des lignes signées jusqu'à Attendu (sans « Livré ») = Attendu ; Facturé − Attendu = Écart. */
function controler(a: Addition) {
  const attendu = montant(a, 'attendu');
  assert.notEqual(attendu, null);
  const i = a.lignes.findIndex((l) => l.genre === 'attendu');
  const somme = a.lignes.slice(0, i).filter((l) => l.genre !== 'somme').reduce((t, l) => t + (l.montant ?? 0), 0);
  proche(somme, attendu!);
  const ecart = montant(a, 'ecart');
  const facture = montant(a, 'facture');
  if (ecart !== null && facture !== null) proche(facture - attendu!, ecart);
}

const bls = (...ht: number[]) => ht.map((h, i) => ({ id: i + 1, numero: `77${31 + i}`, ht: h }));

test('a. remise et éco-participation : lecture « avant remise », reste 6,00', () => {
  const avec = additionRapprochement(FACTURE, bls(1248.6, 612.4), -35.61, [PRIX]);
  assert.equal(avec.reste, 6);
  assert.equal(avec.blsRemises, false);
  assert.equal(avec.verdict, 'article');
  assert.equal(montant(avec, 'somme'), 1861);
  assert.equal(montant(avec, 'remise'), -56.01);
  assert.equal(montant(avec, 'annexes'), 14.4);
  assert.equal(montant(avec, 'attendu'), 1819.39);
  assert.equal(montant(avec, 'autres'), null);
  assert.deepEqual(
    avec.lignes.map((l) => l.genre),
    ['bl', 'bl', 'somme', 'remise', 'annexes', 'attendu', 'facture', 'ecart']
  );
  assert.match(avec.lignes.find((l) => l.genre === 'remise')!.libelle, /3\u00a0%/);
  controler(avec);
  const sans = additionRapprochement(FACTURE, bls(1248.6, 612.4), -35.61, []);
  assert.equal(sans.verdict, 'reste');
  controler(sans);
});

test('b. BL déjà remisés : concorde, ni Remise ni Autres', () => {
  const a = additionRapprochement(FACTURE, bls(1210, 600.99), 14.4, []);
  assert.equal(a.blsRemises, true);
  assert.equal(a.verdict, 'concorde');
  assert.equal(a.reste, 0);
  assert.equal(montant(a, 'remise'), null);
  assert.equal(montant(a, 'autres'), null);
  assert.equal(a.lignes.find((l) => l.genre === 'attendu')?.note, 'BL déjà remisés');
  controler(a);
});

test('c. sans remise ni annexes, écart 0,03 : concorde', () => {
  const a = additionRapprochement({ ht: 100.03, remise_ht: null, remise_taux: null, annexes_ht: null }, bls(60, 40), 0.03, []);
  assert.equal(a.verdict, 'concorde');
  assert.equal(a.reste, 0.03);
  controler(a);
});

test('d. écart inconnu : pas de comparaison, pas de ligne Écart', () => {
  const a = additionRapprochement({ ht: 100, remise_ht: null, remise_taux: null, annexes_ht: null }, [{ id: 1, numero: 'X', ht: null }], null, []);
  assert.equal(a.verdict, 'sans');
  assert.equal(a.reste, null);
  assert.equal(montant(a, 'ecart'), null);
  assert.ok(!a.lignes.some((l) => l.genre === 'ecart'));
  controler(a);
});

test('e. un seul BL visible : « Autres montants retenus » pour le reste', () => {
  const a = additionRapprochement(FACTURE, bls(1248.6), -35.61, [PRIX]);
  assert.equal(montant(a, 'autres'), 612.4);
  assert.equal(montant(a, 'somme'), null);
  assert.equal(a.reste, 6);
  controler(a);
});

test('f. normaliser', () => {
  assert.equal(normaliser('cmd-077 / a'), 'CMD077A');
  assert.equal(normaliser('Réf. é-1'), 'RÉFÉ1');
});

test('g. ecartDeLigne : référence normalisée, sinon désignation', () => {
  const ligne = (reference: string | null, designation: string | null): LigneArticle => ({
    rang: 1,
    reference,
    designation,
    quantite: 1,
    unite: null,
    prix_unitaire_ht: 1,
    montant_ht: 1
  });
  const parRef: EcartLigne = { genre: 'prix', reference: '033484', designation: 'Lot bougies', qte_bl: 1, qte_facture: 1 };
  const parNom: EcartLigne = { genre: 'en_trop', reference: 'AUTRE', designation: 'Coussin velours 45x45', qte_bl: 2, qte_facture: 0 };
  assert.equal(ecartDeLigne(ligne('03/3484', 'autre chose'), [parNom, parRef]), parRef);
  assert.equal(ecartDeLigne(ligne('ZZ9', 'coussin velours 45 x 45'), [parRef, parNom]), parNom);
  assert.equal(ecartDeLigne(ligne(null, 'coussin-velours-45x45'), [parNom]), parNom);
  assert.equal(ecartDeLigne(ligne('ZZ9', 'Inconnu'), [parRef, parNom]), null);
  assert.equal(ecartDeLigne(ligne(null, null), [parRef, parNom]), null);
});

test('i. écarts qui se compensent : concorde, mais les deux articles sont gardés', () => {
  const enTrop: EcartLigne = { genre: 'en_trop', reference: 'A', designation: 'A', qte_bl: 1, qte_facture: 0, montant_ht: 10 };
  const prix: EcartLigne = { genre: 'prix', reference: 'B', designation: 'B', qte_bl: 1, qte_facture: 1, montant_ht: -10 };
  const a = additionRapprochement({ ht: 100, remise_ht: null, remise_taux: null, annexes_ht: null }, bls(100), 0, [enTrop, prix]);
  assert.equal(a.verdict, 'concorde');
  assert.deepEqual(a.causes, [enTrop, prix]);
  controler(a);
});
