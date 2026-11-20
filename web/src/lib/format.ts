const dateHeure = new Intl.DateTimeFormat('fr-FR', {
  timeZone: 'Europe/Paris',
  dateStyle: 'short',
  timeStyle: 'short'
});
const date = new Intl.DateTimeFormat('fr-FR', { timeZone: 'Europe/Paris', dateStyle: 'medium' });

export function quand(iso: string | null | undefined): string {
  return iso ? dateHeure.format(new Date(iso)) : '—';
}

export function jour(iso: string | null | undefined): string {
  return iso ? date.format(new Date(iso)) : '—';
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
