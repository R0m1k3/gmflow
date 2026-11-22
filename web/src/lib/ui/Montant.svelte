<script lang="ts" module>
  // Format français à chiffres fixes : « 2 210,11 € », « −312,00 € ».
  // Espaces insécables ordinaires (U+00A0) : resserrés par .num (word-spacing), contrairement à U+202F.
  const fmt = new Intl.NumberFormat('fr-FR', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

  export function formatMontant(v: number | null | undefined, devise = '€'): string {
    if (v === null || v === undefined || Number.isNaN(v)) return '—';
    const s = fmt
      .format(Math.abs(v))
      .replace(/[\u202f\u00a0\s]/g, '\u00a0');
    return `${v < 0 ? '\u2212' : ''}${s}${devise ? ` ${devise}` : ''}`;
  }
</script>

<script lang="ts">
  let {
    valeur,
    devise = '€',
    class: classe = '',
    title,
    avoir = false
  }: {
    valeur: number | null | undefined;
    devise?: string;
    class?: string;
    title?: string;
    /** Montant d'un avoir : toujours affiché en négatif dans les listes (« −312,00 € »). */
    avoir?: boolean;
  } = $props();
  let affiche = $derived(avoir && typeof valeur === 'number' ? -Math.abs(valeur) : valeur);
</script>

<span class="num {classe}" {title}>{formatMontant(affiche, devise)}</span>
