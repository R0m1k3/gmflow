// Tri des listes par l'adresse (`?tri=montant&sens=asc`) : un clic sur un en-tête trie par cette
// colonne dans son sens habituel (dates et montants : le plus récent ou le plus grand d'abord ;
// noms : de A à Z), un second clic inverse. Le serveur fait le tri (liste blanche des colonnes,
// `crates/gmflow-app/src/routes/tri.rs`) ; le tri par défaut n'apparaît pas dans l'adresse.
// Module pur : testé par `node --test` (tri.test.ts).

export interface ColonneTri {
  /** Clé envoyée au serveur (`montant`) : une de sa liste blanche. */
  cle: string;
  libelle: string;
  /** Sens du premier clic. */
  croissant: boolean;
  /** Pour dire le sens en mots (« du plus récent au plus ancien »). */
  genre: 'date' | 'nombre' | 'texte';
}

export interface TriChoisi {
  cle: string;
  croissant: boolean;
  /** Tri par défaut de la liste (première colonne, dans son sens). */
  defaut: boolean;
}

/** Tri lu dans l'adresse ; une clé inconnue revient au tri par défaut (première colonne). */
export function lireTri(params: Pick<URLSearchParams, 'get'>, colonnes: ColonneTri[]): TriChoisi {
  const c = colonnes.find((x) => x.cle === params.get('tri')) ?? colonnes[0];
  const sens = params.get('sens');
  const croissant = sens === 'asc' ? true : sens === 'desc' ? false : c.croissant;
  return { cle: c.cle, croissant, defaut: c === colonnes[0] && croissant === c.croissant };
}

/** Paramètres à envoyer au serveur (rien pour le tri par défaut). */
export function parametresTri(t: TriChoisi): { tri?: string; sens?: string } {
  return t.defaut ? {} : { tri: t.cle, sens: t.croissant ? 'asc' : 'desc' };
}

/**
 * Adresse après un clic sur l'en-tête `cle` : même colonne = sens inversé, autre colonne = son
 * sens habituel. Les autres paramètres sont gardés, sauf la fiche ouverte (`retirer`) : la liste
 * repart du début.
 */
export function lienTri(url: { pathname: string; search: string }, colonnes: ColonneTri[], cle: string, retirer: string[] = []): string {
  const p = new URLSearchParams(url.search);
  const actuel = lireTri(p, colonnes);
  const c = colonnes.find((x) => x.cle === cle) ?? colonnes[0];
  const croissant = actuel.cle === c.cle ? !actuel.croissant : c.croissant;
  for (const r of retirer) p.delete(r);
  p.delete('tri');
  p.delete('sens');
  if (!(c === colonnes[0] && croissant === c.croissant)) {
    p.set('tri', c.cle);
    p.set('sens', croissant ? 'asc' : 'desc');
  }
  const q = p.toString();
  return `${url.pathname}${q ? `?${q}` : ''}`;
}

/** Libellé lu par les lecteurs d'écran et en infobulle : « Montant, du plus grand au plus petit ». */
export function sensLisible(genre: ColonneTri['genre'], croissant: boolean): string {
  if (genre === 'date') return croissant ? 'du plus ancien au plus récent' : 'du plus récent au plus ancien';
  if (genre === 'nombre') return croissant ? 'du plus petit au plus grand' : 'du plus grand au plus petit';
  return croissant ? 'de A à Z' : 'de Z à A';
}
