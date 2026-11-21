<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import { quand, taille } from '#lib/format.ts';
  import type { Tableau } from '#lib/types.ts';

  let t = $state<Tableau | null>(null);
  let erreur = $state('');
  $effect(() => {
    api.get<Tableau>('/api/tableau-de-bord').then((v) => (t = v)).catch((e) => (erreur = message(e)));
  });
  const libelleClaude: Record<string, string> = { valide: 'valide', erreur: 'en erreur', absent: 'absent', en_test: 'en cours de test' };
</script>

<h1>Tableau de bord</h1>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}
{#if t}
  {#each t.boites_alerte as b (b.id)}
    <div class="bandeau {b.motif === 'pause_auto' ? 'erreur' : 'attention'}">
      <strong>{b.libelle}</strong> :
      {b.motif === 'pause_auto' ? 'en pause automatique après 3 refus de connexion' : 'relève en échec'}
      {#if b.depuis} depuis {quand(b.depuis)}{/if}{#if b.message} ({b.message}){/if}.
      {#if t.claude}<a href="/admin/boites/{b.id}">Voir la boîte</a>{/if}
    </div>
  {/each}

  <div class="grille">
    {#each t.mails_du_jour as s (s.societe_id)}
      <a class="carte stat" href="/mails?societe={s.societe_id}">
        <div class="discret">{s.raison_sociale}</div>
        <div class="valeur nombre">{s.n}</div>
        <div class="discret">mail{s.n > 1 ? 's' : ''} aujourd'hui</div>
      </a>
    {/each}
    {#if t.claude}
      <a class="carte stat" href="/admin/claude">
        <div class="discret">Jeton Claude</div>
        <div class="valeur">
          <span class="etiquette {t.claude.etat === 'valide' ? 'ok' : t.claude.etat === 'erreur' ? 'ko' : 'att'}">{libelleClaude[t.claude.etat] ?? t.claude.etat}</span>
        </div>
        <div class="discret">testé {quand(t.claude.teste_le)}</div>
      </a>
    {/if}
    {#if t.stockage}
      <div class="carte stat">
        <div class="discret">Stockage</div>
        <div class="valeur nombre">{taille(t.stockage.octets)}</div>
        <div class="discret" class:rouge={t.stockage.libre_pourcent !== null && t.stockage.libre_pourcent < 10}>
          {t.stockage.libre_pourcent === null ? 'place libre inconnue' : `${t.stockage.libre_pourcent.toLocaleString('fr-FR')} % libres`}
        </div>
      </div>
    {/if}
  </div>
  {#if t.mails_du_jour.length === 0}
    <p class="discret">Aucune structure. {#if t.claude}<a href="/admin/structures">Créer la première.</a>{/if}</p>
  {/if}
{/if}

<style>
  .stat { text-decoration: none; color: inherit; display: grid; gap: 0.2rem; }
  a.stat:hover { border-color: var(--accent); }
  .valeur { font-size: 1.7rem; font-weight: 650; }
  .rouge { color: var(--alerte); font-weight: 600; }
</style>
