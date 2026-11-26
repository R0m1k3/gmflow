// Dernier filtre choisi dans chaque liste (Factures, Avoirs, BL), gardé par navigateur dans localStorage.
const PREFIXE = 'gmflow.filtre.';

/** Filtre gardé pour la liste `cle` (son chemin), ou `null`. */
export function filtreGarde(cle: string): string | null {
  try {
    return localStorage.getItem(PREFIXE + cle);
  } catch {
    return null;
  }
}

/** Garde le filtre de la liste `cle` (`null` : l'oublie). */
export function garderFiltre(cle: string, valeur: string | null): void {
  try {
    if (valeur === null) localStorage.removeItem(PREFIXE + cle);
    else localStorage.setItem(PREFIXE + cle, valeur);
  } catch {
    // navigation privée : le filtre vaut pour la page ouverte seulement
  }
}
