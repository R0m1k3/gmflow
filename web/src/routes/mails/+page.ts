// Ancienne liste des mails : remplacée par la recherche unique (requête vide = derniers mails reçus).
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  const garde = new URLSearchParams();
  for (const k of ['q', 'du', 'au']) {
    const v = url.searchParams.get(k);
    if (v) garde.set(k, v);
  }
  // Un filtre de catégorie (lien du tri par semaine) : les mails seulement.
  if (url.searchParams.get('categorie')) garde.set('type', 'mail');
  const q = garde.toString();
  redirect(307, `/recherche${q ? `?${q}` : ''}`);
};
