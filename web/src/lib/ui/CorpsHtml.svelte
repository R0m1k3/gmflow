<script lang="ts">
  /**
   * Corps HTML d'origine d'un mail, comme dans un lecteur de mails. La page vient de
   * `GET /api/mails/{id}/html` (CSP sans script) et s'affiche dans un iframe `sandbox` sans
   * `allow-scripts` : aucun code du mail ne s'exécute. `allow-same-origin` sert seulement à lire
   * la hauteur du contenu pour agrandir le cadre ; les liens s'ouvrent dans un nouvel onglet.
   */
  let { id }: { id: number } = $props();

  let cadre = $state<HTMLIFrameElement | null>(null);
  let hauteur = $state(160);
  let observateur: ResizeObserver | null = null;

  /** Ajuste le cadre : un mail plus large que la place (tableau de 600 px sur téléphone) est
   *  réduit pour tenir en largeur, comme dans un lecteur de mails, puis la hauteur suit. */
  function ajuster() {
    const doc = cadre?.contentDocument;
    const racine = doc?.documentElement;
    if (!cadre || !racine) return;
    racine.style.zoom = '';
    const place = cadre.clientWidth;
    const largeur = racine.scrollWidth;
    const echelle = largeur > place && place > 0 ? Math.max(0.35, place / largeur) : 1;
    if (echelle < 1) racine.style.zoom = String(echelle);
    hauteur = Math.max(80, Math.ceil(racine.getBoundingClientRect().height)) + 2;
  }

  function charge() {
    observateur?.disconnect();
    const doc = cadre?.contentDocument;
    if (!cadre || !doc?.body) return;
    ajuster();
    // Largeur du volet qui change, puis images distantes arrivées après le chargement.
    observateur = new ResizeObserver(ajuster);
    observateur.observe(cadre);
    for (const img of Array.from(doc.images)) if (!img.complete) img.addEventListener('load', ajuster);
  }

  $effect(() => () => observateur?.disconnect());
</script>

<iframe
  bind:this={cadre}
  class="corps-html"
  title="Contenu du mail"
  src="/api/mails/{id}/html"
  sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
  referrerpolicy="no-referrer"
  style:height="{hauteur}px"
  onload={charge}
></iframe>

<style>
  .corps-html {
    display: block;
    flex: none;
    width: 100%;
    border: 1px solid var(--line-faint);
    border-radius: 10px;
    background: #fff;
  }
</style>
