<script lang="ts">
  // IBAN connus d'un fournisseur (alerte « IBAN changé » contre la fraude au RIB), masqués au milieu.
  // Le premier IBAN lu sur une facture validée est retenu ; un autre ne l'est que par « C'est le bon
  // IBAN » sur la facture. Un administrateur peut en retirer un (journalisé).
  import { ibansFournisseur, message, retirerIban } from '#lib/api.ts';
  import { dateSimple } from '#lib/format.ts';
  import type { IbanConnu } from '#lib/types.ts';
  import Bandeau from './Bandeau.svelte';
  import Confirmation from './Confirmation.svelte';

  let { id, admin }: { id: number; admin: boolean } = $props();

  let ibans = $state<IbanConnu[] | null>(null);
  let erreur = $state('');
  let info = $state('');

  async function charger() {
    try {
      ibans = await ibansFournisseur(id);
      erreur = '';
    } catch (e) {
      erreur = message(e);
    }
  }
  $effect(() => {
    void id;
    ibans = null;
    info = '';
    void charger();
  });

  function origine(i: IbanConnu): string {
    const qui = i.ajoute_par ? ` par ${i.ajoute_par}` : '';
    return i.origine === 'confirmation' ? `Confirmé${qui} le ${dateSimple(i.ajoute_le.slice(0, 10))}` : `Retenu à la validation${qui} le ${dateSimple(i.ajoute_le.slice(0, 10))}`;
  }

  let aRetirer = $state<IbanConnu | null>(null);
  let dlgRetirer = $state(false);
  async function retirer() {
    if (!aRetirer) return;
    try {
      await retirerIban(id, aRetirer.id);
      info = `IBAN ${aRetirer.iban} retiré`;
      dlgRetirer = false;
      aRetirer = null;
      await charger();
    } catch (e) {
      erreur = message(e);
      dlgRetirer = false;
    }
  }
</script>

<section class="bloc-iban" aria-labelledby="iban-{id}">
  <h3 id="iban-{id}">IBAN connus {#if ibans?.length}<span class="num muted">{ibans.length}</span>{/if}</h3>
  {#if erreur}<Bandeau ton="erreur">{erreur}</Bandeau>{/if}
  {#if info}<Bandeau ton="ok" role="status">{info}</Bandeau>{/if}
  {#if ibans && ibans.length === 0}
    <p class="aide-i">Aucun IBAN connu. Le premier lu sur une facture validée sera retenu ; ensuite, une facture avec un autre IBAN sera signalée avant l’envoi.</p>
  {:else if ibans}
    <div class="filets">
      {#each ibans as i (i.id)}
        <div class="ligne" class:bloc={admin}>
          <div class="a"><span class="num">{i.iban}</span>{#if i.bic}{' '}<span class="num petit muted">BIC {i.bic}</span>{/if}</div>
          <div class="r petit t2">{i.pays}</div>
          <div class="b petit t2">
            {origine(i)}{#if i.premiere_vue_le}, vu pour la première fois sur la facture du <span class="num">{dateSimple(i.premiere_vue_le)}</span>{/if}{#if i.document_id}{' '}(<a href="/documents/{i.document_id}">voir</a>){/if}{i.du_groupe ? ', connu pour tout le groupe' : ''}
          </div>
          {#if admin}
            <div class="r2">
              <button
                type="button"
                class="btn sm fantome"
                onclick={() => {
                  aRetirer = i;
                  dlgRetirer = true;
                }}>Retirer</button
              >
            </div>
          {/if}
        </div>
      {/each}
    </div>
    <p class="aide-i petit">Une facture avec un autre IBAN est signalée avant l’envoi : vérifiez par téléphone auprès du fournisseur avant de payer.</p>
  {/if}
</section>

<Confirmation bind:ouvert={dlgRetirer} titre="Retirer cet IBAN" libelle="Retirer" danger onconfirmer={retirer}>
  <p>L’IBAN <span class="num">{aRetirer?.iban}</span> ne sera plus tenu pour celui du fournisseur. Les factures qui le portent seront de nouveau signalées.</p>
</Confirmation>

<style>
  .bloc-iban {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .bloc-iban > h3 {
    margin: 0;
    font-size: 20px;
    font-weight: 500;
    line-height: 1.3;
  }
  .aide-i {
    margin: 0;
    color: var(--text2);
    text-wrap: pretty;
  }
</style>
