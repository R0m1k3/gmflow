// Petits utilitaires des écrans À traiter et fiche document (dates courtes, montants saisis,
// alertes rattachées aux champs). Le vocabulaire affiché reste dans #lib/libelles.ts.
import { libelleType, titreAlerte } from '#lib/libelles.ts';
import { formatMontant } from './Montant.svelte';
import type { ChampControle, Controle, DocumentAValider, EcartLigne, FicheDocument, TypeDocument } from '#lib/types.ts';

const heure = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', hour: '2-digit', minute: '2-digit' });
const jourMois = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: '2-digit', month: '2-digit' });
const jourParis = new Intl.DateTimeFormat('fr-CA', { timeZone: 'Europe/Paris', year: 'numeric', month: '2-digit', day: '2-digit' });

/** Jour (AAAA-MM-JJ) d'un instant, à l'heure de Paris. */
function jourDe(d: Date): string {
  return jourParis.format(d);
}

/** Nombre de jours entiers entre deux jours AAAA-MM-JJ. */
function ecartJours(a: string, b: string): number {
  return Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / 86_400_000);
}

/** Âge d'un document de la file : « 09:41 » aujourd'hui, « hier », puis « 8 j ». */
export function age(iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  const n = ecartJours(jourDe(d), jourDe(new Date()));
  if (n <= 0) return heure.format(d);
  if (n === 1) return 'hier';
  return `${n} j`;
}

/** Date d'un mail dans la file : « 09:41 » aujourd'hui, « hier », puis « 20/11 ». */
export function quandCourt(iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  const n = ecartJours(jourDe(d), jourDe(new Date()));
  if (n <= 0) return heure.format(d);
  if (n === 1) return 'hier';
  return jourMois.format(d);
}

/** « 14/11 08:57 » */
export function jourHeure(iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  return `${jourMois.format(d)} ${heure.format(d)}`;
}

/** AAAA-MM-JJ en « 14/11/2026 » (ou « 14/11 » avec `court`). */
export function dateFr(j: string | null | undefined, court = false): string {
  if (!j) return '';
  const [a, m, d] = j.split('-');
  return court ? `${d}/${m}` : `${d}/${m}/${a}`;
}

/** Nom affiché d'un expéditeur : « Céline Marchal <c@x.fr> » donne « Céline Marchal ». */
export function nomExpediteur(e: string | null | undefined): string {
  if (!e) return '';
  const m = e.match(/^\s*"?([^"<]+?)"?\s*<[^>]+>\s*$/);
  return m ? m[1] : e.trim();
}

/** Montant saisi (« 1 841,76 € », « 1.841,76 », « 1841.76 ») ; null si vide, NaN si illisible. */
export function lireMontant(s: string): number | null {
  let t = s.replace(/[\s\u00a0\u202f€]/g, '').replace('\u2212', '-');
  if (t === '') return null;
  if (t.includes(',')) t = t.replace(/\./g, '').replace(',', '.');
  if (!/^-?\d+(\.\d+)?$/.test(t)) return NaN;
  return Math.round(Number(t) * 100) / 100;
}

/** Montant à saisir : « 1841,76 » (pas de séparateur de milliers, virgule décimale). */
export function ecrireMontant(v: number | null | undefined): string {
  return v === null || v === undefined ? '' : v.toFixed(2).replace('.', ',');
}

/** Une phrase du serveur (déjà dans le vocabulaire de l'écran), avec son point final (pour un bandeau). */
export function phrase(message: string): string {
  const t = message.trim();
  return /[.!?…]$/.test(t) ? t : `${t}.`;
}

/** Alerte courte d'une ligne de la file (« Montant incohérent ») et sa couleur. */
export function drapeau(l: DocumentAValider): { texte: string; ton: 'w' | 'e' } | null {
  if (l.alerte) {
    const code = l.alerte_code ?? '';
    const texte = code === 'depot' ? 'Pas d’adresse Dext / Yooz' : titreAlerte({ code, message: l.alerte });
    return { texte, ton: code === 'doublon' ? 'e' : 'w' };
  }
  if (l.fournisseur_a_confirmer) return { texte: 'Nouveau fournisseur', ton: 'w' };
  return null;
}

/** « Vileda, facture F2611-08734 » : ce qui s'ouvrira ensuite. */
export function nomCourtDocument(l: {
  fournisseur: string | null;
  type: TypeDocument | null;
  numero: string | null;
}): string {
  const type = libelleType(l.type);
  const t = `${l.type === 'bl' ? type : type.toLowerCase()}${l.numero ? ` ${l.numero}` : ''}`;
  return l.fournisseur ? `${l.fournisseur}, ${t}` : t.charAt(0).toUpperCase() + t.slice(1);
}

export type CleChamp = 'type' | 'societe' | 'fournisseur' | 'numero' | 'date' | 'ht' | 'tva' | 'ttc';

const CHAMPS: Record<ChampControle, CleChamp> = {
  numero: 'numero',
  date_document: 'date',
  montant_ht: 'ht',
  montant_tva: 'tva',
  montant_ttc: 'ttc',
  societe_id: 'societe',
  fournisseur_id: 'fournisseur'
};

/** Champs visés par une alerte (pour les surligner en ambre) : ceux que donne le serveur. */
export function champsDeLAlerte(c: Controle): CleChamp[] {
  if (c.champs) return c.champs.map((k) => CHAMPS[k]).filter((k) => k !== undefined);
  // Contrôles enregistrés avant que le serveur ne donne les champs.
  switch (c.code) {
    case 'identifiants':
    case 'fournisseur':
      return ['fournisseur'];
    case 'societe':
    case 'depot':
      return ['societe'];
    case 'date':
      return ['date'];
    default:
      return [];
  }
}

/** Seuils de confiance de la lecture sous lesquels un champ est surligné. */
const SEUILS: Record<string, number> = { societe: 0.9 };
const SEUIL = 0.7;

/** Champs dont la lecture est peu sûre (confiance de Claude). */
export function champsPeuSurs(d: FicheDocument): Set<CleChamp> {
  const r = new Set<CleChamp>();
  const c = d.confiance ?? {};
  const bas = (k: keyof typeof c) => c[k] !== undefined && (c[k] as number) < (SEUILS[k] ?? SEUIL);
  if (bas('type')) r.add('type');
  if (bas('societe')) r.add('societe');
  if (bas('fournisseur')) r.add('fournisseur');
  if (bas('numero')) r.add('numero');
  if (bas('date')) r.add('date');
  if (bas('montants')) {
    r.add('ht');
    r.add('tva');
    r.add('ttc');
  }
  return r;
}

/** Taux de TVA plausible (« 20 % », « 5,5 % ») d'après HT et TVA, sinon ''. */
export function tauxTva(ht: number | null, tva: number | null): string {
  if (ht === null || tva === null || ht === 0) return '';
  const t = (tva / ht) * 100;
  for (const u of [20, 10, 5.5, 2.1, 8.5, 13, 0]) {
    if (Math.abs(t - u) < 0.15) return `${String(u).replace('.', ',')} %`;
  }
  return '';
}

/** Remise déduite sur une facture, si elle compte : « 280,16 € HT (20 %) ». */
export function texteRemise(remise: number | null | undefined, taux?: number | null): string {
  if (!remise || remise <= 0.005) return '';
  const t = taux && taux > 0 ? ` (${taux.toLocaleString('fr-FR', { maximumFractionDigits: 2 })}\u00a0%)` : '';
  return `${formatMontant(remise)} HT${t}`;
}

const qteFr = (v: number) => v.toLocaleString('fr-FR', { maximumFractionDigits: 3 });
const pluriel = (v: number, mot: string) => `${qteFr(v)}\u00a0${mot}${Math.abs(v) > 1 ? 's' : ''}`;

/** Écart d'un article entre facture et BL : titre court (« Manque au BL ») et détail. */
export function phraseEcartLigne(e: EcartLigne): { article: string; titre: string; detail: string; montant: number | null } {
  const genre = e.genre ?? (e.qte_facture > e.qte_bl ? 'manquant' : 'en_trop');
  const article = e.designation ?? e.reference ?? 'Article';
  const montant = e.montant_ht ?? null;
  if (genre === 'prix') {
    const pf = e.prix_facture ?? null;
    const pb = e.prix_bl ?? null;
    const detail = pf !== null && pb !== null ? `${formatMontant(pf)} facturé, ${formatMontant(pb)} au BL, pour ${qteFr(e.qte_facture)}` : 'prix différent';
    return { article, titre: 'Prix changé', detail, montant };
  }
  const detail = `${pluriel(e.qte_facture, 'facturé')}, ${pluriel(e.qte_bl, 'livré')}`;
  return { article, titre: genre === 'manquant' ? (e.qte_bl > 0 ? 'Livré en partie' : 'Manque au BL') : 'Livré, pas facturé', detail, montant };
}
