extern crate sqlx_core as sqlx;

use futures_util::future::try_join_all;
use sqlx_core::connection::Connection;
use sqlx_core::migrate::Migrator;
use sqlx_core::postgres::PgConnection;
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;

use sqlx_postgres_test_connection::TestConnection;

static MIGRATOR: Migrator = sqlx_macros::migrate!("./tests/migrations");

async fn assert_table_empty(db_url: &str) -> anyhow::Result<()> {
    let mut connection = PgConnection::connect(&db_url).await?;
    let table_names: Vec<String> =
        query_scalar("SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'")
            .fetch_all(&mut connection)
            .await?;
    assert!(table_names.is_empty());
    connection.close().await?;
    Ok(())
}

async fn async_migration_test(with_close: bool) -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL")?;

    assert_table_empty(&db_url).await?;

    let connection = PgConnection::connect(&db_url).await?;
    let mut test_connection = TestConnection::new(connection, &MIGRATOR).await?;

    let mut table_names: Vec<String> =
        query_scalar("SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'")
            .fetch_all(&mut *test_connection)
            .await?;
    table_names.sort();
    assert_eq!(table_names, &["_sqlx_migrations", "bar", "foo"]);

    let mut transaction = test_connection.begin().await?;
    query("INSERT INTO foo VALUES (10, 'foo')")
        .execute(&mut transaction)
        .await?;
    query("INSERT INTO bar VALUES ('na')")
        .execute(&mut transaction)
        .await?;
    transaction.commit().await?;

    let query_resp: i64 = query_scalar("SELECT a FROM foo")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, 10);
    let query_resp: String = query_scalar("SELECT b FROM foo")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, "foo");
    let query_resp: String = query_scalar("SELECT c::TEXT FROM bar")
        .fetch_one(&mut *test_connection)
        .await?;
    assert_eq!(query_resp, "na");

    if with_close {
        test_connection.close().await?;
    } else {
        drop(test_connection);
    }

    assert_table_empty(&db_url).await?;

    Ok(())
}

#[test]
fn migration_test() -> anyhow::Result<()> {
    sqlx_rt::block_on(async_migration_test(true))?;
    sqlx_rt::block_on(async_migration_test(false))?;
    Ok(())
}

#[test]
fn migration_stress_test() -> anyhow::Result<()> {
    sqlx_rt::block_on(try_join_all((0..100).map(|_| async_migration_test(true))))?;
    Ok(())
}
