// Ancienne page Rapprochement : chaque fiche fournisseur a son encadré « À rapprocher ».
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  const f = url.searchParams.get('fournisseur');
  redirect(307, f && /^\d+$/.test(f) ? `/fournisseurs?fiche=${f}` : '/fournisseurs');
};
