// Tests de la phrase d'état : `npm test` (node --test).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ORDRE, phraseCourte, phraseEcheance, rangerPhrases, type CodePhrase, type Phrase, type TonPhrase } from './phraseEtat.ts';

const p = (code: CodePhrase, ton: TonPhrase = 'attention', id?: string): Phrase => ({ code, ton, titre: `${code}.`, texte: '', id });
const codes = (l: Phrase[]) => l.map((x) => x.code);

test('aucune phrase : rien en tête', () => {
  assert.deepEqual(rangerPhrases([]), { principale: null, aussi: [] });
});

test('la plus grave en tête, le reste dans « Aussi », rien de perdu', () => {
  const r = rangerPhrases([p('articles'), p('lecture_incomplete'), p('iban', 'erreur'), p('controle', 'attention', 'a'), p('controle', 'attention', 'b')]);
  assert.equal(r.principale?.code, 'iban');
  assert.deepEqual(codes(r.aussi), ['lecture_incomplete', 'controle', 'controle', 'articles']);
  assert.deepEqual(r.aussi.filter((x) => x.code === 'controle').map((x) => x.id), ['a', 'b']);
});

test('toute combinaison : principale + aussi = phrases reçues (hors « tout va bien »)', () => {
  const toutes = ORDRE.map((c) => p(c, c === 'prete' ? 'ok' : 'attention'));
  for (let i = 0; i < toutes.length; i++) {
    for (let j = i; j < toutes.length; j++) {
      const recues = i === j ? [toutes[i]] : [toutes[j], toutes[i]];
      const r = rangerPhrases(recues);
      const rendues = [r.principale, ...r.aussi].filter(Boolean) as Phrase[];
      const attendues = recues.filter((x) => x.code !== 'prete' || recues.length === 1);
      assert.deepEqual(new Set(codes(rendues)), new Set(codes(attendues)));
      assert.equal(rendues.length, attendues.length);
    }
  }
});

test('« tout va bien » seulement sans alerte', () => {
  assert.equal(rangerPhrases([p('prete', 'ok')]).principale?.code, 'prete');
  const avecAlerte = rangerPhrases([p('prete', 'ok'), p('echeance')]);
  assert.equal(avecAlerte.principale?.code, 'echeance');
  assert.deepEqual(avecAlerte.aussi, []);
  // une information (pas de BL rattaché) ne retire pas le « tout va bien », elle le suit
  const avecInfo = rangerPhrases([p('sans_bl_info', 'info'), p('prete', 'ok')]);
  assert.equal(avecInfo.principale?.code, 'prete');
  assert.deepEqual(codes(avecInfo.aussi), ['sans_bl_info']);
});

test('l’échéance passe après les écarts et les défauts de lecture', () => {
  const r = rangerPhrases([p('echeance'), p('sans_bl'), p('articles'), p('montants_non_lus')]);
  assert.deepEqual(codes([r.principale!, ...r.aussi]), ['montants_non_lus', 'articles', 'sans_bl', 'echeance']);
});

test('pièce hors contrôle : son état passe devant ses alertes', () => {
  const r = rangerPhrases([p('articles'), p('envoyee', 'ok')]);
  assert.equal(r.principale?.code, 'envoyee');
  assert.deepEqual(codes(r.aussi), ['articles']);
  assert.equal(rangerPhrases([p('lecture_incomplete'), p('validee', 'info')]).principale?.code, 'validee');
});

test('phrase courte de la liste « Aussi »', () => {
  assert.equal(phraseCourte({ code: 'articles', ton: 'attention', titre: '2 articles en écart avec le BL.', texte: 'PL130 : prix.' }), '2 articles en écart avec le BL. PL130 : prix.');
  assert.equal(phraseCourte({ code: 'lecture_incomplete', ton: 'attention', titre: 'Lecture incomplète.', texte: 'Pages 4 à 6 non lues.' }), 'lecture incomplète. Pages 4 à 6 non lues.');
  assert.equal(phraseCourte({ code: 'iban', ton: 'erreur', titre: 'IBAN changé :', texte: 'autre compte.' }), 'IBAN changé : autre compte.');
  assert.equal(phraseCourte({ code: 'echeance', ton: 'attention', titre: 'À payer demain (28/11).', texte: '' }), 'à payer demain (28/11).');
});

test('échéance : 7 jours au plus', () => {
  const J = '2026-11-27';
  assert.equal(phraseEcheance('2026-11-30', J), 'À payer dans 3 jours (30/11).');
  assert.equal(phraseEcheance('2026-11-28', J), 'À payer demain (28/11).');
  assert.equal(phraseEcheance(J, J), 'À payer aujourd’hui (27/11).');
  assert.equal(phraseEcheance('2026-11-25', J), 'Échéance dépassée depuis 2 jours (25/11).');
  assert.equal(phraseEcheance('2026-12-04', J), 'À payer dans 7 jours (04/12).');
  assert.equal(phraseEcheance('2026-12-05', J), null);
  assert.equal(phraseEcheance(null, J), null);
  assert.equal(phraseEcheance('n’importe quoi', J), null);
});
