use std::sync::Arc;

use async_std::sync::RwLock;
use sqlx::migrate::Migrate;
use sqlx::{Connection, Database, Executor, Transaction};
use tide::{Middleware, Next, Request};
use tide_sqlx::ConnectionWrapInner;

pub use sqlx_test_connection::TestConnection;

pub struct TestSQLxMiddleware<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    test_connection: Box<RwLock<TestConnection<DB>>>,
}

impl<DB> TestSQLxMiddleware<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    pub fn new(test_connection: TestConnection<DB>) -> Self {
        let test_connection = Box::new(RwLock::new(test_connection));
        Self { test_connection }
    }
}

async fn handle_inner<State, DB>(
    transaction: Transaction<'static, DB>,
    mut req: Request<State>,
    next: Next<'_, State>,
) -> tide::Result
where
    State: Clone + Send + Sync + 'static,
    DB: Database,
    DB::Connection: Migrate + Sync,
{
    let conn_wrap_inner = ConnectionWrapInner::Transacting(transaction);
    let conn_wrap = Arc::new(RwLock::new(conn_wrap_inner));
    req.set_ext(conn_wrap.clone());

    let res = next.run(req).await;

    if res.error().is_none() {
        if let Ok(conn_wrap_inner) = Arc::try_unwrap(conn_wrap) {
            if let ConnectionWrapInner::Transacting(connection) = conn_wrap_inner.into_inner() {
                connection.commit().await?;
            }
        } else {
            panic!();
        }
    }

    Ok(res)
}

#[async_trait::async_trait]
impl<State, DB> Middleware<State> for TestSQLxMiddleware<DB>
where
    State: Clone + Send + Sync + 'static,
    DB: Database,
    DB::Connection: Migrate + Sync,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let mut conn_lock = self.test_connection.write().await;
        let transaction = conn_lock.begin().await;

        let res = match transaction {
            Ok(transaction) => {
                let transaction: Box<Transaction<'static, DB>> =
                    unsafe { std::mem::transmute(Box::new(transaction)) };
                handle_inner::<State, DB>(*transaction, req, next).await
            }
            Err(err) => Err(err.into()),
        };

        res
    }
}
