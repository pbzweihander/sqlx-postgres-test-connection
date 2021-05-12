use sqlx::{Acquire, Connection};
use tide_sqlx::SQLxRequestExt;
use tide_sqlx_test_connection::{TestConnection, TestSQLxMiddleware};
use tide_testing::TideTestingExt;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../tests/migrations");

type Database = sqlx::postgres::Postgres;
type DbConnection = sqlx::postgres::PgConnection;

async fn ping(req: tide::Request<()>) -> tide::Result {
    let mut db_conn = req.sqlx_conn::<Database>().await;
    let query_resp: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(db_conn.acquire().await?)
        .await?;
    assert_eq!(query_resp, 1);
    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn insert(req: tide::Request<()>) -> tide::Result {
    let mut db_conn = req.sqlx_conn::<Database>().await;
    let mut transaction = Connection::begin(&mut **db_conn).await?;
    sqlx::query("INSERT INTO foo VALUES (10, 'foo')")
        .execute(&mut transaction)
        .await?;
    sqlx::query("INSERT INTO bar VALUES ('na')")
        .execute(&mut transaction)
        .await?;
    transaction.commit().await?;
    let query_resp: i64 = sqlx::query_scalar("SELECT a FROM foo")
        .fetch_one(db_conn.acquire().await?)
        .await?;
    assert_eq!(query_resp, 10);
    let query_resp: String = sqlx::query_scalar("SELECT b FROM foo")
        .fetch_one(db_conn.acquire().await?)
        .await?;
    assert_eq!(query_resp, "foo");
    let query_resp: String = sqlx::query_scalar("SELECT c::TEXT FROM bar")
        .fetch_one(db_conn.acquire().await?)
        .await?;
    assert_eq!(query_resp, "na");
    Ok(tide::Response::new(tide::StatusCode::Ok))
}

async fn database_integration_test(within_transaction: bool) -> tide::Result<()> {
    dotenv::dotenv().unwrap();

    let db_url = std::env::var("DATABASE_URL")?;
    let connection = DbConnection::connect(&db_url).await?;
    let mut test_connection = TestConnection::<Database>::new(
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

    let middleware = TestSQLxMiddleware::new(test_connection);
    let mut server = tide::new();
    server.with(middleware);
    // For debuging tests
    /*
    server.with(tide::utils::After(|resp: tide::Response| async move {
        if let Some(err) = resp.error() {
            println!("{:?}", err)
        }
        Ok(resp)
    }));
    */
    server.at("/ping").get(ping);
    server.at("/insert").get(insert);

    {
        let client = server.client();

        let mut resp = client.get("/ping").await?;
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body_string().await?, String::new());

        let mut resp = client.get("/insert").await?;
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body_string().await?, String::new());
    }
    drop(server);

    let mut connection = DbConnection::connect(&db_url).await?;
    let table_names: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'",
    )
    .fetch_all(&mut connection)
    .await?;
    assert!(table_names.is_empty());

    Ok(())
}

#[async_std::test]
#[serial_test::serial]
async fn test_within_transaction() {
    database_integration_test(true).await.unwrap();
}

#[async_std::test]
#[serial_test::serial]
async fn test_without_transaction() {
    database_integration_test(false).await.unwrap();
}
