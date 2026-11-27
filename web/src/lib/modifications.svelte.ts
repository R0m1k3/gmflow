// Modifications non enregistrées (U13) : demande avant de quitter une fiche ou un formulaire
// modifié, que ce soit dans GMFlow (`beforeNavigate`) ou en fermant l'onglet (`beforeunload`).
// À appeler pendant l'initialisation d'un composant.
import { beforeNavigate } from '$app/navigation';
import { MESSAGE_QUITTER, quitte } from './quitter.ts';

/**
 * `modifie` : vrai tant qu'il y a quelque chose à perdre. `cles` : paramètres de l'adresse dont
 * le changement quitte aussi (la fiche ouverte dans une liste : `doc`).
 */
export function garderAvantDeQuitter(modifie: () => boolean, cles: string[] = []) {
  beforeNavigate((nav) => {
    if (!modifie() || nav.willUnload) return;
    if (!nav.from || !quitte(nav.from.url, nav.to?.url, cles)) return;
    if (!confirm(MESSAGE_QUITTER)) nav.cancel();
  });
  $effect(() => {
    const avant = (e: BeforeUnloadEvent) => {
      if (!modifie()) return;
      e.preventDefault();
      // Anciens navigateurs : une chaîne non vide demande la confirmation.
      e.returnValue = MESSAGE_QUITTER;
    };
    window.addEventListener('beforeunload', avant);
    return () => window.removeEventListener('beforeunload', avant);
  });
}
