// Sections des réglages (U10) : une vue par section à son adresse (`/reglages/<section>`),
// groupées dans le menu latéral. Les anciennes ancres (`/reglages#boites`, liens gardés dans
// les favoris et les mails) mènent à la vue de la section. Module pur : testé par `node --test`
// (reglages.test.ts) et lu par le matcher de route `src/params.ts`.

export type CleSection =
  | 'societes'
  | 'utilisateurs'
  | 'boites'
  | 'claude'
  | 'envoi'
  | 'serveur-api'
  | 'logiflow'
  | 'traitements'
  | 'avance'
  | 'journal'
  | 'zone-dangereuse';

export interface GroupeReglages {
  titre: string;
  sections: CleSection[];
}

/** Ordre du menu. Toutes ces sections sont réservées aux administrateurs. */
export const GROUPES: GroupeReglages[] = [
  { titre: 'Magasins et accès', sections: ['societes', 'utilisateurs'] },
  { titre: 'Connexions', sections: ['boites', 'claude', 'envoi', 'serveur-api', 'logiflow'] },
  { titre: 'Maintenance', sections: ['traitements', 'avance', 'journal', 'zone-dangereuse'] }
];

export const SECTIONS: CleSection[] = GROUPES.flatMap((g) => g.sections);

export function estSection(s: string | null | undefined): s is CleSection {
  return !!s && (SECTIONS as string[]).includes(s);
}

/** Anciennes ancres qui ne portent pas le nom d'une section. */
const ANCIENNES: Record<string, string> = {
  'mise-en-route': '/reglages',
  historique: '/reglages/journal',
  limites: '/reglages/avance'
};

/**
 * Adresse d'une ancienne ancre de la page unique (`#boites`, `#mise-en-route`), ou `null` si
 * l'ancre n'en est pas une (rien à faire).
 */
export function adresseAncre(hash: string): string | null {
  const id = decodeURIComponent(hash.replace(/^#/, ''));
  if (estSection(id)) return `/reglages/${id}`;
  return ANCIENNES[id] ?? null;
}

/** Section affichée pour une adresse (`/reglages/boites` → `boites`), `null` sur l'accueil. */
export function sectionDe(pathname: string): CleSection | null {
  const m = /^\/reglages\/([^/]+)\/?$/.exec(pathname);
  return m && estSection(m[1]) ? m[1] : null;
}
