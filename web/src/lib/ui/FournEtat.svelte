<script lang="ts" module>
  // Ligne d'état d'un fournisseur : « 3 factures ce mois · 1 à valider · 1 BL sans facture ».
  import type { EtatFournisseur } from '#lib/types.ts';

  /** Un morceau de la ligne : compteur (mono) puis texte ; `alerte` = à faire (ambre). */
  export interface MorceauEtat {
    n: number | null;
    texte: string;
    alerte: boolean;
  }

  export function morceauxEtat(e: EtatFournisseur): MorceauEtat[] {
    const m: MorceauEtat[] = [
      e.factures_mois
        ? { n: e.factures_mois, texte: `${e.factures_mois > 1 ? 'factures' : 'facture'} ce mois`, alerte: false }
        : { n: null, texte: 'Aucune facture ce mois', alerte: false }
    ];
    if (e.a_valider) m.push({ n: e.a_valider, texte: 'à valider', alerte: true });
    if (e.bl_sans_facture) m.push({ n: e.bl_sans_facture, texte: 'BL sans facture', alerte: true });
    return m;
  }

  /** Même ligne en texte simple (titre, lecteur d'écran). */
  export function texteEtat(e: EtatFournisseur): string {
    return morceauxEtat(e)
      .map((x) => (x.n === null ? x.texte : `${x.n} ${x.texte}`))
      .join(' · ');
  }
</script>

<script lang="ts">
  let { etat }: { etat: EtatFournisseur } = $props();
</script>

{#each morceauxEtat(etat) as m, i (i)}{#if i > 0}<span class="muted">{' · '}</span>{/if}<span class:flag={m.alerte} class:w={m.alerte}
    >{#if m.n !== null}<span class="num">{m.n}</span>{' '}{/if}{m.texte}</span
  >{/each}
