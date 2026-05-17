use crate::store::{Store, StoreError};

type MigrationFn = fn(&Store) -> Result<(), StoreError>;

fn migrations() -> Vec<(&'static str, MigrationFn)> {
    vec![
        ("001_initial_sqlite", m001_initial),
        ("002_client_management", m002_client_management),
        ("003_telemetry_enhanced", m003_telemetry_enhanced),
        ("004_user_data_interfaces", m004_user_data_interfaces),
        ("005_learning_record_type", m005_learning_record_type),
        ("006_session_shown_words", m006_session_shown_words),
        ("007_session_perf_indexes", m007_session_perf_indexes),
        ("008_admin_analytics_indexes", m008_admin_analytics_indexes),
    ]
}

pub fn run(store: &Store) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    let all = migrations();

    for (index, (name, func)) in all.iter().enumerate() {
        let version = (index + 1) as u32;
        if version > current {
            tracing::info!(version, name, "Running migration");
            func(store)?;
            set_version(store, version)?;
            tracing::info!(version, name, "Migration complete");
        }
    }

    Ok(())
}

pub fn get_current_version(store: &Store) -> Result<u32, StoreError> {
    let conn = store.conn()?;
    let version: u32 = conn
        .query_row(
            "SELECT version FROM schema_version WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(version)
}

pub fn set_version(store: &Store, version: u32) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    if version < current {
        return Err(StoreError::Migration {
            version,
            message: format!("Refuse to downgrade from {} to {}", current, version),
        });
    }
    let conn = store.conn()?;
    conn.execute(
        "INSERT INTO schema_version (singleton_id, version, updated_at)
         VALUES (1, ?1, datetime('now'))
         ON CONFLICT(singleton_id) DO UPDATE SET version = ?1, updated_at = datetime('now')",
        rusqlite::params![version],
    )?;
    Ok(())
}

fn m001_initial(_store: &Store) -> Result<(), StoreError> {
    // DDL is created by init_schema(); this migration just marks the version
    Ok(())
}

fn m002_client_management(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS client_devices (
            device_id TEXT NOT NULL,
            platform TEXT NOT NULL DEFAULT 'unknown',
            user_id TEXT,
            first_seen_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            is_banned INTEGER NOT NULL DEFAULT 0 CHECK (is_banned IN (0, 1)),
            banned_at TEXT DEFAULT NULL,
            banned_by TEXT DEFAULT NULL,
            ban_reason TEXT DEFAULT NULL,
            PRIMARY KEY (device_id)
        );
        CREATE INDEX IF NOT EXISTS idx_client_devices_user ON client_devices(user_id, last_seen_at DESC);
        CREATE INDEX IF NOT EXISTS idx_client_devices_active ON client_devices(last_seen_at DESC) WHERE is_banned = 0;

        CREATE TABLE IF NOT EXISTS telemetry_events (
            id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            user_id TEXT,
            event_type TEXT NOT NULL DEFAULT 'periodic',
            triggered_by_request_id TEXT DEFAULT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            client_ts TEXT NOT NULL,
            server_ts TEXT NOT NULL,
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_telemetry_device ON telemetry_events(device_id, server_ts DESC);
        CREATE INDEX IF NOT EXISTS idx_telemetry_server_ts ON telemetry_events(server_ts DESC);",
    )?;
    Ok(())
}

fn m003_telemetry_enhanced(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS telemetry_summaries (
            id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            user_id TEXT,
            event_type TEXT NOT NULL,
            server_ts TEXT NOT NULL,
            cpu_cores INTEGER,
            memory_gb REAL,
            screen_width INTEGER,
            screen_height INTEGER,
            pixel_ratio REAL,
            os_name TEXT,
            browser_name TEXT,
            browser_version TEXT,
            timezone TEXT,
            language TEXT,
            touch_support INTEGER,
            online_status INTEGER,
            session_duration_secs INTEGER NOT NULL DEFAULT 0,
            actions_per_min REAL NOT NULL DEFAULT 0,
            error_count INTEGER NOT NULL DEFAULT 0,
            avg_response_time_ms REAL NOT NULL DEFAULT 0,
            current_route TEXT,
            click_count INTEGER,
            click_targets_json TEXT,
            scroll_depth_pct REAL,
            visibility_changes INTEGER,
            route_changes INTEGER,
            feature_usage_json TEXT NOT NULL DEFAULT '{}',
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_telemetry_summaries_device
            ON telemetry_summaries(device_id, server_ts DESC);",
    )?;
    Ok(())
}

fn m004_user_data_interfaces(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS word_favorites (
            user_id TEXT NOT NULL,
            word_id TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (user_id, word_id)
        );
        CREATE INDEX IF NOT EXISTS idx_word_favorites_user_created_at
            ON word_favorites(user_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_word_favorites_word_id
            ON word_favorites(word_id);

        CREATE TABLE IF NOT EXISTS word_notes (
            user_id TEXT NOT NULL,
            id TEXT NOT NULL,
            word_id TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (user_id, id)
        );
        CREATE INDEX IF NOT EXISTS idx_word_notes_user_word
            ON word_notes(user_id, word_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_word_notes_word_id
            ON word_notes(word_id);

        CREATE TABLE IF NOT EXISTS wordbook_import_history (
            id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_name TEXT DEFAULT NULL,
            source_url TEXT DEFAULT NULL,
            status TEXT NOT NULL,
            wordbook_id TEXT DEFAULT NULL,
            wordbook_name TEXT DEFAULT NULL,
            words_imported INTEGER DEFAULT NULL,
            words_skipped INTEGER DEFAULT NULL,
            error_message TEXT DEFAULT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_wordbook_import_history_user_created_at
            ON wordbook_import_history(user_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_wordbook_import_history_wordbook
            ON wordbook_import_history(wordbook_id);",
    )?;
    Ok(())
}

fn m005_learning_record_type(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "record_type");
    if !has_column {
        conn.execute(
            "ALTER TABLE learning_records ADD COLUMN record_type TEXT NOT NULL DEFAULT 'all'
             CHECK (record_type IN ('learning', 'review', 'all'))",
            [],
        )?;
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_learning_records_user_type_time
         ON learning_records(user_id, record_type, created_at DESC)",
        [],
    )?;
    Ok(())
}

fn m006_session_shown_words(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_shown_words (
            session_id TEXT NOT NULL,
            word_id TEXT NOT NULL,
            shown_at INTEGER NOT NULL,
            batch_index INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (session_id, word_id)
        );
        CREATE INDEX IF NOT EXISTS idx_ssw_session_batch
            ON session_shown_words(session_id, batch_index);",
    )?;
    Ok(())
}

fn m007_session_perf_indexes(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_ssw_shown_at
            ON session_shown_words(shown_at);
         CREATE INDEX IF NOT EXISTS idx_learning_records_user_session
            ON learning_records(user_id, session_id, created_at);",
    )?;
    Ok(())
}

fn m008_admin_analytics_indexes(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_learning_records_type_time
            ON learning_records(record_type, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_word_favorites_created_at
            ON word_favorites(created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_learning_sessions_created_at
            ON learning_sessions(created_at DESC);",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent() {
        let store = Store::open(":memory:", 5000, 1).unwrap();

        run(&store).unwrap();
        let first = get_current_version(&store).unwrap();
        run(&store).unwrap();
        let second = get_current_version(&store).unwrap();

        let expected = migrations().len() as u32;
        assert_eq!(first, expected);
        assert_eq!(second, expected);
    }

    #[test]
    fn migration_creates_session_shown_words_table() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        run(&store).unwrap();
        let conn = store.conn().unwrap();
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_shown_words'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1);
    }

    #[test]
    fn downgrade_is_rejected() {
        let store = Store::open(":memory:", 5000, 1).unwrap();

        set_version(&store, 3).unwrap();
        let err = set_version(&store, 2).unwrap_err();
        assert!(matches!(err, StoreError::Migration { .. }));
    }

    /// 回归：v0.3.3 部署的 DB 升 v0.4.x 时不应在 init_schema 阶段 fail。
    /// 修复前 init_schema 跑 schema::DDL → 试图建依赖 m005 才有的 record_type
    /// 列的索引，因列不存在直接 panic，根本走不到 run_migrations。
    ///
    /// 修复点：init_schema 仅在 schema_version 表不存在时（即全新 DB）执行 DDL。
    /// 本测试只验证「is_fresh 检测」核心逻辑：构造一个含 schema_version 但缺失
    /// 关键列的 DB，Store::open 应跳过 DDL（不再 panic）；不模拟完整 v0.3.3
    /// schema（避免文件系统级 WAL handle 残留干扰，r2d2 Pool 超时假阳性）。
    #[test]
    fn init_schema_skips_full_ddl_when_schema_version_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("legacy.db");
        let path_str = path.to_str().unwrap();

        // 1. 用 rusqlite 直接构造「最小 v0.3.3 形态」：schema_version 表存在 +
        //    learning_records 表无 record_type 列。这是导致原 bug 的最小复现 DB。
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (
                    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                    version INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                 );
                 INSERT INTO schema_version (singleton_id, version, updated_at)
                 VALUES (1, 4, datetime('now'));
                 CREATE TABLE learning_records (
                    user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    word_id TEXT NOT NULL,
                    is_correct INTEGER NOT NULL DEFAULT 0,
                    response_time_ms INTEGER NOT NULL DEFAULT 0,
                    session_id TEXT DEFAULT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (user_id, id)
                 );",
            )
            .unwrap();
        }

        // 2. Store::open 触发 init_schema：修复前必然 panic（CREATE INDEX 依赖
        //    record_type 列），修复后检测到 schema_version 表存在，跳过 DDL。
        let store = Store::open(path_str, 5000, 1)
            .expect("init_schema must not fail when schema_version exists");

        // 验证 init_schema 确实跳过了全量 DDL：record_type 列仍不存在、
        // 全量 DDL 中创建的 idx_learning_records_user_type_time 索引也不存在。
        let conn = store.conn().unwrap();
        let has_record_type: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('learning_records') WHERE name='record_type'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_record_type, 0, "init_schema 不应在已有 schema_version 表时追加新列");
        let has_index: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_learning_records_user_type_time'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_index, 0, "init_schema 不应跑全量 DDL");
    }

    /// 配套验证：全新 DB 上 init_schema 仍应建立完整目标态 schema。
    #[test]
    fn init_schema_creates_full_ddl_on_fresh_db() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        let conn = store.conn().unwrap();
        // schema.rs::DDL 中的代表性表都应存在。
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('users', 'learning_records', 'schema_version')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3, "fresh DB 上 init_schema 应建立全量目标 schema");
    }
}
