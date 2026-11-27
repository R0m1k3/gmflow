// Paramètres de route typés. `[section=section]` (réglages) : seules les sections connues ont
// une vue, une autre adresse sous /reglages donne la page « introuvable ».
import { defineParams } from '@sveltejs/kit/params';
import { estSection } from '#lib/reglages.ts';

export const params = defineParams({
  section: (param) => (estSection(param) ? param : undefined)
});
