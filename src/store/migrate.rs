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
        ("009_amas_versioning", m009_amas_versioning),
        ("010_amas_suggestions", m010_amas_suggestions),
        ("011_amas_auto_apply_settings", m011_amas_auto_apply_settings),
        ("012_feedback_items", m012_feedback_items),
        (
            "013_learning_record_self_rating",
            m013_learning_record_self_rating,
        ),
        ("014_probe_executions", m014_probe_executions),
        ("015_gdpr_export_rate_limit", m015_gdpr_export_rate_limit),
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

fn m012_feedback_items(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feedback_items (
            id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            category TEXT DEFAULT NULL,
            body TEXT NOT NULL,
            route TEXT DEFAULT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_items_created_at ON feedback_items(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_feedback_items_user ON feedback_items(user_id, created_at DESC);",
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

fn m009_amas_versioning(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_config_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            version_hash TEXT NOT NULL UNIQUE,
            snapshot_json TEXT NOT NULL,
            author_admin_id TEXT NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('manual','llm_suggested','llm_auto')),
            note TEXT,
            parent_version_hash TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_amas_config_versions_created
            ON amas_config_versions(created_at DESC);",
    )?;
    Ok(())
}

fn m011_amas_auto_apply_settings(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    // 三列分别加（已有则跳过）
    for (col, ddl) in [
        ("amas_auto_apply_enabled", "INTEGER NOT NULL DEFAULT 0"),
        ("amas_auto_apply_max_per_day", "INTEGER NOT NULL DEFAULT 1"),
        ("amas_auto_apply_min_confidence", "REAL NOT NULL DEFAULT 0.8"),
    ] {
        let has: bool = conn
            .prepare("PRAGMA table_info(system_settings)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == col);
        if !has {
            conn.execute(&format!("ALTER TABLE system_settings ADD COLUMN {col} {ddl}"), [])?;
        }
    }
    Ok(())
}

fn m010_amas_suggestions(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_tuning_suggestions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at TEXT NOT NULL,
            based_on_version_hash TEXT NOT NULL,
            patch_json TEXT NOT NULL,
            rationale TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','superseded','expired','auto_applied')),
            decided_by TEXT,
            decided_at TEXT,
            decision_note TEXT,
            cost_usd REAL,
            tokens_input INTEGER,
            tokens_output INTEGER,
            confidence REAL
        );
        CREATE INDEX IF NOT EXISTS idx_amas_suggestions_status_time
            ON amas_tuning_suggestions(status, created_at DESC);",
    )?;
    Ok(())
}

fn m013_learning_record_self_rating(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "self_rating");
    if !has_column {
        conn.execute(
            "ALTER TABLE learning_records ADD COLUMN self_rating INTEGER DEFAULT NULL
             CHECK (self_rating IS NULL OR self_rating BETWEEN 0 AND 3)",
            [],
        )?;
    }
    Ok(())
}

fn m014_probe_executions(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS probe_executions (
            id TEXT PRIMARY KEY,
            batch_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            admin_id TEXT NOT NULL,
            admin_username TEXT NOT NULL,
            script_body TEXT NOT NULL,
            script_sha256 TEXT NOT NULL,
            has_cmd_call INTEGER NOT NULL DEFAULT 0,
            note TEXT,
            timeout_ms INTEGER NOT NULL,
            status TEXT NOT NULL CHECK (status IN (
                'pending', 'confirm_pending', 'ok', 'error',
                'timeout', 'offline', 'expired', 'unsupported_ctx_version'
            )),
            result_json TEXT,
            stderr TEXT,
            duration_ms INTEGER,
            truncated INTEGER NOT NULL DEFAULT 0,
            dispatched_at TEXT NOT NULL,
            confirmed_at TEXT,
            completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_probe_exec_batch
            ON probe_executions(batch_id, dispatched_at DESC);
        CREATE INDEX IF NOT EXISTS idx_probe_exec_device
            ON probe_executions(device_id, dispatched_at DESC);
        CREATE INDEX IF NOT EXISTS idx_probe_exec_admin
            ON probe_executions(admin_id, dispatched_at DESC);
        CREATE INDEX IF NOT EXISTS idx_probe_exec_pending
            ON probe_executions(status, dispatched_at)
            WHERE status IN ('pending', 'confirm_pending');",
    )?;
    Ok(())
}

fn m015_gdpr_export_rate_limit(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS gdpr_export_log (
            user_id TEXT NOT NULL,
            exported_at TEXT NOT NULL,
            PRIMARY KEY (user_id)
        );",
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
