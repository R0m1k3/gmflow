// Images distantes des mails HTML (constat C23) : bloquées par défaut (pixel espion), affichées
// à la demande, ou toujours pour les expéditeurs choisis (mémoire du navigateur). Fonctions
// pures, testées par `npm test`.

/** Clé du localStorage : adresses des expéditeurs dont les images s'affichent toujours. */
export const CLE_EXPEDITEURS = 'gmflow.images.expediteurs';

type Lecture = Pick<Storage, 'getItem'>;
type Ecriture = Pick<Storage, 'getItem' | 'setItem'>;

/** Adresse seule, en minuscules : « Jean <J@x.fr> » → « j@x.fr ». */
export function adresseDe(expediteur: string): string {
  const entre = expediteur.match(/<([^>]+)>/)?.[1] ?? expediteur;
  return entre.trim().toLowerCase();
}

export function expediteursAutorises(stockage: Lecture | null | undefined): string[] {
  try {
    const v = JSON.parse(stockage?.getItem(CLE_EXPEDITEURS) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

export function toujoursAfficher(expediteur: string, stockage: Lecture | null | undefined): boolean {
  const a = adresseDe(expediteur);
  return a !== '' && expediteursAutorises(stockage).includes(a);
}

/** Retient l'expéditeur (sans doublon). */
export function memoriser(expediteur: string, stockage: Ecriture | null | undefined): void {
  const a = adresseDe(expediteur);
  if (!stockage || a === '') return;
  const liste = expediteursAutorises(stockage);
  if (!liste.includes(a)) liste.push(a);
  try {
    stockage.setItem(CLE_EXPEDITEURS, JSON.stringify(liste));
  } catch {
    /* stockage plein ou refusé : le choix vaut pour cette fois */
  }
}

const ATTRIBUT_DISTANT = /\b(?:src|srcset|background|poster)\s*=\s*["']?\s*(?:https?:)?\/\//i;
const URL_CSS_DISTANTE = /url\(\s*["']?\s*(?:https?:)?\/\//i;

/** Le HTML charge-t-il des images (ou fonds) depuis Internet ? Les `data:` ne comptent pas. */
export function contientImagesDistantes(html: string): boolean {
  return ATTRIBUT_DISTANT.test(html) || URL_CSS_DISTANTE.test(html);
}

/** Adresse du corps HTML d'un mail, avec ou sans les images distantes. */
export function adresseCorps(id: number, images: boolean): string {
  return `/api/mails/${id}/html${images ? '?images=1' : ''}`;
}
