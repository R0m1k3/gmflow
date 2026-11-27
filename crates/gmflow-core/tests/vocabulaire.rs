//! Les valeurs décrites au front (`gmflow_core::vocabulaire`) sont celles que la base accepte :
//! une valeur ajoutée à une contrainte `check (x in (...))` doit l'être aussi au type TypeScript.
#![cfg(all(feature = "ts", feature = "test-utils"))]

use std::collections::BTreeSet;

use gmflow_core::test_utils::BaseTest;
use gmflow_core::vocabulaire as v;
use ts_rs::{Config, TS};

/// Valeurs d'une union de chaînes TypeScript (`"a" | "b"`).
fn valeurs<T: TS>() -> BTreeSet<String> {
    T::inline(&Config::default())
        .split('|')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect()
}

/// Valeurs de la contrainte `check` de `table.colonne` (`colonne = ANY (ARRAY['a'::text, …])`).
async fn contrainte(pool: &sqlx::PgPool, table: &str, colonne: &str) -> BTreeSet<String> {
    let defs: Vec<String> = sqlx::query_scalar(
        "select pg_get_constraintdef(oid) from pg_constraint
         where contype = 'c' and conrelid = $1::regclass",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap();
    let debut = format!("(({colonne} = ANY (ARRAY[");
    let def = defs
        .iter()
        .find(|d| d.contains(&debut))
        .unwrap_or_else(|| panic!("pas de contrainte sur {table}.{colonne} : {defs:?}"));
    let liste = &def[def.find(&debut).unwrap() + debut.len()..];
    liste[..liste.find(']').unwrap()]
        .split(", ")
        .map(|x| x.trim_end_matches("::text").trim_matches('\'').to_string())
        .collect()
}

#[tokio::test]
async fn vocabulaire_comme_la_base() {
    let base = BaseTest::nouvelle().await;
    let p = &base.pool;
    let cas: Vec<(&str, &str, BTreeSet<String>)> = vec![
        ("utilisateur", "role", valeurs::<v::Role>()),
        ("mail", "categorie", valeurs::<v::Categorie>()),
        ("mail", "categorie_par", valeurs::<v::CategoriePar>()),
        ("mail", "priorite", valeurs::<v::Priorite>()),
        ("mail", "statut_reponse", valeurs::<v::StatutReponse>()),
        ("mail", "tri_etat", valeurs::<v::TriEtat>()),
        (
            "brouillon_reponse",
            "statut",
            valeurs::<v::StatutBrouillon>(),
        ),
        ("document", "type", valeurs::<v::TypeDocument>()),
        ("document", "etat", valeurs::<v::EtatDocument>()),
        ("document", "statut_depot", valeurs::<v::StatutDepot>()),
        ("envoi_depot", "etat", valeurs::<v::EtatEnvoi>()),
        ("societe", "outil_depot", valeurs::<v::OutilDepot>()),
        ("fournisseur", "statut", valeurs::<v::StatutFournisseur>()),
        (
            "fournisseur",
            "colonnes_export",
            valeurs::<v::ColonnesExport>(),
        ),
        (
            "rapprochement",
            "methode",
            valeurs::<v::MethodeRapprochement>(),
        ),
        ("boite_mail", "smtp_securite", valeurs::<v::SecuriteSmtp>()),
        ("fournisseur_iban", "origine", valeurs::<v::OrigineIban>()),
        ("seance_claude", "etat", valeurs::<v::EtatSeance>()),
    ];
    for (table, colonne, attendu) in cas {
        assert_eq!(
            contrainte(p, table, colonne).await,
            attendu,
            "{table}.{colonne} : la base et gmflow_core::vocabulaire diffèrent"
        );
    }
    // Hors contrainte : les voies de lecture (`lecture::Voie`).
    use gmflow_core::lecture::Voie;
    let voies: BTreeSet<String> = [
        Voie::FacturX,
        Voie::Xml,
        Voie::PdfTexte,
        Voie::Tableur,
        Voie::Scan,
    ]
    .iter()
    .map(|x| x.code().to_string())
    .collect();
    assert_eq!(voies, valeurs::<v::Voie>());
    // Hors contrainte : les dépassements de borne (`confiance.trop_volumineux`).
    use gmflow_core::lecture::limites::Depassement as D;
    let depassements: BTreeSet<String> = [
        D::Taille { octets: 0, max: 0 },
        D::Decompresse { max: 0 },
        D::EntreesZip { entrees: 0 },
        D::Cellules { cellules: 0 },
        D::Pages { pages: 0, max: 0 },
        D::Objets { objets: 0 },
        D::Duree { secondes: 0 },
        D::Structure,
    ]
    .iter()
    .map(|x| x.code().to_string())
    .collect();
    assert_eq!(depassements, valeurs::<v::Depassement>());
    base.fermer().await;
}
