// Ancienne file « À répondre » : c'est une section de À traiter.
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = () => {
  redirect(307, '/#a-repondre');
};
