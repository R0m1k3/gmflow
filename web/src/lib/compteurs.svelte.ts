// Compteurs du menu : badges de Factures, Avoirs, BL, À répondre, et pastille d'état (admins).
// Lus par le layout (changement de page, de société, toutes les minutes) ; une page qui vient
// de valider ou d'envoyer quelque chose appelle `rafraichirCompteurs()`.
// Source : GET /api/a-traiter/compteurs (docs/api-refonte.md, crates/gmflow-app/src/routes/a_traiter.rs).
import { api } from './api.ts';
import type { Boite } from './types.ts';

export interface Compteurs {
  /** Éléments à traiter : documents à contrôler, mails à répondre, blocages. */
  aTraiter: number;
  /** Pièces à contrôler, par type : badges de Factures, Avoirs et BL. */
  factures: number;
  avoirs: number;
  bl: number;
  /** Mails qui attendent une réponse : onglet « À répondre ». */
  aRepondre: number;
  /** Envois à Dext / Yooz en échec, par type : pastille rouge de Factures et Avoirs. */
  echecsFactures: number;
  echecsAvoirs: number;
  /** Admins : date de la dernière relève des boîtes actives (`null` : aucune), et leur nombre. */
  derniereReleve: string | null;
  boitesActives: number;
  /** Problèmes à débloquer (admins), `null` si inconnu ou non admin. */
  problemes: number | null;
}

interface ReponseCompteurs {
  bloque: number;
  a_valider: number;
  a_valider_types: { facture: number; avoir: number; bl: number };
  a_repondre: number;
  total: number;
}

/** Nombre d'envois en échec d'un type (liste des documents, `statut_depot=erreur` : seul le total compte). */
async function echecs(type: 'facture' | 'avoir', parametreSociete: string): Promise<number> {
  try {
    const r = await api.get<{ total: number }>(
      `/api/documents?type=${type}&statut_depot=erreur${parametreSociete ? `&${parametreSociete}` : ''}`
    );
    return r.total;
  } catch {
    return 0;
  }
}

/** Dernière relève parmi les boîtes actives (liste des boîtes, admins seulement). */
async function releve(admin: boolean): Promise<{ derniereReleve: string | null; boitesActives: number } | null> {
  if (!admin) return { derniereReleve: null, boitesActives: 0 };
  try {
    const boites = (await api.get<Boite[]>('/api/admin/boites')).filter((b) => b.active && !b.pause_auto);
    const dates = boites.map((b) => b.derniere_releve).filter((x): x is string => !!x);
    return { derniereReleve: dates.sort().at(-1) ?? null, boitesActives: boites.length };
  } catch {
    return null;
  }
}

/** Demande une relève de chaque boîte active (admins). Rend le nombre de boîtes relancées. */
export async function releverMaintenant(): Promise<number> {
  const boites = (await api.get<Boite[]>('/api/admin/boites')).filter((b) => b.active && !b.pause_auto);
  await Promise.all(boites.map((b) => api.post(`/api/admin/boites/${b.id}/relever`)));
  return boites.length;
}

async function lireCompteurs(parametreSociete: string, admin: boolean): Promise<Compteurs> {
  const q = parametreSociete ? `?${parametreSociete}` : '';
  const [r, echecsFactures, echecsAvoirs, rel] = await Promise.all([
    api.get<ReponseCompteurs>(`/api/a-traiter/compteurs${q}`),
    echecs('facture', parametreSociete),
    echecs('avoir', parametreSociete),
    releve(admin)
  ]);
  return { echecsFactures, echecsAvoirs, derniereReleve: rel?.derniereReleve ?? null, boitesActives: rel?.boitesActives ?? 0, aTraiter: r.total, factures: r.a_valider_types.facture,
    avoirs: r.a_valider_types.avoir,
    bl: r.a_valider_types.bl,
    aRepondre: r.a_repondre, problemes: admin ? r.bloque : null };
}

/** Valeurs affichées dans la barre du haut. */
export const compteurs = $state<Compteurs & { charge: boolean }>({ aTraiter: 0, factures: 0, avoirs: 0, bl: 0, aRepondre: 0, echecsFactures: 0, echecsAvoirs: 0, derniereReleve: null, boitesActives: 0, problemes: null, charge: false });

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
    compteurs.factures = c.factures;
    compteurs.avoirs = c.avoirs;
    compteurs.bl = c.bl;
    compteurs.aRepondre = c.aRepondre;
    compteurs.echecsFactures = c.echecsFactures;
    compteurs.echecsAvoirs = c.echecsAvoirs;
    compteurs.derniereReleve = c.derniereReleve;
    compteurs.boitesActives = c.boitesActives;
    compteurs.problemes = c.problemes;
    compteurs.charge = true;
  } catch {
    /* on garde les dernières valeurs */
  }
}
