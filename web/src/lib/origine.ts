// D'où vient chaque valeur de la fiche d'une pièce (U9) : lue avec certitude, lue avec doute,
// corrigée par X le …, calculée, ou modifiée mais pas encore enregistrée. Les corrections
// viennent de `corrections` (état net par champ) et de `historique` (journal : qui, quand) de
// `GET /api/documents/{id}` ; le doute, de la confiance de la lecture. Module pur : testé par
// `node --test` (origine.test.ts).
import type { CorrectionChamp } from './types.ts';

/** Champs de la fiche, par leur nom en base. */
export type ChampFiche =
  | 'type'
  | 'societe_id'
  | 'fournisseur_id'
  | 'numero'
  | 'date_document'
  | 'numero_commande'
  | 'date_echeance'
  | 'montant_ht'
  | 'montant_tva'
  | 'montant_ttc';

export type GenreOrigine = 'sur' | 'doute' | 'corrige' | 'calcule' | 'modifie';

export interface Origine {
  genre: GenreOrigine;
  /** « Lue avec certitude », « Corrigée par marie le 12/11/2026 à 14:05 ». */
  libelle: string;
  /** Corrections de ce champ, de la plus récente à la plus ancienne. */
  historique: CorrectionChamp[];
}

/** Ce que la fiche sait d'un document, pour dire l'origine de ses valeurs. */
export interface SourceFiche {
  corrections: Record<string, { avant: unknown; apres: unknown }>;
  historique: CorrectionChamp[];
  confiance: { montants_calcules?: ('ht' | 'tva' | 'ttc')[] } | null;
  /** Voie de lecture : `facturx`, `xml` = données structurées, sans doute possible. */
  voie: string | null;
  /** Pièce vérifiée par un humain : une valeur lue avec doute a été relue. */
  verification?: { par: string | null; quand: string } | null;
}

/** Libellés courts de la légende, dans l'ordre. */
export const LEGENDE: { genre: GenreOrigine; libelle: string }[] = [
  { genre: 'sur', libelle: 'lue avec certitude' },
  { genre: 'doute', libelle: 'lue avec doute' },
  { genre: 'corrige', libelle: 'corrigée à la main' },
  { genre: 'calcule', libelle: 'calculée' },
  { genre: 'modifie', libelle: 'modifiée, pas encore enregistrée' }
];

const quandFr = new Intl.DateTimeFormat('fr-FR', {
  timeZone: 'Europe/Paris',
  day: '2-digit',
  month: '2-digit',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit'
});

/** « 12/11/2026 à 14:05 » (heure de Paris). */
export function quandCorrection(iso: string): string {
  return quandFr.format(new Date(iso)).replace(' ', ' à ');
}

const CALCULES: Partial<Record<ChampFiche, 'ht' | 'tva' | 'ttc'>> = { montant_ht: 'ht', montant_tva: 'tva', montant_ttc: 'ttc' };

/**
 * Origine de la valeur du champ `champ`. `vide` : rien de lu ni de saisi (pas de pastille).
 * `douteuse` : la lecture est peu sûre (confiance basse). `modifiee` : changée dans la fiche,
 * pas encore enregistrée.
 */
export function origineChamp(
  champ: ChampFiche,
  d: SourceFiche,
  o: { vide: boolean; douteuse: boolean; modifiee: boolean }
): Origine | null {
  const historique = d.historique.filter((c) => c.champ === champ).reverse();
  if (o.modifiee) return { genre: 'modifie', libelle: 'Modifiée, pas encore enregistrée', historique };
  if (d.corrections?.[champ]) {
    const der = historique[0];
    const libelle = der
      ? `Corrigée par ${der.par ?? 'un compte supprimé'} le ${quandCorrection(der.quand)}`
      : 'Corrigée à la main';
    return { genre: 'corrige', libelle, historique };
  }
  if (o.vide) return null;
  const c = CALCULES[champ];
  if (c && d.confiance?.montants_calcules?.includes(c))
    return { genre: 'calcule', libelle: 'Calculée à partir des lignes (non imprimée sur la pièce)', historique };
  if (d.voie === 'facturx' || d.voie === 'xml')
    return { genre: 'sur', libelle: 'Lue dans les données de la facture électronique', historique };
  const v = d.verification;
  if (o.douteuse && v)
    return { genre: 'sur', libelle: `Lue avec doute, vérifiée par ${v.par ?? 'un compte supprimé'} le ${quandCorrection(v.quand)}`, historique };
  if (o.douteuse) return { genre: 'doute', libelle: 'Lue avec doute : à vérifier sur la pièce', historique };
  return { genre: 'sur', libelle: 'Lue avec certitude', historique };
}
