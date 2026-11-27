<script lang="ts">
  // Accès direct à une page de réglages sans être administrateur (lien reçu, favori) : un refus
  // clair avec la suite possible, plutôt qu'une page vide ou un renvoi muet vers l'accueil.
</script>

<section class="refus" aria-labelledby="refus-t">
  <h2 id="refus-t">Réservé aux administrateurs</h2>
  <p>
    Les réglages de GMFlow (sociétés, boîtes mail, connexions, maintenance) ne se changent qu’avec un compte
    administrateur. Demandez-lui si quelque chose doit changer.
  </p>
  <div class="actions">
    <a class="btn principal" href="/compte">Mon compte</a>
    <a class="btn" href="/">Retour à l’accueil</a>
  </div>
</section>

<style>
  .refus {
    display: flex;
    flex-direction: column;
    gap: var(--e4);
    max-width: 640px;
  }
  h2 {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 500;
    margin: 0;
  }
  p {
    margin: 0;
    color: var(--text2);
    font-size: 16px;
    line-height: 1.45;
    max-width: 560px;
  }
  .actions {
    display: flex;
    gap: var(--e3);
    flex-wrap: wrap;
  }
</style>
