use std::ops::{Deref, DerefMut};

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
                let _ = transaction.close().await;
                Err(e)
            }
        }
    }

    pub async fn close(mut self) -> Result<(), sqlx_core::error::Error> {
        if let Some((transaction, connection)) = self.take_inner() {
            transaction.rollback().await?;
            connection.close().await?;
        }
        Ok(())
    }

    fn take_inner(&mut self) -> Option<(Transaction<'static, Postgres>, Box<PgConnection>)> {
        if let Some(mut transaction) = self.0.take() {
            let connection = unsafe { Box::from_raw(transaction.deref_mut()) };
            Some((transaction, connection))
        } else {
            None
        }
    }
}

impl Drop for TestConnection {
    fn drop(&mut self) {
        if let Some((transaction, connection)) = self.take_inner() {
            drop(transaction);
            drop(connection);
        }
    }
}
