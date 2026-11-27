// Erreurs de l'API traduites pour les gens du magasin (U8) : ce qui s'est passé, en français
// simple, ce qu'il faut faire, et quand c'est possible le bouton qui le fait (réessayer, ouvrir
// les réglages concernés, se reconnecter). Le serveur donne à chaque erreur un `code` stable
// (`crates/gmflow-app/src/erreur.rs`) : c'est lui qui est lu ici, jamais la phrase, sauf pour
// les refus dont la phrase du serveur est déjà le bon message (donnée refusée, conflit).
// Module pur (pas d'import de SvelteKit) : testé par `node --test` (erreurs.test.ts).

/** Ce que le bouton d'une erreur fait : ouvrir une page, relancer l'action, ou se reconnecter. */
export type ActionErreur =
  | { genre: 'lien'; libelle: string; href: string }
  | { genre: 'reessayer'; libelle: string }
  | { genre: 'reconnecter'; libelle: string; href: string };

export interface Explication {
  /** Ce qui s'est passé, une phrase. */
  message: string;
  /** Que faire, une phrase (vide si rien à faire). */
  quoiFaire: string;
  action: ActionErreur | null;
  /** Code de l'erreur (`absent`, `expedition`…, `reseau` sans réponse du serveur). */
  code: string;
  /** Code HTTP (0 sans réponse). */
  statut: number;
}

/** Forme de `ErreurApi` (api.ts) lue sans l'importer : `code` HTTP, `raison` = code stable. */
interface ErreurLue {
  code?: unknown;
  raison?: unknown;
  message?: unknown;
}

const REGLAGES = (section: string) => `/reglages/${section}`;

/** Phrase du serveur (« document déjà déposé ») mise en forme : majuscule et point final. */
export function phrase(t: string): string {
  const s = t.trim();
  if (!s) return '';
  const m = s.charAt(0).toUpperCase() + s.slice(1);
  return /[.!?…]$/.test(m) ? m : `${m}.`;
}

type Traduction = (serveur: string, admin: boolean) => Omit<Explication, 'code' | 'statut'>;

const reessayer = (libelle = 'Réessayer'): ActionErreur => ({ genre: 'reessayer', libelle });
const lien = (libelle: string, href: string): ActionErreur => ({ genre: 'lien', libelle, href });
/** Réglage manquant : l'administrateur y va, un responsable le signale. */
function reglage(message: string, section: string, libelle: string): Traduction {
  return (_s, admin) => ({
    message,
    quoiFaire: admin ? 'Complétez ce réglage, puis recommencez.' : 'Prévenez l’administrateur : c’est un réglage à compléter.',
    action: admin ? lien(libelle, REGLAGES(section)) : null
  });
}

/** Par code stable du serveur. */
const PAR_CODE: Record<string, Traduction> = {
  non_connecte: () => ({
    message: 'Votre session a expiré.',
    quoiFaire: 'Reconnectez-vous pour continuer.',
    action: { genre: 'reconnecter', libelle: 'Se reconnecter', href: '/connexion' }
  }),
  interdit: () => ({
    message: 'Cette action est réservée à un administrateur.',
    quoiFaire: 'Demandez à l’administrateur de GMFlow de la faire pour vous.',
    action: null
  }),
  absent: () => ({
    message: 'Cet élément n’existe plus ou n’est pas visible pour vous.',
    quoiFaire: 'Il a peut-être été supprimé, fusionné ou rangé dans l’autre magasin. Rechargez la liste.',
    action: reessayer('Recharger')
  }),
  trop_d_essais: () => ({
    message: 'Trop d’essais de connexion à la suite.',
    quoiFaire: 'Attendez 15 minutes avant de réessayer.',
    action: null
  }),
  interne: () => ({
    message: 'Le serveur a rencontré un problème.',
    quoiFaire: 'Réessayez dans un instant. Si cela recommence, prévenez l’administrateur.',
    action: reessayer()
  }),
  reseau: () => ({
    message: 'Le serveur ne répond pas.',
    quoiFaire: 'Vérifiez la connexion internet, puis réessayez.',
    action: reessayer()
  }),
  donnees: (s) => ({ message: phrase(s) || 'Une valeur est refusée.', quoiFaire: 'Corrigez la valeur, puis recommencez.', action: null }),
  champ: (s) => ({ message: phrase(s) || 'Une valeur est refusée.', quoiFaire: 'Corrigez le champ signalé, puis recommencez.', action: null }),
  conflit: (s) => ({
    message: phrase(s) || 'L’action n’est plus possible.',
    quoiFaire: 'Quelqu’un ou le traitement automatique a pu la changer entre-temps : rechargez pour voir son état.',
    action: reessayer('Recharger')
  }),
  // Refus métier (409 avec code) : docs/api-refonte.md.
  expedition: reglage('L’adresse d’envoi vers Dext / Yooz n’est pas réglée.', 'envoi', 'Régler l’expédition'),
  sans_adresse: reglage('Ce magasin n’a pas d’adresse Dext / Yooz.', 'societes', 'Régler les structures'),
  smtp_absent: reglage('Le serveur d’envoi (SMTP) de cette boîte n’est pas réglé.', 'boites', 'Régler les boîtes mail'),
  serveur_magasin_absent: reglage('Le serveur du magasin (Apiflow) n’est pas réglé.', 'serveur-api', 'Régler le serveur du magasin'),
  logiflow_absent: reglage('Logiflow n’est pas réglé.', 'logiflow', 'Régler Logiflow'),
  essai: (s) => ({
    message: phrase(s) || 'L’essai de connexion n’est plus valable.',
    quoiFaire: 'Relancez l’essai de connexion, puis enregistrez.',
    action: null
  }),
  deja_envoye: () => ({
    message: 'Cette pièce est déjà envoyée, ou en cours d’envoi.',
    quoiFaire: 'Rien à faire : rechargez pour voir où elle en est.',
    action: reessayer('Recharger')
  }),
  erreur_envoi: (_s, admin) => ({
    message: 'L’envoi précédent de cette pièce a échoué.',
    quoiFaire: admin
      ? 'Vérifiez chez Dext / Yooz qu’elle n’est pas arrivée, puis renvoyez-la.'
      : 'Prévenez l’administrateur : lui seul peut la renvoyer (elle est peut-être déjà arrivée).',
    action: null
  }),
  doublon: (s, admin) => ({
    message: phrase(s) || 'Cette facture a déjà été reçue.',
    quoiFaire: admin ? 'Écartez le doublon, ou envoyez quand même si ce n’en est pas un.' : 'Écartez le doublon.',
    action: null
  }),
  etat: (s) => ({
    message: phrase(s) || 'Cette pièce a changé d’état.',
    quoiFaire: 'Rechargez la pièce pour voir où elle en est.',
    action: reessayer('Recharger')
  }),
  alertes: (s) => ({
    message: phrase(s) || 'Cette pièce a des alertes.',
    quoiFaire: 'Ouvrez la pièce, lisez ses alertes et corrigez avant de valider.',
    action: null
  })
};

/** Sans code (ancien serveur, proxy) : d'après le code HTTP. */
function codeDuStatut(statut: number): string {
  if (statut === 0) return 'reseau';
  if (statut === 400 || statut === 422) return 'donnees';
  if (statut === 401) return 'non_connecte';
  if (statut === 403) return 'interdit';
  if (statut === 404) return 'absent';
  if (statut === 409) return 'conflit';
  if (statut === 429) return 'trop_d_essais';
  if (statut === 502 || statut === 503 || statut === 504) return 'reseau';
  return 'interne';
}

/**
 * Explique une erreur attrapée (`ErreurApi`, échec réseau, autre). `admin` : les actions vers
 * les réglages ne sont proposées qu'à un administrateur.
 */
export function expliquer(e: unknown, admin = false): Explication {
  const x = (e ?? {}) as ErreurLue;
  const deLApi = e instanceof Error && typeof x.code === 'number';
  // `fetch` qui échoue (serveur arrêté, réseau coupé) : TypeError sans code HTTP.
  const reseau = e instanceof TypeError;
  if (!deLApi && !reseau) {
    const m = e instanceof Error ? e.message : typeof e === 'string' ? e : '';
    return { message: phrase(m) || 'Quelque chose n’a pas marché.', quoiFaire: '', action: null, code: 'inconnu', statut: 0 };
  }
  const statut = deLApi ? (x.code as number) : 0;
  const raison = typeof x.raison === 'string' && x.raison ? x.raison : '';
  const code = reseau ? 'reseau' : raison && PAR_CODE[raison] ? raison : codeDuStatut(statut);
  const serveur = typeof x.message === 'string' ? x.message : '';
  // Phrase du serveur générique (« Erreur 502 ») : jamais montrée telle quelle.
  const propre = /^Erreur \d+$/.test(serveur) ? '' : serveur;
  return { ...PAR_CODE[code](propre, admin), code, statut };
}

/** La phrase d'une erreur, pour les endroits qui n'ont qu'une ligne : message puis que faire. */
export function texteErreur(e: unknown, admin = false): string {
  const x = expliquer(e, admin);
  return x.quoiFaire ? `${x.message} ${x.quoiFaire}` : x.message;
}

/** Erreur connue seulement par son code HTTP et sa phrase (page d'erreur de SvelteKit). */
export function expliquerStatut(statut: number, message = '', admin = false): Explication {
  const e = Object.assign(new Error(message), { code: statut });
  return expliquer(e, admin);
}
