<script lang="ts" module>
  // État d'un fournisseur : « 3 factures ce mois », puis les étiquettes ambre « 1 à contrôler »,
  // « 1 BL sans facture ». `quoi` choisit la partie affichée (tuile : les deux sur deux lignes).
  import type { EtatFournisseur } from '#lib/types.ts';

  /** Un morceau de l'état : compteur (mono) puis texte ; `alerte` = à faire (ambre). */
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
    if (e.a_valider) m.push({ n: e.a_valider, texte: 'à contrôler', alerte: true });
    if (e.bl_sans_facture) m.push({ n: e.bl_sans_facture, texte: 'BL sans facture', alerte: true });
    return m;
  }

  /** Même ligne en texte simple (titre, lecteur d'écran). */
  export function texteEtat(e: EtatFournisseur): string {
    return morceauxEtat(e)
      .map((x) => (x.n === null ? x.texte : `${x.n} ${x.texte}`))
      .join(' · ');
  }

  /** Le fournisseur a quelque chose à faire (étiquettes ambre). */
  export const aDesAlertes = (e: EtatFournisseur) => e.a_valider > 0 || e.bl_sans_facture > 0;
</script>

<script lang="ts">
  let { etat, quoi = 'tout' }: { etat: EtatFournisseur; quoi?: 'texte' | 'etiquettes' | 'tout' } = $props();
  let morceaux = $derived(morceauxEtat(etat));
</script>

{#if quoi !== 'etiquettes'}
  {@const t = morceaux[0]}
  <span class="texte-etat">{#if t.n !== null}<span class="num">{t.n}</span>{' '}{/if}{t.texte}</span>
{/if}
{#if quoi !== 'texte'}
  {#each morceaux.filter((m) => m.alerte) as m, i (i)}
    <span class="etiquette att"><span class="num">{m.n}</span>{' '}{m.texte}</span>
  {/each}
{/if}
