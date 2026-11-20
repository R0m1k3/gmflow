import { goto } from '$app/navigation';

export class ErreurApi extends Error {
  constructor(
    public code: number,
    message: string
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
    try {
      const j = await rep.json();
      if (j?.erreur) message = j.erreur;
    } catch {
      /* corps vide */
    }
    throw new ErreurApi(rep.status, message);
  }
  if (rep.status === 204 || rep.status === 202 || rep.headers.get('content-length') === '0') {
    return undefined as T;
  }
  const texte = await rep.text();
  return (texte ? JSON.parse(texte) : undefined) as T;
}

export const api = {
  get: <T>(chemin: string) => appel<T>('GET', chemin),
  post: <T>(chemin: string, corps?: unknown) => appel<T>('POST', chemin, corps),
  put: <T>(chemin: string, corps?: unknown) => appel<T>('PUT', chemin, corps),
  del: <T>(chemin: string) => appel<T>('DELETE', chemin)
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
