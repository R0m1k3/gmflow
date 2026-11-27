// Ancienne page d'administration : remplacée par Réglages, Sociétés.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = () => {
  redirect(308, '/reglages/societes');
};
