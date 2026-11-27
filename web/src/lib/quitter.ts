// Modifications non enregistrées (U13), partie pure : quand une navigation quitte ce qu'on
// modifie, et quels formulaires ont été touchés sans être envoyés. Le branchement sur SvelteKit
// (`beforeNavigate`, `beforeunload`) est dans modifications.svelte.ts. Testé par `node --test`.

export const MESSAGE_QUITTER = 'Des modifications ne sont pas enregistrées. Quitter quand même ? Elles seront perdues.';

interface Adresse {
  pathname: string;
  searchParams: Pick<URLSearchParams, 'get'>;
}

/**
 * La navigation de `de` vers `vers` quitte-t-elle ce qui est modifié ? Oui si la page change,
 * ou si l'un des paramètres `cles` change (la fiche ouverte : `doc`) ; `vers` absent (fermeture
 * de l'onglet, autre site) : oui.
 */
export function quitte(de: Adresse, vers: Adresse | null | undefined, cles: string[] = []): boolean {
  if (!vers) return true;
  if (de.pathname !== vers.pathname) return true;
  return cles.some((k) => de.searchParams.get(k) !== vers.searchParams.get(k));
}

/** Formulaires touchés (une saisie) et pas encore envoyés ; un formulaire retiré de la page est oublié. */
export class FormulairesTouches<F extends { isConnected: boolean }> {
  private touches = new Set<F>();

  toucher(f: F) {
    this.touches.add(f);
  }

  /** Envoyé (ou remis à zéro) : plus à garder. */
  oublier(f: F) {
    this.touches.delete(f);
  }

  vider() {
    this.touches.clear();
  }

  get modifie(): boolean {
    for (const f of this.touches) if (!f.isConnected) this.touches.delete(f);
    return this.touches.size > 0;
  }
}
