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
        // cache_size/mmap_size 为每连接独占，乘 pool_size：压测实测 -64000(62.5MB)×16≈1GB
        // 峰值，在 2h2g 机型上是 OOM 风险，故收到 16MB/128MB。勿无依据调高。
        let manager = SqliteConnectionManager::file(db_path).with_init(move |conn| {
            conn.execute_batch(&format!(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = {};
                 PRAGMA wal_autocheckpoint = 2000;
                 PRAGMA cache_size = -16000;
                 PRAGMA mmap_size = 134217728;
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

    /// 主动把 WAL 帧落回主库并**截断** WAL 文件，回收磁盘并避免臃肿 WAL 放大每次 commit 的
    /// 页查找成本。由周期 worker（wal_checkpoint，见 workers/mod.rs）在低峰调用——默认
    /// `wal_autocheckpoint` 的 PASSIVE 内联 checkpoint 不缩文件、且高写时与写锁争用，故另设
    /// TRUNCATE 周期回收。TRUNCATE 与并发写短暂竞争属预期；busy 时本调用返回错误，调用方仅 warn。
    pub fn checkpoint_truncate(&self) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
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

    /// 从备份文件恢复到当前库（SQLite Online Backup API：src→live）。
    /// 调用前应已开维护模式 + 做好 pre-restore 兜底备份。先 checkpoint(TRUNCATE) 落 WAL。
    /// 注意：恢复后建议重启进程让池中所有连接重读；本方法仅做在线页拷贝。
    pub fn restore_from(&self, src: &std::path::Path) -> Result<(), StoreError> {
        if !src.is_file() {
            return Err(StoreError::NotFound {
                entity: "backup".into(),
                key: src.to_string_lossy().into(),
            });
        }
        let src_conn = rusqlite::Connection::open_with_flags(
            src,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut dst = self.conn()?;
        dst.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst)?;
        backup.run_to_completion(200, std::time::Duration::from_millis(25), None)?;
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
    ///
    /// 用 rusqlite RAII 事务（`unchecked_transaction`）：commit 前任何早退（含 COMMIT
    /// 失败、闭包返回 Err、panic）都由 `Transaction` 的 Drop 自动 ROLLBACK，连接归还池前
    /// 不会残留未提交事务，避免污染后续使用者。
    pub fn with_transaction<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let conn = self.conn()?;
        let tx = conn.unchecked_transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    /// 在**单条**池连接 + **单个 `BEGIN IMMEDIATE` 事务**内运行闭包：跨多步 read-modify-write
    /// 全部走同一连接同一事务，commit 前任何早退（闭包返回 Err / COMMIT 失败 / panic）由
    /// `Transaction` 的 Drop 自动 ROLLBACK。
    ///
    /// 用 `IMMEDIATE` 而非默认 `DEFERRED`：BEGIN 时即取写锁，避免「先 SELECT 拿读锁、再写时升级」
    /// 的丢更新窗口与升级死锁（DEFERRED 读后写经典坑）。AMAS 热路径的 ELO/user_stats 计数器等
    /// read-modify-write 必须经此原语，配合调用侧持有 per-user 锁，保证逻辑单元整体原子串行。
    ///
    /// B6/records 路径采纳说明见 `records.rs`/调用侧：把「ELO RMW + record 行 + user_stats」收进
    /// 同一 `with_user_tx` 闭包，使「AMAS 状态 ⟺ ELO ⟺ 幂等标记」全有或全无。
    pub fn with_user_tx<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
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

    #[test]
    fn restore_from_recovers_deleted_row() {
        // 唯一目录：进程 pid + 固定后缀，避免并发测试碰撞（不依赖 rand/Date）。
        let dir = std::env::temp_dir().join(format!("wf-restore-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let live_path = dir.join("live.db");
        let backup_path = dir.join("snapshot.db");
        let _ = std::fs::remove_file(&live_path);
        let _ = std::fs::remove_file(&backup_path);

        let store = Store::open(live_path.to_str().expect("live path"), 5000, 2).expect("open");
        store
            .conn()
            .expect("conn")
            .execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t (id, v) VALUES (1, 'keep');")
            .expect("seed row");

        store.backup_to(&backup_path).expect("backup_to");

        store
            .conn()
            .expect("conn")
            .execute_batch("DELETE FROM t WHERE id = 1;")
            .expect("delete row");
        let after_delete: i64 = store
            .conn()
            .expect("conn")
            .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
            .expect("count after delete");
        assert_eq!(after_delete, 0, "行应已删除");

        store.restore_from(&backup_path).expect("restore_from");

        let after_restore: i64 = store
            .conn()
            .expect("conn")
            .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
            .expect("count after restore");
        assert_eq!(after_restore, 1, "恢复后行应回来");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_from_missing_src_returns_not_found() {
        let dir =
            std::env::temp_dir().join(format!("wf-restore-test-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let live_path = dir.join("live.db");
        let _ = std::fs::remove_file(&live_path);
        let store = Store::open(live_path.to_str().expect("live path"), 5000, 1).expect("open");
        let missing = dir.join("does-not-exist.db");
        assert!(matches!(
            store.restore_from(&missing),
            Err(StoreError::NotFound { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
