use sqlx::Connection;

use sqlx_test_connection::TestConnection;

type DbConnection = sqlx::postgres::PgConnection;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("tests/migrations");

async fn migration_test(within_transaction: bool) -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL")?;
    let connection = DbConnection::connect(&db_url).await?;
    let mut test_connection = TestConnection::<sqlx::Postgres>::new(
        connection,
        MIGRATOR.iter().cloned().collect(),
        within_transaction,
    )
    .await?;

    let mut table_names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'",
    )
    .fetch_all(&mut *test_connection)
    .await?;
    table_names.sort();
    assert_eq!(table_names, &["_sqlx_migrations", "bar", "foo"]);

    let mut transaction = test_connection.begin().await?;
    sqlx::query("INSERT INTO foo VALUES (10, 'foo')")
        .execute(&mut transaction)
        .await?;
    sqlx::query("INSERT INTO bar VALUES ('na')")
        .execute(&mut transaction)
        .await?;
    transaction.commit().await?;

    let query_resp: i64 = sqlx::query_scalar("SELECT a FROM foo")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, 10);
    let query_resp: String = sqlx::query_scalar("SELECT b FROM foo")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, "foo");
    let query_resp: String = sqlx::query_scalar("SELECT c::TEXT FROM bar")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, "na");

    drop(test_connection);

    let mut connection = DbConnection::connect(&db_url).await?;
    let table_names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'",
    )
    .fetch_all(&mut connection)
    .await?;
    assert!(table_names.is_empty());

    Ok(())
}

#[sqlx::sqlx_macros::test]
#[serial_test::serial]
async fn test_within_transaction() {
    migration_test(true).await.unwrap();
}

#[sqlx::sqlx_macros::test]
#[serial_test::serial]
async fn test_without_transaction() {
    migration_test(false).await.unwrap();
}
