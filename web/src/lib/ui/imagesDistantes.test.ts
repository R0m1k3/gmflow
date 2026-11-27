// Images distantes des mails : `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  CLE_EXPEDITEURS,
  adresseCorps,
  adresseDe,
  contientImagesDistantes,
  memoriser,
  toujoursAfficher
} from './imagesDistantes.ts';

class Memoire {
  valeurs = new Map<string, string>();
  getItem(k: string) {
    return this.valeurs.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.valeurs.set(k, v);
  }
}

test('adresse de l’expéditeur, en minuscules', () => {
  assert.equal(adresseDe('Jean Dupont <Jean.Dupont@Fournisseur.FR>'), 'jean.dupont@fournisseur.fr');
  assert.equal(adresseDe(' compta@x.fr '), 'compta@x.fr');
});

test('« toujours afficher » mémorisé par expéditeur', () => {
  const m = new Memoire();
  assert.equal(toujoursAfficher('a@x.fr', m), false);
  memoriser('Fournisseur <A@x.fr>', m);
  memoriser('a@x.fr', m);
  assert.equal(toujoursAfficher('a@x.fr', m), true);
  assert.equal(toujoursAfficher('b@x.fr', m), false);
  assert.deepEqual(JSON.parse(m.getItem(CLE_EXPEDITEURS)!), ['a@x.fr']);
  // Valeur abîmée : rien n'est autorisé, sans erreur.
  m.setItem(CLE_EXPEDITEURS, '{pas du json');
  assert.equal(toujoursAfficher('a@x.fr', m), false);
  assert.equal(toujoursAfficher('a@x.fr', null), false);
});

test('images distantes détectées, images intégrées ignorées', () => {
  assert.equal(contientImagesDistantes('<img src="https://suivi.exemple.com/p.gif">'), true);
  assert.equal(contientImagesDistantes("<td background='http://x.fr/fond.jpg'>"), true);
  assert.equal(contientImagesDistantes('<img src="//cdn.x.fr/logo.png">'), true);
  assert.equal(contientImagesDistantes('<div style="background:url( https://x.fr/a.png)">'), true);
  assert.equal(contientImagesDistantes('<img src="data:image/png;base64,AAAA">'), false);
  assert.equal(contientImagesDistantes('<a href="https://x.fr">lien</a>'), false);
  // Tel que `outerHTML` écrit un style entre guillemets doubles.
  assert.equal(
    contientImagesDistantes('<td style="background-image:url(&quot;https://x.fr/f.png&quot;)">'),
    true
  );
  assert.equal(contientImagesDistantes('<svg><image xlink:href="https://x.fr/p.svg"/></svg>'), true);
  assert.equal(contientImagesDistantes('<link rel="stylesheet" href="https://x.fr/s.css">'), false);
});

test('adresse du corps HTML', () => {
  assert.equal(adresseCorps(7, false), '/api/mails/7/html');
  assert.equal(adresseCorps(7, true), '/api/mails/7/html?images=1');
});
