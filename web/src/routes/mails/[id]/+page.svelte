<script lang="ts">
  // Fiche mail plein écran (maquette B, écran 3) : tout est dans MailFiche.
  import { afterNavigate } from '$app/navigation';
  import { page } from '$app/state';
  import MailFiche from '#lib/ui/MailFiche.svelte';

  let id = $derived(Number(page.params.id));
  let retour = $state({ href: '/mails', libelle: 'Mails' });

  // Retour vers la page d'où l'on vient (recherche, fiche fournisseur…), gardé quand on enchaîne les mails.
  afterNavigate(({ from }) => {
    const p = from?.url.pathname;
    if (!p || p.startsWith('/mails/')) return;
    const href = from!.url.pathname + from!.url.search + from!.url.hash;
    if (p === '/recherche') retour = { href, libelle: 'Recherche' };
    else if (p.startsWith('/fournisseurs')) retour = { href, libelle: 'Fournisseur' };
    else if (p.startsWith('/documents/')) retour = { href, libelle: 'Document' };
    else if (p === '/bilan') retour = { href, libelle: 'Bilan du mois' };
    else if (p === '/') retour = { href: '/', libelle: 'Accueil' };
    else retour = { href: '/mails', libelle: 'Mails' };
  });
</script>

<div class="pleine fixe">
  {#if Number.isInteger(id) && id > 0}
    <MailFiche {id} {retour} />
  {/if}
</div>
