// Changement de mot de passe (« Mon mot de passe ») : contrôles avant l'envoi, mêmes règles
// que le serveur (12 caractères au moins, différent de l'actuel). Fonctions pures, `npm test`.

export const LONGUEUR_MIN = 12;

export type ErreursMdp = { ancien?: string; nouveau?: string; confirmation?: string };

export function verifierChangement(ancien: string, nouveau: string, confirmation: string): ErreursMdp {
  const e: ErreursMdp = {};
  if (ancien === '') e.ancien = 'Saisissez votre mot de passe actuel.';
  if ([...nouveau].length < LONGUEUR_MIN) {
    e.nouveau = `Le nouveau mot de passe doit faire ${LONGUEUR_MIN} caractères au moins.`;
  } else if (nouveau === ancien) {
    e.nouveau = 'Le nouveau mot de passe doit être différent de l’actuel.';
  }
  if (confirmation !== nouveau) e.confirmation = 'Les deux mots de passe sont différents.';
  return e;
}

/** Phrase du serveur mise en forme : majuscule au début, point à la fin. */
export function phrase(m: string): string {
  const t = m.trim().replace(/'/g, '’');
  if (t === '') return t;
  const p = t[0].toUpperCase() + t.slice(1);
  return /[.!?]$/.test(p) ? p : `${p}.`;
}
