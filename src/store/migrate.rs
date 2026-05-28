//! 数据库迁移注册表与执行器。
//!
//! ## up：单调向前
//! 生产路径只用 [`run`]：根据 `schema_version` 表的当前版本，从 m001 依次向前执行
//! 未跑过的迁移。每个 up 必须幂等（脚本使用 `IF NOT EXISTS` / `PRAGMA table_info`
//! 探测）。
//!
//! ## down：仅用于本地 dev 重置与测试
//! 每个 up 配套一个对应的 down 函数（注册在 [`migrations_down`]），通过 [`revert_to`]
//! 可把 schema 从当前版本回退到目标版本。**严禁**在 production 自动调用；仅限：
//!   - 本地开发时清理脏 schema 以重新跑迁移；
//!   - 集成测试验证迁移的可逆性（up → down → up 循环）。
//!
//! 设计约束：
//! - `m001_initial` 的实体 DDL 由 [`crate::store::Store::open`] 中的 `init_schema()`
//!   在全新 DB 上一次性跑掉，m001 本身仅作 marker；其 down 也必须是 no-op：核心表
//!   （users / learning_records 等）由 schema.rs 拥有，不在迁移回退范围内。
//!   `revert_to(0)` 仅把 m002~mXXX 的副作用全部回退，保留 schema.rs 建的基线 schema
//!   与 `schema_version` 表本身（让下一次 [`run`] 走增量路径而非全量 DDL 路径）。
//! - SQLite `ALTER TABLE DROP COLUMN` 要求列未被 CHECK 表达式 / partial index /
//!   generated 列引用；被引用时须先 `DROP INDEX`。下方 m005/m008 的 down 顺序已
//!   显式处理 `learning_records.record_type` 相关索引；`revert_to` 倒序执行，
//!   保证依赖列的索引先于列本身被丢弃。
//! - 若某 up 引入了 `NOT NULL` 列且承载了用户数据，DROP COLUMN 会丢失数据，down 仅
//!   适合 dev/test。生产不可用 down，因此可接受这种数据损失。

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
        ("016_llm_cost_ledger", m016_llm_cost_ledger),
        ("017_update_audit_log", m017_update_audit_log),
        ("018_feedback_priority_status", m018_feedback_priority_status),
        ("019_worker_last_run", m019_worker_last_run),
        ("020_resource_packs", m020_resource_packs),
        ("021_admin_audit_log_v2", m021_admin_audit_log_v2),
        ("022_admin_ui_completeness", m022_admin_ui_completeness),
    ]
}

/// down 注册表：索引 i 对应 [`migrations`]()[i] 的 inverse。
/// [`revert_to`] 倒序消费此表。
fn migrations_down() -> Vec<(&'static str, MigrationFn)> {
    vec![
        ("001_initial_sqlite", m001_initial_down),
        ("002_client_management", m002_client_management_down),
        ("003_telemetry_enhanced", m003_telemetry_enhanced_down),
        ("004_user_data_interfaces", m004_user_data_interfaces_down),
        ("005_learning_record_type", m005_learning_record_type_down),
        ("006_session_shown_words", m006_session_shown_words_down),
        ("007_session_perf_indexes", m007_session_perf_indexes_down),
        ("008_admin_analytics_indexes", m008_admin_analytics_indexes_down),
        ("009_amas_versioning", m009_amas_versioning_down),
        ("010_amas_suggestions", m010_amas_suggestions_down),
        (
            "011_amas_auto_apply_settings",
            m011_amas_auto_apply_settings_down,
        ),
        ("012_feedback_items", m012_feedback_items_down),
        (
            "013_learning_record_self_rating",
            m013_learning_record_self_rating_down,
        ),
        ("014_probe_executions", m014_probe_executions_down),
        ("015_gdpr_export_rate_limit", m015_gdpr_export_rate_limit_down),
        ("016_llm_cost_ledger", m016_llm_cost_ledger_down),
        ("017_update_audit_log", m017_update_audit_log_down),
        (
            "018_feedback_priority_status",
            m018_feedback_priority_status_down,
        ),
        ("019_worker_last_run", m019_worker_last_run_down),
        ("020_resource_packs", m020_resource_packs_down),
        ("021_admin_audit_log_v2", m021_admin_audit_log_v2_down),
        ("022_admin_ui_completeness", m022_admin_ui_completeness_down),
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
    write_version(store, version)
}

/// 不做"禁止降级"校验的版本写入，供 [`revert_to`] 内部使用。
fn write_version(store: &Store, version: u32) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute(
        "INSERT INTO schema_version (singleton_id, version, updated_at)
         VALUES (1, ?1, datetime('now'))
         ON CONFLICT(singleton_id) DO UPDATE SET version = ?1, updated_at = datetime('now')",
        rusqlite::params![version],
    )?;
    Ok(())
}

/// 把 schema 从当前版本回退到 `target_version`（含）。
///
/// 仅供 dev / test 使用。语义：
///   - 若 `target_version >= current` 则直接返回 `Ok`（无需回退）；
///   - 否则倒序对 `(target_version, current]` 区间每个版本调用其 down 函数；
///   - 每个 down 成功后立即把 `schema_version` 改为 `version - 1`，保证中途失败可见
///     已部分回退的状态。
///
/// `target_version = 0` 表示"全部回退"：m001..mN 的所有 down 都跑一遍，最终
/// `schema_version` 表保留但 version 字段写 0。`schema_version` 表本身不删，让下次
/// [`run`] 走增量路径而非全量 DDL 路径。
pub fn revert_to(store: &Store, target_version: u32) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    if target_version >= current {
        return Ok(());
    }
    let downs = migrations_down();
    let max_idx = (current as usize).min(downs.len());
    // 倒序：current, current-1, ..., target_version + 1
    for version in ((target_version + 1)..=max_idx as u32).rev() {
        let (name, func) = downs[(version - 1) as usize];
        tracing::info!(version, name, "Reverting migration");
        func(store)?;
        write_version(store, version - 1)?;
        tracing::info!(version, name, "Revert complete");
    }
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

fn m017_update_audit_log(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS update_audit_log (
            id           TEXT NOT NULL PRIMARY KEY,
            admin_id     TEXT NOT NULL,
            from_version TEXT NOT NULL,
            to_version   TEXT NOT NULL,
            channel      TEXT NOT NULL,
            started_at   TEXT NOT NULL,
            completed_at TEXT,
            outcome      TEXT NOT NULL DEFAULT 'in_progress',
            error        TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_update_audit_started
            ON update_audit_log(started_at DESC);",
    )?;
    Ok(())
}

/// M1-G2：新建 llm_advisor_cost_ledger 月度成本台账；system_settings 追加月度成本上限列。
fn m016_llm_cost_ledger(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS llm_advisor_cost_ledger (
            month TEXT NOT NULL,
            total_yuan REAL NOT NULL DEFAULT 0.0,
            last_updated_at TEXT NOT NULL,
            PRIMARY KEY (month)
        );",
    )?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "llm_advisor_max_cost_per_month_yuan");
    if !has_col {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN
             llm_advisor_max_cost_per_month_yuan REAL NOT NULL DEFAULT 100.0",
            [],
        )?;
    }
    Ok(())
}


fn m018_feedback_priority_status(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    for (col, ddl) in [
        ("priority", "TEXT NOT NULL DEFAULT 'normal'"),
        ("status", "TEXT NOT NULL DEFAULT 'open'"),
        ("assignee_admin_id", "INTEGER"),
        ("resolved_at", "TEXT"),
        ("resolution", "TEXT"),
    ] {
        let has: bool = conn
            .prepare("PRAGMA table_info(feedback_items)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == col);
        if !has {
            conn.execute(
                &format!("ALTER TABLE feedback_items ADD COLUMN {} {}", col, ddl),
                [],
            )?;
        }
    }
    Ok(())
}

/// M1-A5：worker 执行时间上报表。
fn m019_worker_last_run(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS worker_last_run (
            worker_name      TEXT NOT NULL PRIMARY KEY,
            last_run_at      INTEGER NOT NULL,
            last_duration_ms INTEGER NOT NULL DEFAULT 0,
            last_error       TEXT,
            last_outcome     TEXT NOT NULL CHECK (last_outcome IN ('success','failure','skipped'))
        );",
    )?;
    Ok(())
}

/// v1.1-P0.1：资源包热更四表 —— 元数据、版本、各 channel 激活指针、安装日志。
/// 字段约束对齐 `docs/backend-handoff-resource-pack-v1.1.md` §2 契约。
/// 时间戳沿用 `update_audit_log` 的 TEXT (ISO 8601) 风格。down 由 v1.1-P2.2 统一补。
fn m020_resource_packs(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS resource_packs (
            pack_id      TEXT NOT NULL PRIMARY KEY,
            description  TEXT,
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS resource_pack_versions (
            pack_id         TEXT NOT NULL REFERENCES resource_packs(pack_id) ON DELETE CASCADE,
            version         TEXT NOT NULL,
            sha256          TEXT NOT NULL,
            signature       TEXT,
            signature_alg   TEXT NOT NULL DEFAULT 'ed25519',
            size_bytes      INTEGER NOT NULL,
            min_app_version TEXT,
            channel         TEXT NOT NULL CHECK (channel IN ('stable','beta','internal')),
            payload_path    TEXT NOT NULL,
            published_at    TEXT NOT NULL,
            deactivated_at  TEXT,
            PRIMARY KEY (pack_id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_rpv_channel_pubat
            ON resource_pack_versions(pack_id, channel, published_at DESC);
        CREATE TABLE IF NOT EXISTS resource_pack_active (
            pack_id        TEXT NOT NULL,
            channel        TEXT NOT NULL CHECK (channel IN ('stable','beta','internal')),
            version        TEXT NOT NULL,
            activated_at   TEXT NOT NULL,
            activated_by   TEXT,
            PRIMARY KEY (pack_id, channel),
            FOREIGN KEY (pack_id, version) REFERENCES resource_pack_versions(pack_id, version)
        );
        CREATE TABLE IF NOT EXISTS resource_pack_install_log (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            pack_id       TEXT NOT NULL,
            version       TEXT NOT NULL,
            client_id     TEXT,
            app_version   TEXT,
            installed_at  TEXT NOT NULL,
            outcome       TEXT NOT NULL CHECK (outcome IN ('installed','verify_failed','rollback'))
        );
        CREATE INDEX IF NOT EXISTS idx_rpil_pack_ver
            ON resource_pack_install_log(pack_id, version, installed_at DESC);",
    )?;
    Ok(())
}

/// v1.1-P2.10：把 `update_audit_log` 通用化为「admin 敏感操作审计表」。
///
/// 老语义：仅二进制自更新（from_version → to_version）。
/// 新语义：覆盖资源包上传 / 切激活 / 下架，以及 ban / unban / 重置密码 / 设密码等
/// 一切 admin 高权限操作。
///
/// 兼容策略：
///   - 老 `insert_update_audit` 写入仍只填 from/to/channel，action 默认 'self_update'；
///   - 新 `insert_admin_audit` 写入留空 from/to/channel（空串占位），action 显式给值。
///
/// down 已在 v1.1-P2.2 统一规划，本迁移仅做幂等 ALTER。
fn m021_admin_audit_log_v2(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(update_audit_log)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (col, ddl) in [
        ("action", "TEXT NOT NULL DEFAULT 'self_update'"),
        ("target_type", "TEXT"),
        ("target_id", "TEXT"),
        ("metadata_json", "TEXT"),
    ] {
        if !cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE update_audit_log ADD COLUMN {col} {ddl}"),
                [],
            )?;
        }
    }
    // 新增 action 索引，便于 admin UI 按操作类型筛选
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_update_audit_action
            ON update_audit_log(action, started_at DESC);",
    )?;
    Ok(())
}

// ============================================================================
// down 函数：与上方 up 函数一一对应。仅在 dev / test 由 `revert_to` 调用。
// 每个 down 都用 `DROP TABLE IF EXISTS` / `DROP INDEX IF EXISTS` 保证幂等。
// ALTER TABLE DROP COLUMN 前若列被索引引用，必须先 DROP 那些索引；本文件已显式处理。
// ============================================================================

/// m001 的 up 是 no-op（实际 DDL 由 init_schema 跑 schema.rs），down 也保持 no-op，
/// 避免误删 schema.rs 拥有的核心表。`revert_to(0)` 在此停下。
fn m001_initial_down(_store: &Store) -> Result<(), StoreError> {
    Ok(())
}

fn m002_client_management_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_telemetry_server_ts;
         DROP INDEX IF EXISTS idx_telemetry_device;
         DROP TABLE IF EXISTS telemetry_events;
         DROP INDEX IF EXISTS idx_client_devices_active;
         DROP INDEX IF EXISTS idx_client_devices_user;
         DROP TABLE IF EXISTS client_devices;",
    )?;
    Ok(())
}

fn m003_telemetry_enhanced_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_telemetry_summaries_device;
         DROP TABLE IF EXISTS telemetry_summaries;",
    )?;
    Ok(())
}

fn m004_user_data_interfaces_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_wordbook_import_history_wordbook;
         DROP INDEX IF EXISTS idx_wordbook_import_history_user_created_at;
         DROP TABLE IF EXISTS wordbook_import_history;
         DROP INDEX IF EXISTS idx_word_notes_word_id;
         DROP INDEX IF EXISTS idx_word_notes_user_word;
         DROP TABLE IF EXISTS word_notes;
         DROP INDEX IF EXISTS idx_word_favorites_word_id;
         DROP INDEX IF EXISTS idx_word_favorites_user_created_at;
         DROP TABLE IF EXISTS word_favorites;",
    )?;
    Ok(())
}

/// m005 down：先把所有引用 `learning_records.record_type` 的索引干掉，再 DROP COLUMN。
/// `idx_learning_records_type_time` 由 m008 创建，按理在 m008 down 时已经 DROP；这里
/// 再补一次 DROP IF EXISTS 是为了支持"只回退到版本 4"这种跳过 m008 的边界场景。
fn m005_learning_record_type_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_learning_records_user_type_time;
         DROP INDEX IF EXISTS idx_learning_records_type_time;",
    )?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "record_type");
    if has_column {
        conn.execute("ALTER TABLE learning_records DROP COLUMN record_type", [])?;
    }
    Ok(())
}

fn m006_session_shown_words_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_ssw_session_batch;
         DROP INDEX IF EXISTS idx_ssw_shown_at;
         DROP TABLE IF EXISTS session_shown_words;",
    )?;
    Ok(())
}

/// m007 仅新增两个索引，down 反向 DROP。`idx_ssw_shown_at` 与 m006 自带的索引同表，
/// m006 down 会再 DROP 一次（IF EXISTS 幂等）。
fn m007_session_perf_indexes_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_learning_records_user_session;
         DROP INDEX IF EXISTS idx_ssw_shown_at;",
    )?;
    Ok(())
}

fn m008_admin_analytics_indexes_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_learning_sessions_created_at;
         DROP INDEX IF EXISTS idx_word_favorites_created_at;
         DROP INDEX IF EXISTS idx_learning_records_type_time;",
    )?;
    Ok(())
}

fn m009_amas_versioning_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_amas_config_versions_created;
         DROP TABLE IF EXISTS amas_config_versions;",
    )?;
    Ok(())
}

fn m010_amas_suggestions_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_amas_suggestions_status_time;
         DROP TABLE IF EXISTS amas_tuning_suggestions;",
    )?;
    Ok(())
}

/// m011 down：三个 ALTER ADD COLUMN 的反向。SQLite 支持 DROP COLUMN（3.35+）。
/// 这些列各自带 NOT NULL + DEFAULT，DROP 后任何用户写入的非默认值都会丢失——
/// 仅 dev/test 使用，可接受。
fn m011_amas_auto_apply_settings_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in [
        "amas_auto_apply_min_confidence",
        "amas_auto_apply_max_per_day",
        "amas_auto_apply_enabled",
    ] {
        if cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE system_settings DROP COLUMN {col}"), [])?;
        }
    }
    Ok(())
}

fn m012_feedback_items_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_feedback_items_user;
         DROP INDEX IF EXISTS idx_feedback_items_created_at;
         DROP TABLE IF EXISTS feedback_items;",
    )?;
    Ok(())
}

fn m013_learning_record_self_rating_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "self_rating");
    if has_column {
        conn.execute("ALTER TABLE learning_records DROP COLUMN self_rating", [])?;
    }
    Ok(())
}

fn m014_probe_executions_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_probe_exec_pending;
         DROP INDEX IF EXISTS idx_probe_exec_admin;
         DROP INDEX IF EXISTS idx_probe_exec_device;
         DROP INDEX IF EXISTS idx_probe_exec_batch;
         DROP TABLE IF EXISTS probe_executions;",
    )?;
    Ok(())
}

fn m015_gdpr_export_rate_limit_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS gdpr_export_log;")?;
    Ok(())
}

fn m016_llm_cost_ledger_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "llm_advisor_max_cost_per_month_yuan");
    if has_col {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN llm_advisor_max_cost_per_month_yuan",
            [],
        )?;
    }
    conn.execute_batch("DROP TABLE IF EXISTS llm_advisor_cost_ledger;")?;
    Ok(())
}

fn m017_update_audit_log_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_update_audit_started;
         DROP TABLE IF EXISTS update_audit_log;",
    )?;
    Ok(())
}

fn m018_feedback_priority_status_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feedback_items)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in [
        "resolution",
        "resolved_at",
        "assignee_admin_id",
        "status",
        "priority",
    ] {
        if cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE feedback_items DROP COLUMN {col}"), [])?;
        }
    }
    Ok(())
}

fn m019_worker_last_run_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS worker_last_run;")?;
    Ok(())
}

fn m020_resource_packs_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_rpil_pack_ver;
         DROP TABLE IF EXISTS resource_pack_install_log;
         DROP TABLE IF EXISTS resource_pack_active;
         DROP INDEX IF EXISTS idx_rpv_channel_pubat;
         DROP TABLE IF EXISTS resource_pack_versions;
         DROP TABLE IF EXISTS resource_packs;",
    )?;
    Ok(())
}

/// m022：admin-ui 缺位补齐合集。一次性覆盖 6 项 schema 变化:
///   1. client_devices.app_version —— 设备版本透出（替代仅平台分布）
///   2. users.role / users.status / users.last_login_at —— UserManagementPage Drawer 数据源
///   3. feedback_items.device_profile_json / answer_snapshot_json —— FeedbackPage 上下文快照
///   4. amas_tuning_suggestions.base_values_json —— SuggestionCard 左旧值/右新值 diff
///   5. wordbook_local_tags 新表 —— 词书本地标签覆盖层（远端 metadata 不可写）
///   6. amas_canary_config 新表 —— AMAS 灰度配置（百分比抽样 + 强制白名单）
fn m022_admin_ui_completeness(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 1) client_devices.app_version
    let has_app_version: bool = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "app_version");
    if !has_app_version {
        conn.execute(
            "ALTER TABLE client_devices ADD COLUMN app_version TEXT DEFAULT NULL",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_client_devices_app_version
                 ON client_devices(app_version)
                 WHERE app_version IS NOT NULL",
            [],
        )?;
    }

    // 2) users.role / status / last_login_at
    let user_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (col, ddl) in [
        (
            "role",
            "TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user','staff','admin'))",
        ),
        (
            "status",
            "TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','inactive','suspended'))",
        ),
        ("last_login_at", "TEXT DEFAULT NULL"),
    ] {
        if !user_cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE users ADD COLUMN {col} {ddl}"),
                [],
            )?;
        }
    }

    // 3) feedback_items.device_profile_json / answer_snapshot_json
    let feedback_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feedback_items)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["device_profile_json", "answer_snapshot_json"] {
        if !feedback_cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE feedback_items ADD COLUMN {col} TEXT DEFAULT NULL"),
                [],
            )?;
        }
    }

    // 4) amas_tuning_suggestions.base_values_json
    let has_base_values: bool = conn
        .prepare("PRAGMA table_info(amas_tuning_suggestions)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "base_values_json");
    if !has_base_values {
        conn.execute(
            "ALTER TABLE amas_tuning_suggestions ADD COLUMN base_values_json TEXT DEFAULT NULL",
            [],
        )?;
    }

    // 5) wordbook_local_tags（远端词书的本地标签覆盖层）
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wordbook_local_tags (
            wordbook_id TEXT NOT NULL,
            tag         TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            created_by  TEXT,
            PRIMARY KEY (wordbook_id, tag)
        );
        CREATE INDEX IF NOT EXISTS idx_wordbook_local_tags_tag
            ON wordbook_local_tags(tag);",
    )?;

    // 6) amas_canary_config（百分比抽样 + 强制白名单）
    //    一行配置即可：是否启用 + 候选 version_hash + 比例 + 白名单 JSON。
    //    取代 toggle 模式：每次 PUT 覆盖之前的活跃配置（用 active=1 唯一约束）。
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_canary_config (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            version_hash    TEXT NOT NULL,
            percent         INTEGER NOT NULL CHECK (percent BETWEEN 0 AND 100),
            force_user_ids  TEXT NOT NULL DEFAULT '[]',
            active          INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
            created_at      TEXT NOT NULL,
            created_by      TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS uq_amas_canary_active
            ON amas_canary_config(active) WHERE active = 1;
        CREATE INDEX IF NOT EXISTS idx_amas_canary_created
            ON amas_canary_config(created_at DESC);",
    )?;

    Ok(())
}

/// m021 down：DROP 索引 + 4 个新增列。`action` 是 NOT NULL DEFAULT 列，下次 up 会
/// 重新加回。生产严禁 down，仅 dev/test。
fn m021_admin_audit_log_v2_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP INDEX IF EXISTS idx_update_audit_action;")?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(update_audit_log)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["metadata_json", "target_id", "target_type", "action"] {
        if cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE update_audit_log DROP COLUMN {col}"),
                [],
            )?;
        }
    }
    Ok(())
}

/// m022 down：依序撤销 6 项 schema 变化。新加列 / 新建表均带 DROP IF EXISTS 保证幂等。
fn m022_admin_ui_completeness_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 6) amas_canary_config
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_amas_canary_created;
         DROP INDEX IF EXISTS uq_amas_canary_active;
         DROP TABLE IF EXISTS amas_canary_config;",
    )?;

    // 5) wordbook_local_tags
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_wordbook_local_tags_tag;
         DROP TABLE IF EXISTS wordbook_local_tags;",
    )?;

    // 4) amas_tuning_suggestions.base_values_json
    let has_base_values: bool = conn
        .prepare("PRAGMA table_info(amas_tuning_suggestions)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "base_values_json");
    if has_base_values {
        conn.execute(
            "ALTER TABLE amas_tuning_suggestions DROP COLUMN base_values_json",
            [],
        )?;
    }

    // 3) feedback_items.device_profile_json / answer_snapshot_json
    let feedback_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feedback_items)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["answer_snapshot_json", "device_profile_json"] {
        if feedback_cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE feedback_items DROP COLUMN {col}"),
                [],
            )?;
        }
    }

    // 2) users.last_login_at / status / role
    let user_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["last_login_at", "status", "role"] {
        if user_cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE users DROP COLUMN {col}"), [])?;
        }
    }

    // 1) client_devices.app_version
    conn.execute_batch("DROP INDEX IF EXISTS idx_client_devices_app_version;")?;
    let has_app_version: bool = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "app_version");
    if has_app_version {
        conn.execute("ALTER TABLE client_devices DROP COLUMN app_version", [])?;
    }

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
