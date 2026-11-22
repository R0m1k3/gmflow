// Raccourcis clavier d'une page. Usage dans un composant :
//   $effect(() => raccourcis({ j: suivant, k: precedent, Enter: ouvrir, Escape: fermer }));
// Les touches sont des noms de `KeyboardEvent.key` (insensibles à la casse pour les lettres),
// avec préfixe éventuel « Ctrl+ » (Ctrl ou Cmd), « Maj+ », « Alt+ ».
// Ignorés quand on tape dans un champ, sauf ceux listés dans `dansLesChamps`
// (par défaut Escape et Ctrl+Enter), et quand un dialogue modal est ouvert.

export type Gestionnaire = (e: KeyboardEvent) => void | boolean;

export interface OptionsRaccourcis {
  /** Raccourcis actifs même quand le focus est dans un champ. */
  dansLesChamps?: string[];
}

export function dansUnChamp(cible: EventTarget | null): boolean {
  if (!(cible instanceof HTMLElement)) return false;
  if (cible.isContentEditable) return true;
  const tag = cible.tagName;
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (tag === 'INPUT') {
    const t = (cible as HTMLInputElement).type;
    return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range'].includes(t);
  }
  return false;
}

/** Nom normalisé d'un événement : « Ctrl+Enter », « j », « / », « Escape ». */
export function nomTouche(e: KeyboardEvent): string {
  let k = e.key;
  if (k.length === 1) k = k.toLowerCase();
  const p: string[] = [];
  if (e.ctrlKey || e.metaKey) p.push('Ctrl');
  if (e.altKey) p.push('Alt');
  // Maj ne compte que pour les touches nommées : « ? » ou « / » dépendent déjà du clavier.
  if (e.shiftKey && e.key.length > 1) p.push('Maj');
  p.push(k);
  return p.join('+');
}

function normaliser(nom: string): string {
  const morceaux = nom.split('+');
  const k = morceaux.pop() ?? '';
  const mods = morceaux.map((m) => (m.toLowerCase() === 'cmd' || m.toLowerCase() === 'mod' ? 'Ctrl' : m));
  const ordre = ['Ctrl', 'Alt', 'Maj'];
  mods.sort((a, b) => ordre.indexOf(a) - ordre.indexOf(b));
  return [...mods, k.length === 1 ? k.toLowerCase() : k].join('+');
}

/**
 * Installe des raccourcis sur `window` ; rend la fonction qui les retire (à rendre depuis `$effect`).
 * Un gestionnaire qui rend `false` laisse passer l'événement (pas de `preventDefault`).
 */
export function raccourcis(carte: Record<string, Gestionnaire>, options: OptionsRaccourcis = {}): () => void {
  const table = new Map<string, Gestionnaire>();
  for (const [nom, f] of Object.entries(carte)) table.set(normaliser(nom), f);
  const champs = new Set((options.dansLesChamps ?? ['Escape', 'Ctrl+Enter']).map(normaliser));

  const ecoute = (e: KeyboardEvent) => {
    if (e.defaultPrevented || e.isComposing) return;
    const nom = nomTouche(e);
    const f = table.get(nom);
    if (!f) return;
    if (dansUnChamp(e.target) && !champs.has(nom)) return;
    // Entrée sur un bouton ou un lien qui a le focus : on le laisse s'activer normalement.
    if (nom === 'Enter' && e.target instanceof HTMLElement && e.target.closest('button, a[href], summary, [role="button"], [role="menuitem"]'))
      return;
    const dialogue = document.querySelector('dialog[open]');
    if (dialogue && !(e.target instanceof Node && dialogue.contains(e.target))) return;
    if (f(e) !== false) e.preventDefault();
  };
  window.addEventListener('keydown', ecoute);
  return () => window.removeEventListener('keydown', ecoute);
}

/** Indice suivant / précédent dans une liste, borné : pour J et K. */
export function deplacer(indice: number, sens: 1 | -1, longueur: number): number {
  if (longueur === 0) return -1;
  if (indice < 0) return sens === 1 ? 0 : longueur - 1;
  return Math.max(0, Math.min(longueur - 1, indice + sens));
}

/** Fait défiler l'élément `[data-indice=i]` du conteneur jusqu'à le rendre visible. */
export function montrer(conteneur: HTMLElement | null | undefined, selecteur: string) {
  conteneur?.querySelector(selecteur)?.scrollIntoView({ block: 'nearest' });
}
