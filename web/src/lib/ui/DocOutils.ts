// Petits utilitaires des écrans À traiter et fiche document (dates courtes, montants saisis,
// alertes rattachées aux champs). Le vocabulaire affiché reste dans #lib/libelles.ts.
import { libelleType, titreAlerte } from '#lib/libelles.ts';
import type { Controle, DocumentAValider, FicheDocument, TypeDocument } from '#lib/types.ts';

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

const fmtMontant = new Intl.NumberFormat('fr-FR', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const montantFr = (s: string) => fmtMontant.format(Number(s)).replace(/[\u202f\s]/g, '\u00a0');

/** Message d'une alerte dans le vocabulaire de l'écran (la « structure » devient la « société »). */
export function phraseAlerte(message: string): string {
  const t = message
    .trim()
    .replace(/^HT \+ TVA = (\S+) € pour un TTC de (\S+) €$/, 'HT + TVA donnent $1 €, le total lu est $2 €. Vérifiez le total sur la pièce')
    .replace(/-?\d+\.\d{2}(?= €)/g, montantFr)
    .replace(/^doublon\s*:\s*/i, '')
    .replace(/\s*;\s*envoi bloqué\s*$/, '')
    .replace(/structure destinataire/g, 'société')
    .replace(/la structure/g, 'la société')
    .replace(/structure/g, 'société')
    .replace(/adresse de dépôt/g, 'adresse Dext / Yooz');
  return t.charAt(0).toUpperCase() + t.slice(1);
}

/** La même phrase, avec son point final (pour un bandeau). */
export function phrase(message: string): string {
  const t = phraseAlerte(message);
  return /[.!?…]$/.test(t) ? t : `${t}.`;
}

/** Code de contrôle déduit de la phrase d'alerte de la file (la file ne rend que la phrase). */
export function codeAlerte(message: string): string {
  const m = message.toLowerCase();
  if (m.includes('doublon') || m.includes('même facture')) return 'doublon';
  if (m.includes('ht + tva') || m.includes('ht ou tva')) return 'montants';
  if (m.includes('champ manquant')) return 'incomplet';
  if (m.includes('siret') || m.includes('tva')) return 'identifiants';
  if (m.includes('adresse de dépôt')) return 'depot';
  if (m.includes('structure') || m.includes('société')) return 'societe';
  if (m.includes('fournisseur')) return 'fournisseur';
  if (m.includes('date')) return 'date';
  return '';
}

/** Alerte courte d'une ligne de la file (« Montant incohérent ») et sa couleur. */
export function drapeau(l: DocumentAValider): { texte: string; ton: 'w' | 'e' } | null {
  if (l.alerte) {
    const code = codeAlerte(l.alerte);
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

/** Champs visés par une alerte (pour les surligner en ambre). */
export function champsDeLAlerte(c: Controle): CleChamp[] {
  switch (c.code) {
    case 'montants':
      return c.message.includes('manquant') ? ['ht', 'tva'] : ['ttc'];
    case 'incomplet': {
      const m = c.message;
      const r: CleChamp[] = [];
      if (m.includes('numéro')) r.push('numero');
      if (m.includes('date')) r.push('date');
      if (m.includes('TTC')) r.push('ttc');
      return r;
    }
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
