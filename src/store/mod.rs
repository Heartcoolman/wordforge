pub mod keys;
pub mod migrate;
pub mod operations;
pub mod schema;

use std::time::Duration;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

/// 默认连接池获取连接超时。CI / 慢盘环境 250 ms 不够（inline 单元测试并发跑时 pool 抢连接），
/// 改 2000 ms 给 SQLite open + WAL 初始化留足余量。Prod 启动显式传 env 配置，不走默认。
pub const DEFAULT_POOL_CONNECTION_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Clone)]
pub struct Store {
    pool: Pool<SqliteConnectionManager>,
}

/// r2d2 池实时占用快照(m023)。
#[derive(Debug, Clone, Copy)]
pub struct PoolStatus {
    pub max: u32,
    pub connections: u32,
    pub idle: u32,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("pool error: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
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
                 PRAGMA busy_timeout = {};
                 PRAGMA cache_size = -64000;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA temp_store = MEMORY;",
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
        // 仅在全新 DB（无 schema_version 表）时执行全量 DDL。
        // 老 DB 由 run_migrations 增量演进——避免 schema.rs 中依赖
        // migration 后才存在的列的 CREATE INDEX 在老 DB 上 fail
        // （如 idx_learning_records_user_type_time 依赖 m005 添加的 record_type 列）。
        let is_fresh: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |row| row.get(0),
        )?;
        if is_fresh == 0 {
            conn.execute_batch(schema::DDL)?;
        }
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

    /// 把当前库快照拷贝到 `dst`。先 checkpoint 落 WAL，再 `VACUUM INTO` 生成单文件副本。
    /// 用于自更新前的兜底备份。
    ///
    /// v1.1.0-beta.3：VACUUM INTO 之前先 `remove_file(dst)`，避免 SQLite 自带的
    /// "output file already exists" 拒绝（vacuum.c 主动检测 + SQLITE_ERROR）。
    /// 上次升级失败回滚后 backup 文件遗留是 stale，本次升级生成的 backup 才是有效兜底。
    pub fn backup_to(&self, dst: &std::path::Path) -> Result<(), StoreError> {
        // 预清理：忽略不存在（NotFound）以外的错误，避免无谓 fail；存在时清掉让 VACUUM INTO 可写。
        if dst.exists() {
            std::fs::remove_file(dst).map_err(StoreError::Io)?;
        }
        let conn = self.conn()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let raw = dst.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{raw}';"))?;
        Ok(())
    }

    /// 在线 VACUUM：重建 DB 文件，回收删除后产生的空页，收缩磁盘占用。
    /// 月度 retention cron 在删除大量旧 monitoring 事件后调用。
    /// 注意：VACUUM 会短暂阻塞写入（WAL 模式下其他读可以并发），
    /// 因此调度在低峰时段（UTC 03:00 月初 1 日）。
    pub fn vacuum_db(&self) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    /// m023:r2d2 池占用快照 —— Dashboard 系统资源条用。
    /// `connections` 是当前已创建的连接数,`idle` 是空闲数,差值即"在用"。
    pub fn pool_status(&self) -> PoolStatus {
        let st = self.pool.state();
        PoolStatus {
            max: self.pool.max_size(),
            connections: st.connections,
            idle: st.idle_connections,
        }
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
