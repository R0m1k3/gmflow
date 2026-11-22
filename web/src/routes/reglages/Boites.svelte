<script lang="ts">
  // Boîtes mail : liste à filets ; ajout par l'assistant, réglages dans la fiche de la boîte.
  import { quand } from '#lib/format.ts';
  import { RUBRIQUES, nomCourtSociete, type Ton } from '#lib/libelles.ts';
  import type { Boite, Societe } from '#lib/types.ts';
  import Etiquette from '#lib/ui/Etiquette.svelte';
  import Icone from '#lib/ui/Icone.svelte';
  import Vide from '#lib/ui/Vide.svelte';

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
    GMFlow y cherche les nouveaux mails toutes les 5 minutes, lit les factures jointes et propose les réponses.
    {#if !possible}<br /><a href="#societes">Ajoutez d’abord une société</a> : chaque boîte en dépend.{/if}
  </p>

  <div class="filets">
    {#each boites as b (b.id)}
      {@const e = etat(b)}
      <a class="ligne" href="/reglages/boites/{b.id}">
        <div class="a">{b.libelle}</div>
        <div class="r"><Etiquette ton={e.ton}>{e.texte}</Etiquette></div>
        <div class="b">
          {nomSociete(b.societe_id)}{#if b.libelle !== b.identifiant}{' '}· {b.identifiant}{/if}
          {#if b.pause_auto || b.derniere_erreur}{' '}· <span class="flag {b.pause_auto ? 'e' : 'w'}">{b.derniere_erreur ?? 'Mot de passe refusé'}</span>{/if}
        </div>
        <div class="r2">{#if b.derniere_releve}<span class="num">{quand(b.derniere_releve)}</span>{/if}</div>
      </a>
    {:else}
      <Vide compact icone="boite" titre="Aucune boîte mail" texte="Ajoutez l’adresse où arrivent les factures : moins de 3 minutes avec un mot de passe d’application.">
        {#if possible}<a class="btn sm" href="/reglages/boites/nouvelle">Ajouter une boîte</a>{/if}
      </Vide>
    {/each}
  </div>
</section>
