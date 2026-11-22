// Ancienne page Dépôts : les envois bloqués sont dans À traiter (section Bloqué), le suivi du mois dans le Bilan.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  const onglet = url.searchParams.get('onglet');
  if (onglet === 'erreur' || onglet === 'doublon') redirect(307, '/#bloque');
  if (onglet === 'a_verifier' || onglet === 'pret') redirect(307, '/');
  const mois = url.searchParams.get('mois');
  redirect(307, mois ? `/bilan?mois=${encodeURIComponent(mois)}` : '/bilan');
};
