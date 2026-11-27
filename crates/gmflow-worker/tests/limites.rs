//! Pièces hors des bornes d'analyse (constat C9) : marquées illisibles avec la raison, sans
//! appel à Claude ni blocage de la file ; le fichier reste téléchargeable.

mod commun;

use commun::{Banc, WorkersTest, lire_tout, mail};
use gmflow_core::fixtures::{self, SIRET_A};
use gmflow_core::parametre;

#[tokio::test]
async fn tableur_piege_marque_trop_volumineux_et_la_file_continue() {
    let b = Banc::new().await;
    let a: i64 = sqlx::query_scalar(
        "insert into societe (raison_sociale, siret) values ($1, $2) returning id",
    )
    .bind("Magasin Frouard")
    .bind(SIRET_A)
    .fetch_one(b.pool())
    .await
    .unwrap();
    parametre::enregistrer_jeton_claude(b.pool(), &b.ctx.chiffreur, "sk-ant-oat01-test_limites")
        .await
        .unwrap();
    let piege = fixtures::xlsx_cellules(&[(0, 0, "BL 42"), (1_048_575, 16_383, "x")]);
    assert!(piege.len() < 20_000, "{}", piege.len());
    let logo = fixtures::png_logo();
    let m = mail(
        &b,
        a,
        &fixtures::eml(
            "piege@test",
            "BL",
            &[
                (
                    "bl.xlsx",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    &piege,
                ),
                ("logo.png", "image/png", &logo),
            ],
            false,
        ),
    )
    .await;
    let w = WorkersTest::demarrer(b.ctx.clone());
    lire_tout(&b, &[m]).await;
    w.arreter();
    let lignes: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "select d.etat, d.message, d.confiance->>'trop_volumineux'
         from document d join piece_jointe p on p.id = d.piece_jointe_id
         where d.mail_id = $1 order by p.rang",
    )
    .bind(m)
    .fetch_all(b.pool())
    .await
    .unwrap();
    let (etat, message, code) = &lignes[0];
    assert_eq!(etat, "erreur");
    assert!(
        message
            .as_deref()
            .is_some_and(|m| m.starts_with("Pièce trop volumineuse pour être lue automatiquement")),
        "{message:?}"
    );
    assert_eq!(code.as_deref(), Some("cellules"));
    // L'autre pièce du même mail a été traitée normalement (la file n'est pas bloquée).
    assert_eq!(lignes[1].0, "ignore");
    // Aucun appel à Claude n'a été fait.
    assert_eq!(b.compter("select count(*) from appel_claude").await, 0);
    b.fermer().await;
}
