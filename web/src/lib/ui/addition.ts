// Addition d'une facture face à ses BL (chaîne de la fiche document) : chaque BL, le livré, la
// remise et les montants annexes de la facture, l'attendu, le facturé et l'écart. Les montants
// viennent des pièces ; l'écart vient du serveur (`ecart_montant` : HT lu de la facture − somme
// des BL), jamais recalculé ici. Fonctions pures, sans import autre que des types (testées par
// `node --test`, voir addition.test.ts).
import type { EcartLigne, LigneArticle } from '#lib/types.ts';

/** Écart de montant HT au-delà duquel le lien est signalé (comme le serveur, SEUIL_MONTANT). */
export const SEUIL_ECART = 0.05;

const centimes = (x: number) => Math.round(x * 100) / 100;
const positif = (v: number | null | undefined) => (v && v > 0.005 ? v : 0);

/** Lettres et chiffres seulement, en majuscules (copie de `rapprochement::normaliser`). */
export function normaliser(s: string): string {
  return s.replace(/[^\p{L}\p{N}]/gu, '').toUpperCase();
}

const cle = (s: string | null | undefined) => (s ? normaliser(s) : '');

/** Écart d'article qui concerne cette ligne de BL : même référence, sinon même désignation. */
export function ecartDeLigne(l: LigneArticle, ecarts: EcartLigne[]): EcartLigne | null {
  const r = cle(l.reference);
  if (r) {
    const e = ecarts.find((x) => cle(x.reference) === r);
    if (e) return e;
  }
  const d = cle(l.designation);
  if (d) return ecarts.find((x) => cle(x.designation) === d) ?? null;
  return null;
}

export interface FactureAddition {
  /** HT lu, celui que le serveur a utilisé pour `ecart_montant`. */
  ht: number | null;
  remise_ht: number | null;
  remise_taux: number | null;
  annexes_ht: number | null;
}

export interface BlAddition {
  id: number;
  numero: string | null;
  ht: number | null;
}

export type GenreLigneAddition = 'bl' | 'somme' | 'autres' | 'remise' | 'annexes' | 'attendu' | 'facture' | 'ecart';

export interface LigneAddition {
  libelle: string;
  montant: number | null;
  genre: GenreLigneAddition;
  note?: string;
}

export interface Addition {
  lignes: LigneAddition[];
  /** sans : pas de comparaison ; concorde ; article : un article explique le reste ; reste : rien ne l'explique. */
  verdict: 'sans' | 'concorde' | 'article' | 'reste';
  /** Écart qui reste une fois la lecture gardée appliquée (= Facturé − Attendu), null sans comparaison. */
  reste: number | null;
  /** Articles en écart, toujours listés sous l'addition (même si le montant concorde). */
  causes: EcartLigne[];
  /** Lecture « après remise » : les BL sont déjà remisés, la remise n'est pas déduite. */
  blsRemises: boolean;
}

/**
 * Lignes de l'addition d'une facture face à ses BL visibles. `ecart` : `ecart_montant` du serveur
 * (null : pas de comparaison) ; `ecarts` : écarts d'articles de la facture, pris une fois ;
 * `ecoBl` : éco-participations déjà comprises dans le montant des BL (`eco_bl` du serveur, BL au
 * HT calculé depuis leurs lignes), retirées des annexes de la facture (`annexes_hors_bl`).
 * Lecture gardée (comme `ecart_explique`) : avant = e + r − a (BL avant remise), après = e − a
 * (BL déjà remisés), la plus proche de 0, a = annexes absentes des BL.
 */
export function additionRapprochement(
  facture: FactureAddition,
  bls: BlAddition[],
  ecart: number | null,
  ecarts: EcartLigne[],
  ecoBl: number | null = null
): Addition {
  const lignes: LigneAddition[] = [];
  const r = positif(facture.remise_ht);
  const eco = positif(ecoBl);
  const a = positif(centimes(positif(facture.annexes_ht) - eco));

  for (const b of bls) lignes.push({ libelle: `BL ${b.numero ?? b.id}`, montant: b.ht === null ? null : centimes(b.ht), genre: 'bl' });
  const somme = centimes(bls.reduce((t, b) => t + (b.ht ?? 0), 0));
  if (bls.length >= 2) lignes.push({ libelle: 'Livré', montant: somme, genre: 'somme' });

  // Ce que le rapprochement a compté pour les BL : HT facture − écart.
  let base = somme;
  if (ecart !== null && facture.ht !== null) {
    const retenu = centimes(facture.ht - ecart);
    if (Math.abs(retenu - somme) > 0.01) {
      lignes.push({ libelle: 'Autres montants retenus par le rapprochement', montant: centimes(retenu - somme), genre: 'autres' });
    }
    base = retenu;
  }

  let reste: number | null = null;
  let blsRemises = false;
  if (ecart !== null) {
    const avant = centimes(ecart + r - a);
    const apres = centimes(ecart - a);
    blsRemises = r > 0 && Math.abs(apres) < Math.abs(avant);
    reste = blsRemises ? apres : avant;
  }

  if (r > 0 && !blsRemises) {
    const t = facture.remise_taux && facture.remise_taux > 0 ? ` ${facture.remise_taux.toLocaleString('fr-FR', { maximumFractionDigits: 2 })}\u00a0%` : '';
    lignes.push({ libelle: `Remise${t} (facture seulement)`, montant: -centimes(r), genre: 'remise' });
  }
  if (a > 0) {
    lignes.push({
      libelle: eco > 0 ? 'Port, autres montants annexes (facture seulement)' : 'Éco-participation, port (facture seulement)',
      montant: centimes(a),
      genre: 'annexes'
    });
  }
  lignes.push({
    libelle: 'Attendu',
    montant: centimes(base - (blsRemises ? 0 : r) + a),
    genre: 'attendu',
    note: [blsRemises ? 'BL déjà remisés' : '', eco > 0 ? `éco-participation (${eco.toLocaleString('fr-FR', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}\u00a0€) comprise dans les BL` : '']
      .filter(Boolean)
      .join(', ') || undefined
  });
  lignes.push({ libelle: 'Facturé HT', montant: facture.ht === null ? null : centimes(facture.ht), genre: 'facture' });
  if (reste !== null) lignes.push({ libelle: 'Écart facture − BL', montant: reste, genre: 'ecart' });

  const verdict: Addition['verdict'] =
    reste === null ? 'sans' : Math.abs(reste) <= SEUIL_ECART ? 'concorde' : ecarts.length ? 'article' : 'reste';
  return { lignes, verdict, reste, causes: ecarts, blsRemises };
}
