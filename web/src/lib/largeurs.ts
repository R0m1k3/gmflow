// Largeurs des volets choisies à la souris (Poignee), gardées par navigateur dans localStorage.
const PREFIXE = 'gmflow.largeur.';

/** Largeur gardée sous `cle`, sinon `defaut`. */
export function largeurGardee(cle: string, defaut: number): number {
  try {
    const n = Number(localStorage.getItem(PREFIXE + cle));
    return Number.isFinite(n) && n > 0 ? n : defaut;
  } catch {
    return defaut;
  }
}

/** Garde la largeur (`null` : revient à la largeur d'origine). */
export function garderLargeur(cle: string, largeur: number | null): void {
  try {
    if (largeur === null) localStorage.removeItem(PREFIXE + cle);
    else localStorage.setItem(PREFIXE + cle, String(largeur));
  } catch {
    // navigation privée : la largeur vaut pour la page ouverte seulement
  }
}
