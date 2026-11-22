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

const jourSeul = new Intl.DateTimeFormat('fr-FR', { timeZone: 'UTC', dateStyle: 'medium' });

/** Date sans heure (AAAA-MM-JJ). */
export function dateSimple(j: string | null | undefined): string {
  return j ? jourSeul.format(new Date(`${j}T00:00:00Z`)) : '—';
}
