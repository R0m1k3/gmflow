// Compteurs de la barre du haut : badge « À traiter » et pastille d'état (admins).
// Lus par le layout (changement de page, de société, toutes les minutes) ; une page qui vient
// de valider ou d'envoyer quelque chose appelle `rafraichirCompteurs()`.
// Cible : GET /api/a-traiter/compteurs (contrat dans docs/api-refonte.md).
// Tant qu'il ne répond pas, repli sur GET /api/tableau-de-bord. Seule `lireCompteurs`
// est à ajuster quand le contrat sera figé.
import { api, ErreurApi } from './api.ts';
import type { Tableau } from './types.ts';

export interface Compteurs {
  /** Éléments à traiter : documents à valider, mails à répondre, blocages. */
  aTraiter: number;
  /** Problèmes à débloquer (admins), `null` si inconnu ou non admin. */
  problemes: number | null;
}

/** Réponse de /api/a-traiter/compteurs (crates/gmflow-app/src/routes/a_traiter.rs). */
interface ReponseCompteurs {
  bloque?: number;
  a_valider?: number;
  a_repondre?: number;
  total?: number;
}

let nouvelleRouteAbsente = false;

function depuisCompteurs(r: ReponseCompteurs, admin: boolean): Compteurs {
  const bloque = r.bloque ?? 0;
  const aTraiter = r.total ?? (r.a_valider ?? 0) + (r.a_repondre ?? 0) + bloque;
  return { aTraiter, problemes: admin ? bloque : null };
}

function depuisTableau(t: Tableau, admin: boolean): Compteurs {
  const claudeKo = t.claude && t.claude.etat !== 'valide' && t.claude.etat !== 'en_test' ? 1 : 0;
  const problemes = t.boites_alerte.length + t.depots.bloques + t.documents.erreur + claudeKo;
  return {
    aTraiter: t.documents.a_verifier + t.a_repondre + t.depots.bloques,
    problemes: admin ? problemes : null
  };
}

export async function lireCompteurs(parametreSociete: string, admin: boolean): Promise<Compteurs> {
  const q = parametreSociete ? `?${parametreSociete}` : '';
  if (!nouvelleRouteAbsente) {
    try {
      return depuisCompteurs(await api.get<ReponseCompteurs>(`/api/a-traiter/compteurs${q}`), admin);
    } catch (e) {
      // 404 ou 405 : route pas encore livrée, on ne la redemande plus pendant la session.
      if (e instanceof ErreurApi && (e.code === 404 || e.code === 405)) nouvelleRouteAbsente = true;
      else if (e instanceof ErreurApi && e.code === 401) throw e;
    }
  }
  return depuisTableau(await api.get<Tableau>(`/api/tableau-de-bord${q}`), admin);
}

/** Valeurs affichées dans la barre du haut. */
export const compteurs = $state<Compteurs & { charge: boolean }>({ aTraiter: 0, problemes: null, charge: false });

let parametreCourant = '';
let adminCourant = false;
let tour = 0;

/** Relit les compteurs ; sans argument, avec la société et le rôle du dernier appel. */
export async function rafraichirCompteurs(parametreSociete?: string, admin?: boolean): Promise<void> {
  if (parametreSociete !== undefined) parametreCourant = parametreSociete;
  if (admin !== undefined) adminCourant = admin;
  const moi = ++tour;
  try {
    const c = await lireCompteurs(parametreCourant, adminCourant);
    if (moi !== tour) return; // une lecture plus récente est partie entre-temps
    compteurs.aTraiter = c.aTraiter;
    compteurs.problemes = c.problemes;
    compteurs.charge = true;
  } catch {
    /* on garde les dernières valeurs */
  }
}
