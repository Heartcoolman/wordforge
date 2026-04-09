pub mod keys;
pub mod migrate;
pub mod operations;
pub mod schema;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

#[derive(Debug)]
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
            .connection_timeout(std::time::Duration::from_millis(busy_timeout_ms))
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

    pub(crate) fn conn(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, StoreError> {
        Ok(self.pool.get()?)
    }

    pub(crate) fn serialize_json<T: Serialize>(value: &T) -> Result<String, StoreError> {
        Ok(serde_json::to_string(value)?)
    }

    pub(crate) fn deserialize_json<T: DeserializeOwned>(s: &str) -> Result<T, StoreError> {
        Ok(serde_json::from_str(s)?)
    }
}
