<script lang="ts">
  import { goto } from '$app/navigation';
  import { api, message } from '#lib/api.ts';
  import { quand } from '#lib/format.ts';
  import type { Boite, Societe } from '#lib/types.ts';

  let boites = $state<Boite[]>([]);
  let societes = $state<Societe[]>([]);
  let nouvelle = $state<{ societe_id: string; libelle: string; serveur: string; port: number; identifiant: string; mot_de_passe: string; reprise_mois: number } | null>(null);
  let erreur = $state('');
  let chargement = $state(true);

  async function charger() {
    try {
      [boites, societes] = await Promise.all([api.get<Boite[]>('/api/admin/boites'), api.get<Societe[]>('/api/admin/societes')]);
    } finally {
      chargement = false;
    }
  }
  $effect(() => {
    charger().catch((e) => (erreur = message(e)));
  });
  const nomSociete = (id: number) => societes.find((s) => s.id === id)?.raison_sociale ?? '?';

  async function creer(e: SubmitEvent) {
    e.preventDefault();
    if (!nouvelle) return;
    erreur = '';
    try {
      const b = await api.post<Boite>('/api/admin/boites', { ...nouvelle, societe_id: Number(nouvelle.societe_id), dossiers: ['INBOX'] });
      await goto(`/admin/boites/${b.id}`);
    } catch (err) {
      erreur = message(err);
    }
  }

  function etat(b: Boite): [string, string, string] {
    if (b.pause_auto) return ['En pause (refus de connexion)', 'ko', 'Vérifiez le mot de passe puis réactivez la boîte.'];
    if (!b.active) return ['Inactive', '', 'Testez la connexion puis activez la boîte.'];
    if (b.erreur_depuis) return ['En erreur', 'att', 'La relève a échoué : ouvrez la fiche pour voir pourquoi.'];
    return ['Connectée', 'ok', ''];
  }
</script>
<div class="entete">
  <div>
    <h1>Boîtes mail</h1>
    <p class="sous">Les boîtes que GMFlow relève toutes les 5 minutes pour y trouver factures, bons de livraison et questions.</p>
  </div>
  {#if !nouvelle && societes.length > 0}
    <div class="actions"><button onclick={() => (nouvelle = { societe_id: '', libelle: '', serveur: '', port: 993, identifiant: '', mot_de_passe: '', reprise_mois: 3 })}>Ajouter une boîte mail</button></div>
  {/if}
</div>
{#if erreur}<div class="bandeau erreur">{erreur}</div>{/if}

{#if nouvelle}
  <form class="pile" onsubmit={creer}>
    <h2 class="sans-marge">Nouvelle boîte mail</h2>
    <div class="bandeau info sans-marge">
      GMFlow se connecte à la boîte comme un logiciel de messagerie (IMAPS, port 993). Pour Gmail et Infomaniak, créez un
      <strong>mot de passe d'application</strong> dans les paramètres de sécurité du compte : c'est plus sûr que le mot de passe habituel.
      Rien n'est supprimé ni déplacé dans la boîte.
    </div>

    <fieldset>
      <legend>Quelle boîte</legend>
      <div class="champs">
        <div>
          <label for="s">Société</label>
          <select id="s" bind:value={nouvelle.societe_id} required>
            <option value="" disabled>Choisir…</option>
            {#each societes.filter((s) => !s.archivee) as s (s.id)}<option value={String(s.id)}>{s.raison_sociale}</option>{/each}
          </select>
          <p class="aide">La société à laquelle appartiennent les mails de cette boîte.</p>
        </div>
        <div>
          <label for="l">Nom de la boîte</label>
          <input id="l" bind:value={nouvelle.libelle} required placeholder="Compta Frouard" />
          <p class="aide">Un nom pour la reconnaître dans GMFlow.</p>
        </div>
      </div>
    </fieldset>

    <fieldset>
      <legend>Connexion</legend>
      <div class="champs">
        <div>
          <label for="srv">Serveur de réception (IMAP)</label>
          <input id="srv" bind:value={nouvelle.serveur} required placeholder="imap.gmail.com" />
          <p class="aide">Gmail : imap.gmail.com. Infomaniak : mail.infomaniak.com.</p>
        </div>
        <div>
          <label for="p">Port</label>
          <input id="p" type="number" bind:value={nouvelle.port} />
          <p class="aide">993 dans presque tous les cas (connexion sécurisée).</p>
        </div>
        <div>
          <label for="i">Identifiant</label>
          <input id="i" bind:value={nouvelle.identifiant} required autocomplete="off" placeholder="compta@exemple.fr" />
          <p class="aide">Presque toujours l'adresse mail complète.</p>
        </div>
        <div>
          <label for="m">Mot de passe d'application</label>
          <input id="m" type="password" bind:value={nouvelle.mot_de_passe} required autocomplete="new-password" />
          <p class="aide">Il est chiffré et n'est plus jamais affiché ensuite.</p>
        </div>
      </div>
    </fieldset>

    <fieldset>
      <legend>Historique</legend>
      <div class="champs">
        <div>
          <label for="r">Reprise de l'historique (mois)</label>
          <input id="r" type="number" min="0" max="24" bind:value={nouvelle.reprise_mois} />
          <p class="aide">Combien de mois de mails déjà reçus GMFlow doit lire au démarrage. 3 mois convient pour commencer, 0 pour ne suivre que les nouveaux mails.</p>
        </div>
      </div>
    </fieldset>

    <div class="actions"><button>Créer puis tester</button><button type="button" class="second" onclick={() => (nouvelle = null)}>Annuler</button></div>
  </form>
{:else if chargement}
  <div class="charge" aria-busy="true"><span></span></div>
{:else if societes.length === 0}
  <div class="vide">
    <strong>Aucune société pour l'instant</strong>
    <p>Une boîte mail appartient toujours à une société : créez-en une d'abord.</p>
    <div class="actions"><a class="bouton" href="/admin/structures">Créer une société</a></div>
  </div>
{:else if boites.length === 0}
  <div class="vide">
    <strong>Aucune boîte mail</strong>
    <p>Ajoutez la première boîte pour que GMFlow commence à lire les factures reçues.</p>
    <div class="actions"><button onclick={() => (nouvelle = { societe_id: '', libelle: '', serveur: '', port: 993, identifiant: '', mot_de_passe: '', reprise_mois: 3 })}>Ajouter une boîte mail</button></div>
  </div>
{:else}
  <ul class="liste">
    {#each boites as b (b.id)}
      {@const [texte, classe, suite] = etat(b)}
      <li>
        <div class="principal">
          <a href="/admin/boites/{b.id}">{b.libelle}</a>
          <span class="discret">{nomSociete(b.societe_id)} · {b.identifiant}</span>
          <span class="discret">
            Dernière relève {quand(b.derniere_releve)} · Dossiers : {b.dossiers.join(', ')}{suite ? ` · ${suite}` : ''}
          </span>
        </div>
        <div class="droite">
          <span class="etiquette {classe}">{texte}</span>
          <a class="bouton second" href="/admin/boites/{b.id}">Ouvrir</a>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .sans-marge { margin: 0; }
</style>
