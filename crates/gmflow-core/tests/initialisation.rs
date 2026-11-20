use gmflow_core::db;
use gmflow_core::test_utils::BaseTest;

async fn compter(pool: &sqlx::PgPool, requete: &'static str) -> i64 {
    sqlx::query_scalar::<_, i64>(requete).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn deux_initialisations_simultanees() {
    let base = BaseTest::vide().await;
    let p1 = db::connecter(&base.url).await.unwrap();
    let p2 = db::connecter(&base.url).await.unwrap();
    let (r1, r2) = tokio::join!(db::initialiser(&p1), db::initialiser(&p2));
    r1.unwrap();
    r2.unwrap();

    let nb_migrations = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "sql"))
        .count() as i64;
    let pool = &base.pool;
    assert_eq!(compter(pool, "select count(*) from public._sqlx_migrations").await, nb_migrations);
    assert_eq!(
        compter(pool, "select count(*) from (select version from public._sqlx_migrations group by version having count(*) > 1) d").await,
        0
    );
    let apalis = compter(pool, "select count(*) from apalis._sqlx_migrations").await;
    assert!(apalis > 0);
    assert_eq!(
        compter(pool, "select count(*) from (select version from apalis._sqlx_migrations group by version having count(*) > 1) d").await,
        0
    );
    assert_eq!(
        compter(pool, "select count(*) from information_schema.tables where table_schema = 'tower_sessions' and table_name = 'session'").await,
        1
    );

    // Troisième appel : rien de nouveau.
    db::initialiser(pool).await.unwrap();
    assert_eq!(compter(pool, "select count(*) from public._sqlx_migrations").await, nb_migrations);
    assert_eq!(compter(pool, "select count(*) from apalis._sqlx_migrations").await, apalis);

    p1.close().await;
    p2.close().await;
    base.fermer().await;
}
