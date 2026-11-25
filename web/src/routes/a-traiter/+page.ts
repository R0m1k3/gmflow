// Ancienne page « À valider » (retirée le 2026-11-25) : les pièces s'ouvrent dans leur liste
// (Factures, Avoirs, BL, filtre « À valider », badges du menu), les mails dans À répondre,
// les blocages sur l'accueil. Les anciens liens y mènent.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
  const doc = url.searchParams.get('doc');
  if (doc && /^\d+$/.test(doc)) redirect(307, `/documents/${doc}`);
  const mail = url.searchParams.get('mail');
  if (mail && /^\d+$/.test(mail)) redirect(307, `/a-repondre?mail=${mail}`);
  redirect(307, '/');
};
