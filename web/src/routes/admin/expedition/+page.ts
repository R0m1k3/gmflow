// Ancienne page d'administration : remplacée par Réglages, Envoi vers Dext / Yooz.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = () => {
  redirect(308, '/reglages#envoi');
};
