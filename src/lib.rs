use std::ops::{Deref, DerefMut};

use futures_executor::block_on;
use sqlx::migrate::{Migrate, Migration};
use sqlx::{Connection, Database, Executor, Transaction};

enum ConnectionOrTransaction<DB>
where
    DB: Database,
{
    Connection(DB::Connection),
    Transaction(Transaction<'static, DB>),
    Dropped,
}

pub struct TestConnection<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    inner: ConnectionOrTransaction<DB>,
    migrations: Vec<Migration>,
}

impl<DB> TestConnection<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    pub async fn new(
        mut connection: DB::Connection,
        migrations: Vec<Migration>,
        within_transaction: bool,
    ) -> Result<Self, sqlx::Error> {
        macro_rules! run_migration {
            ($conn:expr) => {
                $conn.ensure_migrations_table().await?;
                for migration in migrations.iter() {
                    if migration.migration_type.is_down_migration() {
                        continue;
                    }
                    $conn.apply(migration).await?;
                }
            };
        }
        let inner = if within_transaction {
            let connection = Box::new(connection);
            let connection = Box::leak(connection);
            let mut transaction = connection.begin().await?;
            run_migration!(transaction);
            ConnectionOrTransaction::Transaction(transaction)
        } else {
            run_migration!(connection);
            ConnectionOrTransaction::Connection(connection)
        };
        Ok(Self { inner, migrations })
    }
}

impl<DB> Drop for TestConnection<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    fn drop(&mut self) {
        let inner = std::mem::replace(&mut self.inner, ConnectionOrTransaction::Dropped);
        match inner {
            ConnectionOrTransaction::Connection(mut connection) => {
                let migrations = self.migrations.iter().rev();
                let fut = async move {
                    for migration in migrations {
                        if !migration.migration_type.is_down_migration() {
                            continue;
                        }
                        connection.revert(migration).await.unwrap();
                    }
                    connection
                        .execute("DROP TABLE IF EXISTS _sqlx_migrations")
                        .await
                        .unwrap();
                    connection.close().await.unwrap();
                };
                block_on(fut);
            }
            ConnectionOrTransaction::Transaction(mut transaction) => {
                let connection = unsafe { Box::from_raw(transaction.deref_mut()) };
                let fut = async move {
                    transaction.rollback().await.unwrap();
                    connection.close().await.unwrap();
                };
                block_on(fut);
            }
            ConnectionOrTransaction::Dropped => panic!(),
        }
    }
}

impl<DB> Deref for TestConnection<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    type Target = DB::Connection;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match &self.inner {
            ConnectionOrTransaction::Connection(conn) => conn,
            ConnectionOrTransaction::Transaction(trans) => &*trans,
            ConnectionOrTransaction::Dropped => panic!(),
        }
    }
}

impl<DB> DerefMut for TestConnection<DB>
where
    DB: Database,
    DB::Connection: Migrate,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.inner {
            ConnectionOrTransaction::Connection(conn) => conn,
            ConnectionOrTransaction::Transaction(trans) => &mut *trans,
            ConnectionOrTransaction::Dropped => panic!(),
        }
    }
}
