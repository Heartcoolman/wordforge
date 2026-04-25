pub mod keys;
pub mod migrate;
pub mod operations;
pub mod schema;

use std::time::Duration;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

pub const DEFAULT_POOL_CONNECTION_TIMEOUT_MS: u64 = 250;

#[derive(Debug, Clone)]
pub struct Store {
    pool: Pool<SqliteConnectionManager>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("not found: entity={entity}, key={key}")]
    NotFound { entity: String, key: String },
    #[error("conflict: entity={entity}, key={key}")]
    Conflict { entity: String, key: String },
    #[error("CAS retry exhausted after {attempts} attempts: entity={entity}, key={key}")]
    CasRetryExhausted {
        entity: String,
        key: String,
        attempts: u32,
    },
    #[error("validation error: {0}")]
    Validation(String),
    #[error("migration error at version {version}: {message}")]
    Migration { version: u32, message: String },
}

impl Store {
    pub fn open(db_path: &str, busy_timeout_ms: u64, pool_size: u32) -> Result<Self, StoreError> {
        Self::open_with_connection_timeout(
            db_path,
            busy_timeout_ms,
            pool_size,
            DEFAULT_POOL_CONNECTION_TIMEOUT_MS,
        )
    }

    pub fn open_with_connection_timeout(
        db_path: &str,
        busy_timeout_ms: u64,
        pool_size: u32,
        connection_timeout_ms: u64,
    ) -> Result<Self, StoreError> {
        let manager = SqliteConnectionManager::file(db_path).with_init(move |conn| {
            conn.execute_batch(&format!(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = {};",
                busy_timeout_ms
            ))
        });

        let pool = Pool::builder()
            .max_size(pool_size)
            .connection_timeout(Duration::from_millis(connection_timeout_ms.max(1)))
            .build(manager)?;

        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch(schema::DDL)?;
        Ok(())
    }

    pub fn run_migrations(&self) -> Result<(), StoreError> {
        migrate::run(self)
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
        Ok(())
    }

    pub fn connection(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, StoreError> {
        Ok(self.pool.get()?)
    }

    pub(crate) fn conn(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, StoreError> {
        self.connection()
    }

    /// Execute a function within a database transaction.
    /// On success, the transaction is committed; on failure, it is rolled back.
    pub fn with_transaction<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("BEGIN")?;
        match f(&conn) {
            Ok(result) => {
                conn.execute_batch("COMMIT")?;
                Ok(result)
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK").ok();
                Err(e)
            }
        }
    }

    pub(crate) fn serialize_json<T: Serialize>(value: &T) -> Result<String, StoreError> {
        Ok(serde_json::to_string(value)?)
    }

    pub(crate) fn deserialize_json<T: DeserializeOwned>(s: &str) -> Result<T, StoreError> {
        Ok(serde_json::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Store, StoreError};

    #[test]
    fn pool_connection_timeout_is_independent_from_sqlite_busy_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("pool-timeout.db");
        let store =
            Store::open_with_connection_timeout(db_path.to_str().expect("db path"), 5000, 1, 25)
                .expect("open store");
        let _held_conn = store.connection().expect("hold only pooled connection");

        let started = Instant::now();
        let result = store.connection();
        let elapsed = started.elapsed();

        assert!(matches!(result, Err(StoreError::Pool(_))));
        assert!(
            elapsed < Duration::from_millis(500),
            "pool acquisition waited for {elapsed:?}, which suggests it is still tied to busy_timeout"
        );
    }
}
