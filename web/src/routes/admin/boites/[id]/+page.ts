// Ancienne page d'administration : remplacée par la fiche de la boîte dans Réglages.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
  redirect(308, `/reglages/boites/${params.id}`);
};
