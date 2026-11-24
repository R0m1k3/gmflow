<script lang="ts">
  // Boîtes mail : liste à filets ; ajout par l'assistant, réglages dans la fiche de la boîte.
    import { RUBRIQUES, nomCourtSociete, type Ton } from '#lib/libelles.ts';
  import type { Boite, Societe } from '#lib/types.ts';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Vide from '#lib/ui/Vide.svelte';
  import { leA } from './outils.ts';

  let { boites, societes }: { boites: Boite[]; societes: Societe[] } = $props();

  const nomSociete = (id: number) => nomCourtSociete(societes.find((s) => s.id === id)?.raison_sociale) || 'Société inconnue';
  let possible = $derived(societes.some((s) => !s.archivee));

  function etat(b: Boite): { texte: string; ton: Ton } {
    if (b.pause_auto) return { texte: 'Arrêtée : connexion refusée', ton: 'erreur' };
    if (!b.active) return b.dernier_test?.ok ? { texte: 'En pause', ton: 'pale' } : { texte: 'Pas encore démarrée', ton: 'alerte' };
    if (b.erreur_depuis) return { texte: 'En erreur', ton: 'alerte' };
    if (!b.derniere_releve) return { texte: 'Première relève en cours', ton: 'info' };
    return { texte: 'Surveillée', ton: 'ok' };
  }
</script>

<section class="sujet" id="boites">
  <div class="sujet-h">
    <h2>{RUBRIQUES.boites}</h2>
    <span class="sp"></span>
    {#if possible}
      <a class="btn sm" href="/reglages/boites/nouvelle"><Icone nom="plus" />Ajouter une boîte</a>
    {/if}
  </div>
  <p class="lead">
    GMFlow y cherche les nouveaux mails, lit les factures jointes et propose les réponses.
    {#if !possible}<br /><a href="#societes">Ajoutez d’abord une société</a> : chaque boîte en dépend.{/if}
  </p>

  {#if boites.length}
    <div class="carte-r">
      {#each boites as b (b.id)}
        {@const e = etat(b)}
        <a class="rang" href="/reglages/boites/{b.id}">
          <div class="g">
            <span class="t">{b.libelle}</span>
            <span class="s">{nomSociete(b.societe_id)}{' '}· {b.identifiant}</span>
            {#if b.pause_auto || b.derniere_erreur}
              <span class="alerte-l" class:ko={b.pause_auto}><Icone nom="alerte" />{b.derniere_erreur ?? 'Mot de passe refusé'}</span>
            {/if}
          </div>
          <div class="d">
            <Etiquette ton={e.ton}>{#if e.ton === 'ok'}<Icone nom="coche" />{/if}{e.texte}</Etiquette>
            {#if b.derniere_releve}<span class="m">vue le {leA(b.derniere_releve, true)}</span>{/if}
          </div>
        </a>
      {/each}
    </div>
  {:else}
    <Vide compact icone="boite" titre="Aucune boîte mail" texte="Ajoutez l’adresse où arrivent les factures : moins de 3 minutes avec un mot de passe d’application.">
      {#if possible}<a class="btn sm" href="/reglages/boites/nouvelle">Ajouter une boîte</a>{/if}
    </Vide>
  {/if}
</section>

<style>
  .alerte-l.ko {
    color: var(--err-ink);
  }
</style>
