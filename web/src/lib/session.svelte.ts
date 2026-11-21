import type { Moi } from './types';

export const session = $state<{ moi: Moi | null; charge: boolean }>({ moi: null, charge: false });
