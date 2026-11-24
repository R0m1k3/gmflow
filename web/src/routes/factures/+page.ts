// Les avoirs ont leur onglet : l'ancien filtre « Avoirs » des factures y mène.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  if (url.searchParams.get('filtre') !== 'avoirs') return;
  const doc = url.searchParams.get('doc');
  redirect(307, doc ? `/avoirs?doc=${encodeURIComponent(doc)}` : '/avoirs');
};
