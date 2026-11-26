const dateHeure = new Intl.DateTimeFormat('fr-FR', {
  timeZone: 'Europe/Paris',
  dateStyle: 'short',
  timeStyle: 'short'
});

export function quand(iso: string | null | undefined): string {
  return iso ? dateHeure.format(new Date(iso)) : '—';
}

export function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  const unites = ['Ko', 'Mo', 'Go', 'To'];
  let v = octets / 1024;
  let i = 0;
  while (v >= 1024 && i < unites.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString('fr-FR', { maximumFractionDigits: 1 })} ${unites[i]}`;
}

// Règle des dates : « 24/11/2026 » dans les champs et les tableaux, « 24 nov. » dans les listes
// (« 24 nov. 2025 » quand ce n'est pas l'année en cours).
const jourSeul = new Intl.DateTimeFormat('fr-FR', { timeZone: 'UTC', day: '2-digit', month: '2-digit', year: 'numeric' });
const jourListe = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: 'numeric', month: 'short' });
const jourListeAn = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', day: 'numeric', month: 'short', year: 'numeric' });
const anneeParis = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', year: 'numeric' });

/** Date sans heure (AAAA-MM-JJ) pour un champ ou un tableau : « 24/11/2026 ». */
export function dateSimple(j: string | null | undefined): string {
  return j ? jourSeul.format(new Date(`${j}T00:00:00Z`)) : '—';
}

/** Date d'un instant pour une liste : « 24 nov. » cette année, « 24 nov. 2025 » avant (heure de Paris). */
export function dateListe(d: Date): string {
  return (anneeParis.format(d) === anneeParis.format(new Date()) ? jourListe : jourListeAn).format(d);
}

/** Comme `dateListe`, pour un jour AAAA-MM-JJ. */
export function jourListeDe(j: string | null | undefined): string {
  return j ? dateListe(new Date(`${j}T12:00:00Z`)) : '';
}
