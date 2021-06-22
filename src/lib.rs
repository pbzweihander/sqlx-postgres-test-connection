use std::ops::{Deref, DerefMut};

use futures_executor::block_on;
use sqlx_core::connection::Connection;
use sqlx_core::migrate::{MigrateError, Migrator};
use sqlx_core::postgres::{PgConnection, Postgres};
use sqlx_core::transaction::Transaction;

pub struct TestConnection(Option<Transaction<'static, Postgres>>);

impl Deref for TestConnection {
    type Target = Transaction<'static, Postgres>;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl DerefMut for TestConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

impl TestConnection {
    pub async fn new(connection: PgConnection, migrator: &Migrator) -> Result<Self, MigrateError> {
        let connection = Box::new(connection);
        let connection = Box::leak(connection);
        let transaction = connection.begin().await?;
        let mut transaction = Self(Some(transaction));
        let migrate_res = migrator.run(transaction.deref_mut()).await;
        match migrate_res {
            Ok(_) => Ok(transaction),
            Err(e) => {
                transaction.async_drop().await;
                Err(e)
            }
        }
    }

    async fn async_drop(&mut self) {
        if self.0.is_none() {
            return;
        }
        let mut transaction = self.0.take().unwrap();
        let connection = unsafe { Box::from_raw(transaction.deref_mut()) };
        transaction.rollback().await.ok();
        connection.close().await.ok();
    }
}

impl Drop for TestConnection {
    fn drop(&mut self) {
        block_on(self.async_drop());
    }
}
