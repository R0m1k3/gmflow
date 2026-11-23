//! Identification des fournisseurs : une même entreprise écrite autrement ne crée pas de
//! nouvelle fiche, et les doublons déjà en base sont proposés à la fusion.

use gmflow_core::fournisseurs::{self, Identification, Indices};
use gmflow_core::test_utils::BaseTest;

async fn fiche(pool: &sqlx::PgPool, nom: &str, domaines: &[&str]) -> i64 {
    let d: Vec<String> = domaines.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "insert into fournisseur (nom, nom_normalise, domaines) values ($1, $2, $3) returning id",
    )
    .bind(nom)
    .bind(fournisseurs::normaliser(nom))
    .bind(&d)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn nom(n: &str) -> Indices {
    Indices {
        nom: Some(n.to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn meme_entreprise_autre_ecriture() {
    let b = BaseTest::nouvelle().await;
    let pool = &b.pool;
    let lff = fiche(pool, "La Foir'Fouille", &[]).await;
    let durand = fiche(pool, "Grossiste Durand", &["durand.fr"]).await;

    // Nom tiré d'un domaine, nom collé.
    for n in [
        "lafoirfouille.fr",
        "LAFOIRFOUILLE",
        "La Foir Fouille (Frouard)",
    ] {
        assert_eq!(
            fournisseurs::identifier(pool, &nom(n)).await.unwrap(),
            Identification::Trouve {
                id: lff,
                par: "nom"
            },
            "{n}"
        );
    }
    // Domaine de l'expéditeur = nom de la fiche, sans nom lu.
    let ind = Indices {
        domaine: Some("lafoirfouille.fr".into()),
        ..Default::default()
    };
    assert!(matches!(
        fournisseurs::identifier(pool, &ind).await.unwrap(),
        Identification::Trouve { id, .. } if id == lff
    ));
    // Nom proche sans mot commun : montré à Claude, pas rattaché d'office.
    match fournisseurs::identifier(pool, &nom("Foirfouille Centrale"))
        .await
        .unwrap()
    {
        Identification::Candidats(c) => assert!(c.iter().any(|c| c.id == lff)),
        autre => panic!("{autre:?}"),
    }
    // Rien à voir : nouvelle fiche, nom débarrassé de la parenthèse.
    assert_eq!(
        fournisseurs::identifier(pool, &nom("U10 (SAS U10, Lingolsheim)"))
            .await
            .unwrap(),
        Identification::Aucun
    );
    let u10 = fournisseurs::creer(pool, &nom("U10 (SAS U10, Lingolsheim)"))
        .await
        .unwrap()
        .unwrap();
    let n: String = sqlx::query_scalar("select nom from fournisseur where id = $1")
        .bind(u10)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, "U10");

    // Fiche nommée d'après son domaine : prend le vrai nom lu ensuite.
    let glama = fiche(pool, "glama.fr", &["glama.fr"]).await;
    fournisseurs::apprendre(pool, glama, &nom("GLAMA SAS"), false)
        .await
        .unwrap();
    let n: String = sqlx::query_scalar("select nom from fournisseur where id = $1")
        .bind(glama)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, "GLAMA SAS");
    fournisseurs::apprendre(pool, durand, &nom("Autre nom"), false)
        .await
        .unwrap();
    let n: String = sqlx::query_scalar("select nom from fournisseur where id = $1")
        .bind(durand)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, "Grossiste Durand");

    // Doublons déjà en base.
    let lff2 = fiche(pool, "lafoirfouille.fr", &["lafoirfouille.fr"]).await;
    let d = fournisseurs::doublons_probables(pool).await.unwrap();
    assert!(d.iter().any(|p| p.a == lff && p.b == lff2), "{d:?}");
    assert!(!d.iter().any(|p| p.a == durand || p.b == durand), "{d:?}");
    sqlx::query("insert into fournisseur_distinct (a, b) values ($1, $2)")
        .bind(lff)
        .bind(lff2)
        .execute(pool)
        .await
        .unwrap();
    let d = fournisseurs::doublons_probables(pool).await.unwrap();
    assert!(!d.iter().any(|p| p.a == lff && p.b == lff2));
    b.fermer().await;
}
