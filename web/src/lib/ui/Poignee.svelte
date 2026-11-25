<script lang="ts">
  // Séparateur glissable entre deux volets : change `largeur` (px) du volet qu'il borde.
  // `cote="gauche"` : le volet est à gauche de la poignée (tirer à droite l'élargit) ;
  // `cote="droite"` : le volet est à droite (tirer à gauche l'élargit).
  // Flèches au clavier (± 24 px), double-clic = largeur d'origine. La largeur choisie est gardée
  // dans localStorage sous `cle` (une par page), lue par `largeurGardee`.
  import { garderLargeur } from '#lib/largeurs.ts';

  let {
    largeur = $bindable(),
    choisie = $bindable(false),
    defaut,
    min,
    max,
    cote,
    cle,
    libelle,
    fiche = false
  }: {
    largeur: number;
    /** Vrai quand une largeur a été choisie (faux après un retour à l'origine). */
    choisie?: boolean;
    defaut: number;
    min: number;
    max: number;
    cote: 'gauche' | 'droite';
    cle: string;
    libelle: string;
    /** Poignée aperçu / fiche : cachée dès que la fiche passe sous l'aperçu (< 1200 px). */
    fiche?: boolean;
  } = $props();

  let glisse = $state(false);

  // Le CSS du volet (max-width) garde de la place à son voisin ; ici, seulement min et max.
  function borner(v: number): number {
    return Math.round(Math.min(max, Math.max(min, v)));
  }
  function poser(v: number) {
    largeur = borner(v);
    choisie = true;
    garderLargeur(cle, largeur);
  }
  function origine() {
    largeur = defaut;
    choisie = false;
    garderLargeur(cle, null);
  }

  function debut(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const x0 = e.clientX;
    const el = e.currentTarget as HTMLElement;
    const l0 = reelle(el);
    const sens = cote === 'gauche' ? 1 : -1;
    el.setPointerCapture(e.pointerId);
    glisse = true;
    const bouge = (ev: PointerEvent) => {
      largeur = borner(l0 + sens * (ev.clientX - x0));
      choisie = true;
    };
    const fin = () => {
      glisse = false;
      el.removeEventListener('pointermove', bouge);
      el.removeEventListener('pointerup', fin);
      el.removeEventListener('pointercancel', fin);
      poser(largeur);
    };
    el.addEventListener('pointermove', bouge);
    el.addEventListener('pointerup', fin);
    el.addEventListener('pointercancel', fin);
  }

  /** Largeur affichée du volet bordé (le CSS peut la réduire sous `largeur` sur écran moyen). */
  function reelle(el: HTMLElement): number {
    const v = cote === 'gauche' ? el.previousElementSibling : el.nextElementSibling;
    return v ? v.getBoundingClientRect().width : largeur;
  }

  function touche(e: KeyboardEvent) {
    const pas = e.shiftKey ? 96 : 24;
    const sens = cote === 'gauche' ? 1 : -1;
    const l0 = reelle(e.currentTarget as HTMLElement);
    if (e.key === 'ArrowRight') poser(l0 + sens * pas);
    else if (e.key === 'ArrowLeft') poser(l0 - sens * pas);
    else if (e.key === 'Home') origine();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<!-- separator focalisable = widget ARIA (WAI-ARIA « window splitter »), Svelte ne le sait pas -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="poignee"
  class:glisse
  class:fiche
  role="separator"
  aria-orientation="vertical"
  aria-label={libelle}
  aria-valuenow={largeur}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  title="Glisser pour élargir ou rétrécir, double-clic pour revenir à la largeur d'origine"
  onpointerdown={debut}
  ondblclick={origine}
  onkeydown={touche}
></div>

{#if glisse}<div class="poignee-voile"></div>{/if}

<style>
  /* 9 px de prise posés à cheval sur la bordure du volet, sans prendre de place */
  .poignee {
    flex: none;
    width: 9px;
    margin: 0 -4px;
    position: relative;
    z-index: 5;
    cursor: col-resize;
    touch-action: none;
  }
  .poignee::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 3px;
    border-radius: 2px;
    background: transparent;
    transition: background 0.12s;
  }
  .poignee:hover::after,
  .poignee:focus-visible::after,
  .poignee.glisse::after {
    background: var(--accent);
  }
  .poignee:focus-visible {
    outline: none;
  }
  /* pendant le glissé, l'aperçu PDF (iframe) ne doit pas capter la souris */
  .poignee-voile {
    position: fixed;
    inset: 0;
    z-index: 100;
    cursor: col-resize;
  }
  @media (max-width: 1200px) {
    .poignee.fiche {
      display: none;
    }
  }
  @media (max-width: 760px) {
    .poignee {
      display: none;
    }
  }
</style>
