<script lang="ts">
  // Aperçu d'une pièce jointe : PDF dans une iframe, image en <img>,
  // sinon une phrase et le lien de téléchargement. Source : /api/pieces/{id}?inline=1.
  import Icone from './Icone.svelte';

  let {
    id,
    typeMime,
    nom,
    outils = true
  }: { id: number; typeMime: string | null | undefined; nom?: string | null; outils?: boolean } = $props();

  let src = $derived(`/api/pieces/${id}?inline=1`);
  let telecharger = $derived(`/api/pieces/${id}`);
  let genre = $derived.by(() => {
    const t = (typeMime ?? '').toLowerCase();
    if (t === 'application/pdf') return 'pdf';
    if (/^image\/(png|jpe?g|gif|webp)$/.test(t)) return 'image';
    return 'autre';
  });
  let echec = $state(false);
  $effect(() => {
    void id;
    echec = false;
  });
</script>

<div class="apercu">
  {#if outils}
    <div class="apercu-outils">
      <Icone nom="fichier" />
      <span class="nom" title={nom ?? undefined}>{nom ?? 'Pièce jointe'}</span>
      {#if genre !== 'autre'}
        <a class="btn sm fantome" href={src} target="_blank" rel="noopener"><Icone nom="ouvrir" />Ouvrir</a>
      {/if}
      <a class="btn sm fantome" href={telecharger} download={nom ?? true}><Icone nom="telecharger" />Télécharger</a>
    </div>
  {/if}
  {#if genre === 'pdf' && !echec}
    <div class="apercu-cadre sans-marge">
      <iframe {src} title="Aperçu de {nom ?? 'la pièce'}" onerror={() => (echec = true)}></iframe>
    </div>
  {:else if genre === 'image' && !echec}
    <div class="apercu-cadre">
      <img {src} alt="Aperçu de {nom ?? 'la pièce'}" onerror={() => (echec = true)} />
    </div>
  {:else}
    <div class="vide">
      <Icone nom="fichier" />
      <div class="vide-titre">{echec ? 'Aperçu indisponible' : 'Pas d’aperçu pour ce fichier'}</div>
      <p class="vide-texte">{nom ?? 'La pièce'} s’ouvre dans le logiciel de l’ordinateur.</p>
      <div class="vide-actions"><a class="btn sm" href={telecharger} download={nom ?? true}><Icone nom="telecharger" />Télécharger</a></div>
    </div>
  {/if}
</div>
