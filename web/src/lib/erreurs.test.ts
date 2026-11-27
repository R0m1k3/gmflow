// Tests de la traduction des erreurs de l'API : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { expliquer, phrase, texteErreur } from './erreurs.ts';

/** Même forme que `ErreurApi` d'api.ts (non importée : elle tire SvelteKit). */
class FausseErreur extends Error {
  code: number;
  raison?: string;
  constructor(code: number, message: string, raison?: string) {
    super(message);
    this.code = code;
    this.raison = raison;
  }
}

test('chaque code a un message simple, un « que faire » et son action', () => {
  const x = expliquer(new FausseErreur(409, 'adresse d’expédition non configurée', 'expedition'), true);
  assert.equal(x.message, 'L’adresse d’envoi vers Dext / Yooz n’est pas réglée.');
  assert.deepEqual(x.action, { genre: 'lien', libelle: 'Régler l’expédition', href: '/reglages/envoi' });
  // Un responsable ne va pas aux réglages : il prévient l'administrateur.
  const r = expliquer(new FausseErreur(409, 'x', 'expedition'), false);
  assert.equal(r.action, null);
  assert.match(r.quoiFaire, /administrateur/);
  const s = expliquer(new FausseErreur(401, 'non connecté', 'non_connecte'));
  assert.equal(s.action?.genre, 'reconnecter');
  const i = expliquer(new FausseErreur(500, 'erreur interne', 'interne'));
  assert.equal(i.message, 'Le serveur a rencontré un problème.');
  assert.equal(i.action?.genre, 'reessayer');
});

test('sans code (ancien serveur, proxy) : d’après le code HTTP', () => {
  assert.equal(expliquer(new FausseErreur(404, 'introuvable')).code, 'absent');
  assert.equal(expliquer(new FausseErreur(403, 'accès refusé')).code, 'interdit');
  // « Erreur 502 » d'un proxy : jamais montré tel quel.
  const p = expliquer(new FausseErreur(502, 'Erreur 502'));
  assert.equal(p.code, 'reseau');
  assert.equal(p.message, 'Le serveur ne répond pas.');
  // Code inconnu du front : repli sur le code HTTP.
  assert.equal(expliquer(new FausseErreur(409, 'déjà fait', 'nouveau_code')).code, 'conflit');
});

test('les refus dont la phrase du serveur est le bon message la gardent', () => {
  const d = expliquer(new FausseErreur(400, 'port invalide', 'donnees'));
  assert.equal(d.message, 'Port invalide.');
  assert.equal(d.quoiFaire, 'Corrigez la valeur, puis recommencez.');
  assert.equal(texteErreur(new FausseErreur(409, 'brouillon modifié entre-temps', 'conflit')).startsWith('Brouillon modifié entre-temps.'), true);
});

test('réseau coupé et erreurs ordinaires', () => {
  assert.equal(expliquer(new TypeError('Failed to fetch')).code, 'reseau');
  assert.equal(expliquer(new Error('Montant illisible')).message, 'Montant illisible.');
  assert.equal(expliquer('texte').message, 'Texte.');
  assert.equal(phrase('déjà fini !'), 'Déjà fini !');
});
