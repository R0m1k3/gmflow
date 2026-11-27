// Tests du tri par l'adresse : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { lienTri, lireTri, parametresTri, sensLisible, type ColonneTri } from './tri.ts';

const COLONNES: ColonneTri[] = [
  { cle: 'recu', libelle: 'Reçu', croissant: false, genre: 'date' },
  { cle: 'fournisseur', libelle: 'Fournisseur', croissant: true, genre: 'texte' },
  { cle: 'montant', libelle: 'Montant', croissant: false, genre: 'nombre' }
];
const u = (q: string) => new URL(`http://x/factures${q}`);

test('tri par défaut : rien dans l’adresse ni vers le serveur', () => {
  const t = lireTri(new URLSearchParams(''), COLONNES);
  assert.deepEqual(t, { cle: 'recu', croissant: false, defaut: true });
  assert.deepEqual(parametresTri(t), {});
  // Clé inconnue (vieux lien) : tri par défaut, jamais envoyée telle quelle.
  assert.equal(lireTri(new URLSearchParams('tri=pirate'), COLONNES).cle, 'recu');
});

test('un clic trie dans le sens habituel, le second inverse', () => {
  assert.equal(lienTri(u('?filtre=a-valider'), COLONNES, 'montant'), '/factures?filtre=a-valider&tri=montant&sens=desc');
  assert.equal(lienTri(u('?tri=montant&sens=desc'), COLONNES, 'montant'), '/factures?tri=montant&sens=asc');
  assert.equal(lienTri(u(''), COLONNES, 'fournisseur'), '/factures?tri=fournisseur&sens=asc');
  // Retour au tri par défaut : l'adresse redevient propre.
  assert.equal(lienTri(u('?tri=recu&sens=asc'), COLONNES, 'recu'), '/factures');
  // La fiche ouverte est fermée : la liste repart du début.
  assert.equal(lienTri(u('?doc=12&filtre=tous'), COLONNES, 'montant', ['doc']), '/factures?filtre=tous&tri=montant&sens=desc');
});

test('tri lu pour le serveur', () => {
  const t = lireTri(new URLSearchParams('tri=montant&sens=asc'), COLONNES);
  assert.deepEqual(parametresTri(t), { tri: 'montant', sens: 'asc' });
  assert.equal(sensLisible('nombre', true), 'du plus petit au plus grand');
  assert.equal(sensLisible('date', false), 'du plus récent au plus ancien');
});
