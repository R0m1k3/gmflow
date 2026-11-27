<script lang="ts">
  /**
   * Corps HTML d'origine d'un mail, comme dans un lecteur de mails. La page vient de
   * `GET /api/mails/{id}/html` (CSP sans script) et s'affiche dans un iframe `sandbox` sans
   * `allow-scripts` : aucun code du mail ne s'exécute. `allow-same-origin` sert seulement à lire
   * la hauteur du contenu pour agrandir le cadre ; les liens s'ouvrent dans un nouvel onglet.
   *
   * Images distantes (C23) : bloquées par la CSP du serveur (pixel espion). S'il y en a, un
   * bandeau discret propose « Afficher » (ce mail, cette fois) ou « Toujours pour cet
   * expéditeur » (retenu dans ce navigateur) ; la page est alors rechargée avec `?images=1`.
   */
  import {
    adresseCorps,
    adresseDe,
    contientImagesDistantes,
    expediteursAutorises,
    memoriser
  } from './imagesDistantes.ts';

  let { id, expediteur = '' }: { id: number; expediteur?: string } = $props();

  function stockage(): Storage | null {
    try {
      return typeof localStorage === 'undefined' ? null : localStorage;
    } catch {
      return null;
    }
  }

  let autorises = $state(expediteursAutorises(stockage()));
  /** Mail dont l'utilisateur a demandé les images, cette fois. */
  let afficheIci = $state<number | null>(null);
  let afficher = $derived(afficheIci === id || autorises.includes(adresseDe(expediteur)));
  /** Le mail affiché sans ses images en contient (vu au chargement). */
  let masquees = $state(false);

  function afficherUneFois() {
    afficheIci = id;
  }

  function afficherToujours() {
    memoriser(expediteur, stockage());
    autorises = expediteursAutorises(stockage());
    afficheIci = id;
  }

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
    masquees = !afficher && contientImagesDistantes(doc.documentElement.outerHTML);
    ajuster();
    // Largeur du volet qui change, puis images distantes arrivées après le chargement.
    observateur = new ResizeObserver(ajuster);
    observateur.observe(cadre);
    for (const img of Array.from(doc.images)) if (!img.complete) img.addEventListener('load', ajuster);
  }

  $effect(() => () => observateur?.disconnect());
</script>

{#if masquees && !afficher}
  <p class="images-masquees" role="status">
    Images distantes masquées
    <span aria-hidden="true">·</span>
    <button type="button" class="btn-lien" onclick={afficherUneFois}>Afficher</button>
    {#if adresseDe(expediteur)}
      <span aria-hidden="true">·</span>
      <button type="button" class="btn-lien" onclick={afficherToujours}>Toujours pour cet expéditeur</button>
    {/if}
  </p>
{/if}
<iframe
  bind:this={cadre}
  class="corps-html"
  title="Contenu du mail"
  src={adresseCorps(id, afficher)}
  sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
  referrerpolicy="no-referrer"
  style:height="{hauteur}px"
  onload={charge}
></iframe>

<style>
  .images-masquees {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--e1) var(--e2);
    margin: 0 0 var(--e2);
    font-size: 16px;
    color: var(--muted);
  }
  .corps-html {
    display: block;
    flex: none;
    width: 100%;
    border: 1px solid var(--line-faint);
    border-radius: 10px;
    background: #fff;
  }
</style>
