import { goto } from '$app/navigation';
import type {
  BilanMois,
  Boite,
  ChangementsFournisseur,
  CodeRefus,
  CompteursATraiter,
  DemandeEssaiBoite,
  DemandeValidation,
  EligibiliteLot,
  LignesDocument,
  ResultatLigne,
  SaisieLigne,
  EntreeBoite,
  EssaiBoite,
  FileATraiter,
  FilFournisseur,
  Fournisseur,
  LigneFournisseur,
  MiseEnRoute,
  MotifEcartement,
  Page,
  ResultatAnnulation,
  ResultatLot,
  ResultatValidation,
  ResultatsRecherche
} from './types';

export class ErreurApi extends Error {
  constructor(
    public code: number,
    message: string,
    /** Code stable de certains refus (409), par exemple `doublon` : voir docs/api-refonte.md. */
    public raison?: CodeRefus,
    /** Champ en faute d'une donnée refusée (400), par exemple `siret` : le message va sous ce champ. */
    public champ?: string
  ) {
    super(message);
  }
}

async function appel<T>(methode: string, chemin: string, corps?: unknown): Promise<T> {
  const entetes: Record<string, string> = {};
  if (methode !== 'GET') entetes['X-GMFlow'] = '1';
  if (corps !== undefined) entetes['Content-Type'] = 'application/json';
  const rep = await fetch(chemin, {
    method: methode,
    credentials: 'same-origin',
    headers: entetes,
    body: corps === undefined ? undefined : JSON.stringify(corps)
  });
  if (rep.status === 401 && !chemin.startsWith('/api/connexion')) {
    await goto('/connexion');
    throw new ErreurApi(401, 'Session expirée, reconnectez-vous.');
  }
  if (!rep.ok) {
    let message = `Erreur ${rep.status}`;
    let raison: CodeRefus | undefined;
    let champ: string | undefined;
    try {
      const j = await rep.json();
      if (j?.erreur) message = j.erreur;
      if (typeof j?.code === 'string') raison = j.code;
      if (typeof j?.champ === 'string') champ = j.champ;
    } catch {
      /* corps vide */
    }
    throw new ErreurApi(rep.status, message, raison, champ);
  }
  // 202 sans corps (tâche lancée) : rien ; 202 avec corps (essai de boîte) : son JSON.
  if (rep.status === 204 || rep.headers.get('content-length') === '0') {
    return undefined as T;
  }
  const texte = await rep.text();
  return (texte ? JSON.parse(texte) : undefined) as T;
}

export const api = {
  get: <T>(chemin: string) => appel<T>('GET', chemin),
  post: <T>(chemin: string, corps?: unknown) => appel<T>('POST', chemin, corps),
  put: <T>(chemin: string, corps?: unknown) => appel<T>('PUT', chemin, corps),
  patch: <T>(chemin: string, corps?: unknown) => appel<T>('PATCH', chemin, corps),
  del: <T>(chemin: string, corps?: unknown) => appel<T>('DELETE', chemin, corps)
};

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Relance `f` toutes les `ms` tant que la page est ouverte ; rend la fonction d'arrêt. */
export function sonder(f: () => Promise<boolean | void>, ms = 2000): () => void {
  let actif = true;
  const tour = async () => {
    if (!actif) return;
    let continuer: boolean | void = true;
    try {
      continuer = await f();
    } catch {
      /* on réessaie au tour suivant */
    }
    if (actif && continuer !== false) setTimeout(tour, ms);
  };
  setTimeout(tour, ms);
  return () => {
    actif = false;
  };
}

// ---------------------------------------------------------------------------------------------
// Refonte de l'interface (contrat : docs/api-refonte.md).

type Parametres = Record<string, string | number | boolean | null | undefined>;

/** `?a=1&b=x` sans les valeurs vides ; chaîne vide s'il n'y a rien. */
export function requete(p: Parametres): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(p)) {
    if (v !== undefined && v !== null && v !== '') u.set(k, String(v));
  }
  const t = u.toString();
  return t ? `?${t}` : '';
}

/** Adresse de l'aperçu d'une pièce (PDF ou image affichés dans la page ; sinon téléchargée). */
export const urlApercu = (pieceId: number) => `/api/pieces/${pieceId}?inline=1`;

export const aTraiter = {
  file: (societe?: number | null) => api.get<FileATraiter>(`/api/a-traiter${requete({ societe })}`),
  compteurs: (societe?: number | null) =>
    api.get<CompteursATraiter>(`/api/a-traiter/compteurs${requete({ societe })}`)
};

/** « Valider » (et envoyer à Dext/Yooz une facture ou un avoir) ; rend le document suivant. */
export const validerDocument = (id: number, demande: DemandeValidation = {}, societe?: number | null) =>
  api.post<ResultatValidation>(`/api/documents/${id}/valider${requete({ societe })}`, demande);

/** « Écarter » : un doublon (gardé comme doublon), ou tout document lu ou illisible (« Écarté »), avec un motif facultatif. */
export const ecarterDocument = (id: number, societe?: number | null, motif?: { motif?: MotifEcartement | null; texte?: string | null }) =>
  api.post<ResultatValidation>(`/api/documents/${id}/ecarter${requete({ societe })}`, motif ?? {});

/** « Remettre à contrôler » une pièce écartée : elle retrouve son état d'avant. */
export const remettreDocument = (id: number) => api.post<{ id: number; etat: string; statut_depot: string | null }>(`/api/documents/${id}/remettre`);

/** « Annuler » : les envois d'une validation (ou d'un lot) pas encore partis sont annulés. */
export const annulerEnvoi = (lot: string) => api.post<ResultatAnnulation>('/api/documents/annuler-envoi', { lot });

/** Pièces cochables pour une validation par lot (100 au plus). */
export const eligibiliteLot = (ids: number[]) => api.post<EligibiliteLot[]>('/api/documents/lot/eligibilite', { ids });

/** « Valider N pièces » : chaque pièce sans alerte est validée, les autres laissées à contrôler. */
export const validerLot = (ids: number[]) => api.post<ResultatLot>('/api/documents/lot/valider', { ids });

/** Lignes articles d'un document, pour les corriger à la main. */
export const lignesDocument = (id: number) => api.get<LignesDocument>(`/api/documents/${id}/lignes`);
export const modifierLigne = (id: number, ligne: number, s: SaisieLigne) => api.put<ResultatLigne>(`/api/documents/${id}/lignes/${ligne}`, s);
export const ajouterLigne = (id: number, s: SaisieLigne) => api.post<ResultatLigne>(`/api/documents/${id}/lignes`, s);
export const supprimerLigne = (id: number, ligne: number) => api.del<ResultatLigne>(`/api/documents/${id}/lignes/${ligne}`);

export const fournisseurs = (p: { societe?: number | null; q?: string; statut?: string; page?: number } = {}) =>
  api.get<Page<LigneFournisseur>>(`/api/fournisseurs${requete(p)}`);

export const filFournisseur = (id: number, p: { societe?: number | null; mois?: number } = {}) =>
  api.get<FilFournisseur>(`/api/fournisseurs/${id}/fil${requete(p)}`);

export const rechercher = (p: {
  q?: string;
  /** `document` : tous les documents, sans les mails. */
  type?: 'facture' | 'avoir' | 'bl' | 'document' | 'mail' | '';
  du?: string;
  au?: string;
  societe?: number | null;
}) => api.get<ResultatsRecherche>(`/api/recherche${requete(p)}`);

export const bilanMois = (p: { mois?: string; societe?: number | null } = {}) =>
  api.get<BilanMois>(`/api/bilan${requete(p)}`);

/** Modification partielle d'une fiche fournisseur (admin) : « Confirmer », « Toujours ranger dans X ». */
export const modifierFournisseur = (id: number, changements: ChangementsFournisseur) =>
  api.patch<Fournisseur>(`/api/admin/fournisseurs/${id}`, changements);

export const lireBoite = (id: number) => api.get<Boite>(`/api/admin/boites/${id}`);

/** Création d'une boîte ; avec `essai_id` (essai réussi) et `activer`, elle est surveillée tout de suite. */
export const creerBoite = (entree: EntreeBoite) => api.post<Boite>('/api/admin/boites', entree);

export const miseEnRoute = () => api.get<MiseEnRoute>('/api/admin/mise-en-route');

export const essayerBoite = (demande: DemandeEssaiBoite) =>
  api.post<{ id: string }>('/api/admin/boites/tester', demande);

export const essayerSmtp = (boiteId: number) =>
  api.post<{ id: string }>(`/api/admin/boites/${boiteId}/tester-smtp`);

export const lireEssai = (id: string) => api.get<EssaiBoite>(`/api/admin/boites/essais/${id}`);

/** Relit un essai jusqu'à ce qu'il soit terminé (le worker répond en 1 à 30 s). */
export async function attendreEssai(id: string, ms = 1500, max = 40): Promise<EssaiBoite> {
  for (let i = 0; i < max; i++) {
    const e = await lireEssai(id);
    if (e.etat === 'termine') return e;
    await new Promise((r) => setTimeout(r, ms));
  }
  throw new ErreurApi(504, "L'essai ne répond pas. Le traitement automatique (worker) est-il démarré ?");
}
