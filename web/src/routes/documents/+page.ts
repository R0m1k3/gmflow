// Ancienne liste des documents : remplacée par la recherche unique (l'accueil pour ce qui coince, les listes filtrées pour ce qui attend).
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

const TYPES = ['facture', 'avoir', 'bl'];

export const load: PageLoad = ({ url }) => {
  const p = url.searchParams;
  // Documents d'un fournisseur : sa fiche.
  const fournisseur = p.get('fournisseur');
  if (fournisseur && /^\d+$/.test(fournisseur)) redirect(307, `/fournisseurs/${fournisseur}`);
  // Ce qui coince : l'accueil ; ce qui attend : la liste de son type, filtre « À valider ».
  const statut = p.get('statut_depot');
  if (statut === 'erreur' || statut === 'doublon' || p.get('etat') === 'erreur') redirect(307, '/#bloque');
  if (statut === 'a_verifier' || statut === 'pret' || p.get('verifie') === 'false') {
    const t = p.get('type');
    redirect(307, `${t === 'bl' ? '/bl' : t === 'avoir' ? '/avoirs' : '/factures'}?filtre=a-valider`);
  }

  const garde = new URLSearchParams();
  const type = p.get('type') ?? '';
  garde.set('type', TYPES.includes(type) ? type : 'document');
  for (const k of ['q', 'du', 'au']) {
    const v = p.get(k);
    if (v) garde.set(k, v);
  }
  redirect(307, `/recherche?${garde}`);
};
