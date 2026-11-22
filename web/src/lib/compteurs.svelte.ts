// Compteurs de la barre du haut : badge « À traiter » et pastille d'état (admins).
// Lus par le layout (changement de page, de société, toutes les minutes) ; une page qui vient
// de valider ou d'envoyer quelque chose appelle `rafraichirCompteurs()`.
// Source : GET /api/a-traiter/compteurs (docs/api-refonte.md, crates/gmflow-app/src/routes/a_traiter.rs).
import { api } from './api.ts';

export interface Compteurs {
  /** Éléments à traiter : documents à valider, mails à répondre, blocages. */
  aTraiter: number;
  /** Problèmes à débloquer (admins), `null` si inconnu ou non admin. */
  problemes: number | null;
}

interface ReponseCompteurs {
  bloque: number;
  a_valider: number;
  a_repondre: number;
  total: number;
}

async function lireCompteurs(parametreSociete: string, admin: boolean): Promise<Compteurs> {
  const q = parametreSociete ? `?${parametreSociete}` : '';
  const r = await api.get<ReponseCompteurs>(`/api/a-traiter/compteurs${q}`);
  return { aTraiter: r.total, problemes: admin ? r.bloque : null };
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
