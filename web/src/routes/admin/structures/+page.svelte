<script lang="ts">
  import { api, message } from '#lib/api.ts';
  import type { Societe } from '#lib/types.ts';

  type Form = { id: number | null; raison_sociale: string; siret: string; tva: string; adresses: string; outil_depot: string; adresse_depot: string; depot_auto: boolean; archivee: boolean };
  const vide = (): Form => ({ id: null, raison_sociale: '', siret: '', tva: '', adresses: '', outil_depot: '', adresse_depot: '', depot_auto: false, archivee: false });

  let liste = $state<Societe[]>([]);
  let f = $state<Form | null>(null);
  let erreur = $state('');
  let info = $state('');
  let chargement = $state(true);

  async function charger() {
    liste = await api.get<Societe[]>('/api/admin/societes');
    chargement = false;
  }
  $effect(() => {
    charger().catch((e) => {
      chargement = false;
      erreur = message(e);
    });
  });

  function editer(s: Societe) {
    f = { id: s.id, raison_sociale: s.raison_sociale, siret: s.siret ?? '', tva: s.tva ?? '', adresses: (s.adresses ?? []).join('\n'), outil_depot: s.outil_depot ?? '', adresse_depot: s.adresse_depot ?? '', depot_auto: s.depot_auto, archivee: s.archivee };
  }

  async function enregistrer(e: SubmitEvent) {
    e.preventDefault();
    if (!f) return;
    erreur = '';
    info = '';
    const corps = {
      raison_sociale: f.raison_sociale,
      siret: f.siret,
      tva: f.tva,
      adresses: f.adresses.split('\n').map((l) => l.trim()).filter(Boolean),
      outil_depot: f.outil_depot || null,
      adresse_depot: f.adresse_depot,
      depot_auto: f.depot_auto,
      archivee: f.archivee
    };
    try {
      if (f.id) await api.put(`/api/admin/societes/${f.id}`, corps);
      else await api.post('/api/admin/societes', corps);
      info = `Société « ${f.raison_sociale} » enregistrée.`;
      f = null;
      await charger();
    } catch (err) {
      erreur = message(err);
    }
  }
</script>

<div class="entete">
  <div>
    <h1>Sociétés</h1>
    <p class="sous">Les sociétés du groupe, chacune avec sa boîte mail et son adresse de dépôt Dext ou Yooz.</p>
  </div>
  {#if !f}<div class="actions"><button onclick={() => (f = vide())}>Ajouter une société</button></div>{/if}
</div>
{#if erreur}<div class="bandeau erreur">{erreur} <button class="discret" onclick={() => { erreur = ''; chargement = true; charger().catch((e) => (erreur = message(e))).finally(() => (chargement = false)); }}>Réessayer</button></div>{/if}
{#if info}<div class="bandeau ok">{info}</div>{/if}

{#if f}
  <form class="pile" onsubmit={enregistrer}>
    <h2 class="sans-marge">{f.id ? `Modifier « ${f.raison_sociale || 'société'} »` : 'Nouvelle société'}</h2>

    <fieldset>
      <legend>Identité</legend>
      <div class="champs">
        <div>
          <label for="rs">Raison sociale</label>
          <input id="rs" bind:value={f.raison_sociale} required />
          <p class="aide">Le nom affiché partout dans GMFlow.</p>
        </div>
        <div>
          <label for="siret">SIRET</label>
          <input id="siret" bind:value={f.siret} inputmode="numeric" placeholder="14 chiffres" />
          <p class="aide">Facultatif. Aide à reconnaître les factures adressées à cette société.</p>
        </div>
        <div>
          <label for="tva">N° de TVA</label>
          <input id="tva" bind:value={f.tva} placeholder="FR12345678901" />
          <p class="aide">Facultatif. Même usage que le SIRET.</p>
        </div>
      </div>
      <label for="adr">Adresses de la société</label>
      <textarea id="adr" rows="3" bind:value={f.adresses}></textarea>
      <p class="aide">Une adresse par ligne, telle qu'elle figure sur les factures reçues. Sert à vérifier que la facture est bien pour cette société.</p>
    </fieldset>

    <fieldset>
      <legend>Dépôt des factures</legend>
      <div class="champs">
        <div>
          <label for="outil">Outil de dépôt</label>
          <select id="outil" bind:value={f.outil_depot}><option value="">Aucun</option><option value="dext">Dext</option><option value="yooz">Yooz</option></select>
          <p class="aide">Le logiciel de comptabilité qui reçoit les factures par mail.</p>
        </div>
        <div>
          <label for="ad">Adresse de dépôt</label>
          <input id="ad" type="email" bind:value={f.adresse_depot} placeholder="societe@inbox.dext.com" />
          <p class="aide">L'adresse mail fournie par Dext ou Yooz pour cette société.</p>
        </div>
      </div>
      <label class="case"><input type="checkbox" bind:checked={f.depot_auto} /> Déposer automatiquement les factures sans problème</label>
      <p class="aide">Activé, les factures qui passent tous les contrôles partent seules. Sinon, chacune attend votre clic sur « Valider et déposer ».</p>
    </fieldset>

    {#if f.id}
      <fieldset>
        <legend>Archivage</legend>
        <label class="case"><input type="checkbox" bind:checked={f.archivee} /> Société archivée</label>
        <p class="aide">Une société archivée n'apparaît plus dans les choix (nouvelle boîte mail) mais garde son historique.</p>
      </fieldset>
    {/if}

    <div class="actions"><button>Enregistrer</button><button type="button" class="second" onclick={() => (f = null)}>Annuler</button></div>
  </form>
{:else if chargement}
  <div class="charge" aria-busy="true"><span></span></div>
{:else if liste.length === 0}
  <div class="vide">
    <strong>Aucune société</strong>
    <p>Commencez par créer la société dont vous voulez suivre les mails. Vous ajouterez ensuite sa boîte mail.</p>
    <div class="actions"><button onclick={() => (f = vide())}>Créer la première société</button></div>
  </div>
{:else}
  <ul class="liste">
    {#each liste as s (s.id)}
      <li>
        <div class="principal">
          <span>{s.raison_sociale}</span>
          <span class="discret">
            SIRET {s.siret ?? 'Non renseigné'} ·
            {s.outil_depot ? `Dépôt ${s.outil_depot === 'dext' ? 'Dext' : s.outil_depot === 'yooz' ? 'Yooz' : s.outil_depot}${s.adresse_depot ? ` : ${s.adresse_depot}` : ''}` : 'Aucun outil de dépôt'}
          </span>
        </div>
        <div class="droite">
          {#if s.archivee}<span class="etiquette">Archivée</span>{/if}
          {#if s.outil_depot}<span class="etiquette {s.depot_auto ? 'ok' : ''}">{s.depot_auto ? 'Dépôt automatique' : 'Dépôt à valider'}</span>{/if}
          <button class="second" onclick={() => editer(s)}>Modifier</button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .sans-marge { margin: 0; }
</style>
