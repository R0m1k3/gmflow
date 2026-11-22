// Listes paginées des onglets Mails, Factures, BL et À répondre (réponses `Page<T>` du serveur,
// 50 par page) : lecture de la première page, « plus anciens », relecture sans perdre ce qui est affiché.
import { api, message } from './api.ts';
import type { Page } from './types.ts';

export class Liste<T extends { id: number }> {
  elements = $state<T[]>([]);
  total = $state(0);
  charge = $state(false);
  enCours = $state(false);
  erreur = $state('');
  private page = 1;
  private tour = 0;

  /** `adresse(page)` : l'URL de l'API pour une page donnée (filtres et société compris). */
  constructor(private adresse: (page: number) => string) {}

  /** Première page (au changement de filtre ou de société). `garder` : relire autant de pages qu'affiché. */
  async lire(garder = false) {
    const moi = ++this.tour;
    const pages = garder ? this.page : 1;
    this.enCours = true;
    try {
      let tout: T[] = [];
      let total = 0;
      for (let p = 1; p <= pages; p++) {
        const r = await api.get<Page<T>>(this.adresse(p));
        if (moi !== this.tour) return;
        tout = tout.concat(r.elements);
        total = r.total;
        if (r.elements.length === 0) break;
      }
      this.elements = tout;
      this.total = total;
      this.page = pages;
      this.erreur = '';
      this.charge = true;
    } catch (e) {
      if (moi === this.tour) this.erreur = message(e);
    } finally {
      if (moi === this.tour) this.enCours = false;
    }
  }

  /** Page suivante, ajoutée au bout. */
  async plus() {
    if (this.enCours || this.elements.length >= this.total) return;
    const moi = ++this.tour;
    this.enCours = true;
    try {
      const r = await api.get<Page<T>>(this.adresse(this.page + 1));
      if (moi !== this.tour) return;
      const vus = new Set(this.elements.map((e) => e.id));
      this.elements = this.elements.concat(r.elements.filter((e) => !vus.has(e.id)));
      this.total = r.total;
      this.page += 1;
    } catch (e) {
      if (moi === this.tour) this.erreur = message(e);
    } finally {
      if (moi === this.tour) this.enCours = false;
    }
  }

  get reste() {
    return Math.max(0, this.total - this.elements.length);
  }
}

const cleJour = new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Paris' });
const jourLong = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', weekday: 'long', day: 'numeric', month: 'long' });
const jourLongAn = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: 'numeric', month: 'long', year: 'numeric' });

/** « Aujourd’hui », « Hier », « Jeudi 20 novembre », « 3 mars 2025 » (heure de Paris). */
export function titreJour(iso: string | null | undefined): string {
  if (!iso) return 'Sans date';
  const d = new Date(iso);
  const k = cleJour.format(d);
  const auj = cleJour.format(new Date());
  if (k === auj) return 'Aujourd’hui';
  if (k === cleJour.format(new Date(Date.now() - 86_400_000))) return 'Hier';
  const t = k.slice(0, 4) === auj.slice(0, 4) && Date.now() - d.getTime() < 300 * 86_400_000 ? jourLong.format(d) : jourLongAn.format(d);
  return t.charAt(0).toUpperCase() + t.slice(1);
}

/** Regroupe une liste déjà triée par date, un groupe par jour. */
export function parJour<T>(elements: T[], date: (e: T) => string | null | undefined): { titre: string; elements: T[] }[] {
  const groupes: { titre: string; elements: T[] }[] = [];
  for (const e of elements) {
    const t = titreJour(date(e));
    const g = groupes[groupes.length - 1];
    if (g && g.titre === t) g.elements.push(e);
    else groupes.push({ titre: t, elements: [e] });
  }
  return groupes;
}
