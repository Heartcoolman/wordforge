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
        (
            "011_amas_auto_apply_settings",
            m011_amas_auto_apply_settings,
        ),
        ("012_feedback_items", m012_feedback_items),
        (
            "013_learning_record_self_rating",
            m013_learning_record_self_rating,
        ),
        ("014_probe_executions", m014_probe_executions),
        ("015_gdpr_export_rate_limit", m015_gdpr_export_rate_limit),
        ("016_llm_cost_ledger", m016_llm_cost_ledger),
        ("017_update_audit_log", m017_update_audit_log),
        (
            "018_feedback_priority_status",
            m018_feedback_priority_status,
        ),
        ("019_worker_last_run", m019_worker_last_run),
        ("020_resource_packs", m020_resource_packs),
        ("021_admin_audit_log_v2", m021_admin_audit_log_v2),
        ("022_admin_ui_completeness", m022_admin_ui_completeness),
        ("023_user_profile_extras", m023_user_profile_extras),
        ("024_client_extras", m024_client_extras),
        ("025_amas_advisor", m025_amas_advisor),
        ("026_amas_decision_capture", m026_amas_decision_capture),
        ("027_amas_dashboard", m027_amas_dashboard),
        (
            "028_learning_record_question_mode",
            m028_learning_record_question_mode,
        ),
        ("029_wordbook_audit_log", m029_wordbook_audit_log),
        ("030_feedback_ticketing", m030_feedback_ticketing),
        (
            "031_probe_telemetry_sampling",
            m031_probe_telemetry_sampling,
        ),
        ("032_broadcasts_history", m032_broadcasts_history),
        ("033_settings_config", m033_settings_config),
        ("034_admin_rbac_api_keys", m034_admin_rbac_api_keys),
        (
            "035_amas_canary_crowd_filters",
            m035_amas_canary_crowd_filters,
        ),
        ("036_feedback_announcements", m036_feedback_announcements),
        ("037_system_alerts", m037_system_alerts),
        ("038_client_devices_model", m038_client_devices_model),
        ("039_availability_rollup", m039_availability_rollup),
        ("040_canary_thresholds", m040_canary_thresholds),
        ("041_system_alerts_inbox", m041_system_alerts_inbox),
        (
            "042_scheduled_broadcasts_drafts",
            m042_scheduled_broadcasts_drafts,
        ),
        ("043_min_client_version_gate", m043_min_client_version_gate),
        ("044_outbox_event_processing", m044_outbox_event_processing),
        ("045_processed_events", m045_processed_events),
        (
            "046_scheduled_broadcasts_canceled",
            m046_scheduled_broadcasts_canceled,
        ),
        ("047_backup_target_status", m047_backup_target_status),
        (
            "048_scheduled_broadcasts_sending",
            m048_scheduled_broadcasts_sending,
        ),
        (
            "049_wb_center_imports_user_pk",
            m049_wb_center_imports_user_pk,
        ),
        (
            "050_word_elo_user_contrib",
            m050_word_elo_user_contrib,
        ),
        (
            "051_suggestion_status_in_canary",
            m051_suggestion_status_in_canary,
        ),
        (
            "052_whitelist_add_w20_decay",
            m052_whitelist_add_w20_decay,
        ),
        (
            "053_backfill_engine_event_counts",
            m053_backfill_engine_event_counts,
        ),
        ("054_client_devices_risk_flag", m054_client_devices_risk_flag),
        (
            "055_client_devices_fingerprint",
            m055_client_devices_fingerprint,
        ),
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
        (
            "008_admin_analytics_indexes",
            m008_admin_analytics_indexes_down,
        ),
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
        (
            "015_gdpr_export_rate_limit",
            m015_gdpr_export_rate_limit_down,
        ),
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
        ("023_user_profile_extras", m023_user_profile_extras_down),
        ("024_client_extras", m024_client_extras_down),
        ("025_amas_advisor", m025_amas_advisor_down),
        ("026_amas_decision_capture", m026_amas_decision_capture_down),
        ("027_amas_dashboard", m027_amas_dashboard_down),
        (
            "028_learning_record_question_mode",
            m028_learning_record_question_mode_down,
        ),
        ("029_wordbook_audit_log", m029_wordbook_audit_log_down),
        ("030_feedback_ticketing", m030_feedback_ticketing_down),
        (
            "031_probe_telemetry_sampling",
            m031_probe_telemetry_sampling_down,
        ),
        ("032_broadcasts_history", m032_broadcasts_history_down),
        ("033_settings_config", m033_settings_config_down),
        ("034_admin_rbac_api_keys", m034_admin_rbac_api_keys_down),
        (
            "035_amas_canary_crowd_filters",
            m035_amas_canary_crowd_filters_down,
        ),
        (
            "036_feedback_announcements",
            m036_feedback_announcements_down,
        ),
        ("037_system_alerts", m037_system_alerts_down),
        ("038_client_devices_model", m038_client_devices_model_down),
        ("039_availability_rollup", m039_availability_rollup_down),
        ("040_canary_thresholds", m040_canary_thresholds_down),
        ("041_system_alerts_inbox", m041_system_alerts_inbox_down),
        (
            "042_scheduled_broadcasts_drafts",
            m042_scheduled_broadcasts_drafts_down,
        ),
        (
            "043_min_client_version_gate",
            m043_min_client_version_gate_down,
        ),
        (
            "044_outbox_event_processing",
            m044_outbox_event_processing_down,
        ),
        ("045_processed_events", m045_processed_events_down),
        (
            "046_scheduled_broadcasts_canceled",
            m046_scheduled_broadcasts_canceled_down,
        ),
        (
            "047_backup_target_status",
            m047_backup_target_status_down,
        ),
        (
            "048_scheduled_broadcasts_sending",
            m048_scheduled_broadcasts_sending_down,
        ),
        (
            "049_wb_center_imports_user_pk",
            m049_wb_center_imports_user_pk_down,
        ),
        (
            "050_word_elo_user_contrib",
            m050_word_elo_user_contrib_down,
        ),
        (
            "051_suggestion_status_in_canary",
            m051_suggestion_status_in_canary_down,
        ),
        (
            "052_whitelist_add_w20_decay",
            m052_whitelist_add_w20_decay_down,
        ),
        (
            "053_backfill_engine_event_counts",
            m053_backfill_engine_event_counts_down,
        ),
        (
            "054_client_devices_risk_flag",
            m054_client_devices_risk_flag_down,
        ),
        (
            "055_client_devices_fingerprint",
            m055_client_devices_fingerprint_down,
        ),
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

/// 全量迁移条数，即完全迁移后 `schema_version` 应到达的版本号。
pub fn migration_count() -> usize {
    migrations().len()
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
        (
            "amas_auto_apply_min_confidence",
            "REAL NOT NULL DEFAULT 0.8",
        ),
    ] {
        let has: bool = conn
            .prepare("PRAGMA table_info(system_settings)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == col);
        if !has {
            conn.execute(
                &format!("ALTER TABLE system_settings ADD COLUMN {col} {ddl}"),
                [],
            )?;
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
            conn.execute(
                &format!("ALTER TABLE system_settings DROP COLUMN {col}"),
                [],
            )?;
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
            conn.execute(&format!("ALTER TABLE users ADD COLUMN {col} {ddl}"), [])?;
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
            conn.execute(&format!("ALTER TABLE feedback_items DROP COLUMN {col}"), [])?;
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

/// m023:用户档案补全 —— 设计图 Drawer "资料 / 答题 / 设备 / 操作日志" 完整化。
///
/// 新增:
///   1) `user_activity_log`(用户**自有**活动日志,区别于 admin_audit_log)
///   2) `users.referrer_source`(注册来源)
///   3) `user_elo.sigma`(ELO 1432 ± 86 的 sigma)
///   4) `habit_profiles.daily_goal_words / daily_goal_minutes`(每日目标)
fn m023_user_profile_extras(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 1) user_activity_log
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_activity_log (
            id           TEXT NOT NULL,
            user_id      TEXT NOT NULL,
            action       TEXT NOT NULL,
            detail_json  TEXT,
            ip           TEXT,
            created_at   TEXT NOT NULL,
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_user_activity_user_time
            ON user_activity_log(user_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_user_activity_action
            ON user_activity_log(action, created_at DESC);",
    )?;

    // 2) users.referrer_source
    let user_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !user_cols.iter().any(|c| c == "referrer_source") {
        conn.execute(
            "ALTER TABLE users ADD COLUMN referrer_source TEXT DEFAULT NULL",
            [],
        )?;
    }

    // 3) user_elo.sigma
    let elo_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(user_elo)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !elo_cols.iter().any(|c| c == "sigma") {
        conn.execute(
            "ALTER TABLE user_elo ADD COLUMN sigma REAL NOT NULL DEFAULT 86.0",
            [],
        )?;
    }

    // 4) habit_profiles.daily_goal_words / daily_goal_minutes
    let habit_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(habit_profiles)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (col, ddl) in [
        ("daily_goal_words", "INTEGER NOT NULL DEFAULT 30"),
        ("daily_goal_minutes", "INTEGER NOT NULL DEFAULT 25"),
    ] {
        if !habit_cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE habit_profiles ADD COLUMN {col} {ddl}"),
                [],
            )?;
        }
    }

    Ok(())
}

/// m023 down:撤销 user_profile_extras 全部 4 项变更。
fn m023_user_profile_extras_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 4) habit_profiles
    let habit_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(habit_profiles)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["daily_goal_minutes", "daily_goal_words"] {
        if habit_cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE habit_profiles DROP COLUMN {col}"), [])?;
        }
    }

    // 3) user_elo.sigma
    let elo_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(user_elo)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if elo_cols.iter().any(|c| c == "sigma") {
        conn.execute("ALTER TABLE user_elo DROP COLUMN sigma", [])?;
    }

    // 2) users.referrer_source
    let user_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if user_cols.iter().any(|c| c == "referrer_source") {
        conn.execute("ALTER TABLE users DROP COLUMN referrer_source", [])?;
    }

    // 1) user_activity_log
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_user_activity_action;
         DROP INDEX IF EXISTS idx_user_activity_user_time;
         DROP TABLE IF EXISTS user_activity_log;",
    )?;

    Ok(())
}

/// m024:设备页对齐设计图 clients.html 所需补强 ——
///   1) `client_devices.country`(GeoIP ISO-3166-1 alpha-2)
///   2) `client_devices.last_ip`(便于审计、变更检测)
///   3) `client_devices(platform, last_seen_at)` 复合索引(平台聚合 + 月环比加速)
///   4) `client_upgrade_policy` 新表 + 三平台 seed 行(web/ios/android)
fn m024_client_extras(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    let cd_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["country", "last_ip"] {
        if !cd_cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE client_devices ADD COLUMN {col} TEXT DEFAULT NULL"),
                [],
            )?;
        }
    }

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_client_devices_platform
            ON client_devices(platform, last_seen_at DESC);

         CREATE TABLE IF NOT EXISTS client_upgrade_policy (
             platform           TEXT NOT NULL,
             min_version        TEXT,
             suggested_version  TEXT,
             grayscale_pct      INTEGER NOT NULL DEFAULT 0
                                CHECK (grayscale_pct BETWEEN 0 AND 100),
             pwa_silent_update  INTEGER NOT NULL DEFAULT 1
                                CHECK (pwa_silent_update IN (0, 1)),
             updated_at         TEXT NOT NULL,
             updated_by         TEXT,
             PRIMARY KEY (platform)
         );

         INSERT OR IGNORE INTO client_upgrade_policy (platform, updated_at)
             VALUES ('web', datetime('now')),
                    ('ios', datetime('now')),
                    ('android', datetime('now'));",
    )?;

    Ok(())
}

/// m024 down:DROP 顺序 — 表 → 复合索引 → 两列;两列借 SQLite ALTER DROP 单删。
fn m024_client_extras_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    conn.execute_batch(
        "DROP TABLE IF EXISTS client_upgrade_policy;
         DROP INDEX IF EXISTS idx_client_devices_platform;",
    )?;

    let cd_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["last_ip", "country"] {
        if cd_cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE client_devices DROP COLUMN {col}"), [])?;
        }
    }

    Ok(())
}

/// m025:amas-advisor 全栈对齐所需数据模型 ——
///   1) amas_tuning_whitelist 新表(LLM 调参白名单,启动 seed 自 TIER_A_WHITELIST)
///   2) amas_patch_canary 新表(per-patch 真灰度,多条 active,cohort [lo,hi) 不重叠)
///   3) system_settings.llm_advisor_enabled 列(运行时巡查开关)
fn m025_amas_advisor(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS amas_tuning_whitelist (
            path        TEXT NOT NULL,
            min_safe    REAL NOT NULL,
            max_safe    REAL NOT NULL,
            created_at  TEXT NOT NULL,
            created_by  TEXT NOT NULL,
            PRIMARY KEY (path)
        );

        CREATE TABLE IF NOT EXISTS amas_patch_canary (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            suggestion_id         INTEGER NOT NULL,
            version_hash          TEXT NOT NULL,
            percent               INTEGER NOT NULL CHECK (percent BETWEEN 0 AND 100),
            cohort_lo             INTEGER NOT NULL CHECK (cohort_lo BETWEEN 0 AND 100),
            cohort_hi             INTEGER NOT NULL CHECK (cohort_hi BETWEEN 0 AND 100),
            status                TEXT NOT NULL DEFAULT 'active'
                                  CHECK (status IN ('active','effective','rolled_back')),
            baseline_metrics_json TEXT NOT NULL,
            started_at            TEXT NOT NULL,
            updated_at            TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_active
            ON amas_patch_canary(status) WHERE status = 'active';
        CREATE INDEX IF NOT EXISTS idx_amas_patch_canary_started
            ON amas_patch_canary(started_at DESC);",
    )?;

    // system_settings.llm_advisor_enabled —— 列守卫(幂等)
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "llm_advisor_enabled");
    if !has_col {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN llm_advisor_enabled
                INTEGER NOT NULL DEFAULT 0 CHECK (llm_advisor_enabled IN (0, 1))",
            [],
        )?;
    }

    // C3:system_settings.amas_grayscale_steps —— 灰度策略三档,逗号分隔字符串列(幂等)
    let has_steps: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "amas_grayscale_steps");
    if !has_steps {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN amas_grayscale_steps
                TEXT NOT NULL DEFAULT '20,60,100'",
            [],
        )?;
    }

    Ok(())
}

/// m025 down:DROP 两表;llm_advisor_enabled 列借 SQLite ALTER DROP 单删。生产严禁 down。
fn m025_amas_advisor_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS amas_patch_canary;
         DROP TABLE IF EXISTS amas_tuning_whitelist;",
    )?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "llm_advisor_enabled");
    if has_col {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN llm_advisor_enabled",
            [],
        )?;
    }
    let has_steps: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "amas_grayscale_steps");
    if has_steps {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN amas_grayscale_steps",
            [],
        )?;
    }
    Ok(())
}

/// m026:engine_monitoring_events 决策埋点三列 —— routing_algo / routing_weights_json /
/// is_correct。生产 INSERT 此前只写 6 列、14 个专用列恒为 DEFAULT，本迁移配合 INSERT 重写
/// 让决策算法路由分布与首答对错可被 SQL 聚合（甜甜圈分布 / ensemble 路由占比 / 命中率）。
/// 列守卫保证幂等。
fn m026_amas_decision_capture(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(engine_monitoring_events)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "routing_algo") {
        conn.execute(
            "ALTER TABLE engine_monitoring_events ADD COLUMN routing_algo TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "routing_weights_json") {
        conn.execute(
            "ALTER TABLE engine_monitoring_events ADD COLUMN routing_weights_json TEXT NOT NULL DEFAULT '{}'",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "is_correct") {
        conn.execute(
            "ALTER TABLE engine_monitoring_events ADD COLUMN is_correct INTEGER NOT NULL DEFAULT 0 CHECK (is_correct IN (0, 1))",
            [],
        )?;
    }
    Ok(())
}

/// m026 down:SQLite 支持 ALTER TABLE DROP COLUMN（仅 dev/test，生产严禁 down）。
fn m026_amas_decision_capture_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(engine_monitoring_events)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["is_correct", "routing_weights_json", "routing_algo"] {
        if cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE engine_monitoring_events DROP COLUMN {col}"),
                [],
            )?;
        }
    }
    Ok(())
}

/// m027:AMAS 指标看板 —— ELO 日快照表，供"用户 ELO 散点"的 7 天 Δ ELO 着色。
/// `user_elo` 仅存当前 rating，无历史；daily_aggregation worker 每天写一行快照，
/// 散点端点用 `today.rating - rating_7d_ago` 计算颜色档（无历史时 Δ=0 取中性色）。
/// 阶段分布 7 天趋势复用既有 `monitoring_timeseries` 表，无需额外建表。
fn m027_amas_dashboard(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_elo_history (
            user_id       TEXT NOT NULL,
            snapshot_date TEXT NOT NULL,
            rating        REAL NOT NULL,
            games         INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (user_id, snapshot_date)
        );
        CREATE INDEX IF NOT EXISTS idx_user_elo_history_date
            ON user_elo_history(snapshot_date);",
    )?;
    Ok(())
}

/// m027 down:DROP user_elo_history。仅 dev/test。
fn m027_amas_dashboard_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_user_elo_history_date;
         DROP TABLE IF EXISTS user_elo_history;",
    )?;
    Ok(())
}

/// m028:learning_records 增加 question_mode（出题模式）列，供数据分析"答题分布·题型"使用。
/// 约定取值 word-to-meaning / meaning-to-word / audio-to-meaning / meaning-to-spelling；
/// 可空、无 CHECK（非法值原样存，聚合端按映射处理），历史与默认为 NULL → 聚合端按"未标注"。
/// 列守卫保证幂等。
fn m028_learning_record_question_mode(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "question_mode");
    if !has_column {
        conn.execute(
            "ALTER TABLE learning_records ADD COLUMN question_mode TEXT DEFAULT NULL",
            [],
        )?;
    }
    Ok(())
}

/// m028 down:DROP question_mode 列。该列无索引引用，可直接 DROP。仅 dev/test。
fn m028_learning_record_question_mode_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_column: bool = conn
        .prepare("PRAGMA table_info(learning_records)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "question_mode");
    if has_column {
        conn.execute("ALTER TABLE learning_records DROP COLUMN question_mode", [])?;
    }
    Ok(())
}

/// m029:词库管理审计日志表,供 admin 词库中心记录 create/update/delete/add_word/
/// remove_word/import/sync 操作。IF NOT EXISTS 保证幂等。
fn m029_wordbook_audit_log(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS wordbook_audit_log (
            id TEXT NOT NULL,
            wordbook_id TEXT NOT NULL,
            action TEXT NOT NULL,
            detail TEXT NOT NULL DEFAULT '',
            admin_id TEXT DEFAULT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (id)
        );
        CREATE INDEX IF NOT EXISTS idx_wordbook_audit_wordbook
            ON wordbook_audit_log(wordbook_id, created_at DESC);",
    )?;
    Ok(())
}

/// m029 down:DROP wordbook_audit_log。仅 dev/test。
fn m029_wordbook_audit_log_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_wordbook_audit_wordbook;
         DROP TABLE IF EXISTS wordbook_audit_log;",
    )?;
    Ok(())
}

/// m030:反馈工单化 —— feedback_items 增 6 列（read_at/first_response_at/csat_score/
/// csat_comment/dedup_count/github_issue_url）+ 三张新表（回复 / 时间线事件 / 附件）。
/// 列守卫保证幂等;新表 IF NOT EXISTS。
fn m030_feedback_ticketing(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feedback_items)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (col, ddl) in [
        ("read_at", "TEXT"),
        ("first_response_at", "TEXT"),
        ("csat_score", "INTEGER"),
        ("csat_comment", "TEXT"),
        ("dedup_count", "INTEGER NOT NULL DEFAULT 0"),
        ("github_issue_url", "TEXT"),
    ] {
        if !cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE feedback_items ADD COLUMN {col} {ddl}"),
                [],
            )?;
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feedback_replies (
            id TEXT NOT NULL PRIMARY KEY,
            feedback_id TEXT NOT NULL,
            author_kind TEXT NOT NULL,
            author_id TEXT,
            body TEXT NOT NULL,
            push_inapp INTEGER NOT NULL DEFAULT 0,
            cc_email INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_replies_fb
            ON feedback_replies(feedback_id, created_at);

        CREATE TABLE IF NOT EXISTS feedback_events (
            id TEXT NOT NULL PRIMARY KEY,
            feedback_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            actor TEXT,
            summary TEXT NOT NULL DEFAULT '',
            ref_id TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_events_fb
            ON feedback_events(feedback_id, created_at);

        CREATE TABLE IF NOT EXISTS feedback_attachments (
            id TEXT NOT NULL PRIMARY KEY,
            feedback_id TEXT NOT NULL,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'image',
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_attachments_fb
            ON feedback_attachments(feedback_id);",
    )?;
    Ok(())
}

/// m030 down:DROP 三张新表;feedback_items 6 个增列借 SQLite ALTER DROP 单删。仅 dev/test。
fn m030_feedback_ticketing_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_feedback_attachments_fb;
         DROP TABLE IF EXISTS feedback_attachments;
         DROP INDEX IF EXISTS idx_feedback_events_fb;
         DROP TABLE IF EXISTS feedback_events;
         DROP INDEX IF EXISTS idx_feedback_replies_fb;
         DROP TABLE IF EXISTS feedback_replies;",
    )?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feedback_items)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in [
        "github_issue_url",
        "dedup_count",
        "csat_comment",
        "csat_score",
        "first_response_at",
        "read_at",
    ] {
        if cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE feedback_items DROP COLUMN {col}"), [])?;
        }
    }
    Ok(())
}

/// m031:数据探针看板采样配置 ——
///   1) system_settings.telemetry_sample_rate（全局默认采样率，[0,1]）
///   2) probe_sampling_config 新表（按 telemetry event_type 主键的采样规则）
///   3) probe_sampling_audit 新表（每次 PATCH 成功落一行）
///
/// seed:'*'=1.0（兜底）/'periodic'=1.0（看板 click 行滑杆绑定）/
/// 'on_demand' 与 'session_start' locked=1 恒 1.0（核心数据强制 100%）。
/// 默认全 1.0 → 采样注入零行为变化。
fn m031_probe_telemetry_sampling(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 1) system_settings.telemetry_sample_rate（列守卫幂等）
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "telemetry_sample_rate");
    if !has_col {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN telemetry_sample_rate
                REAL NOT NULL DEFAULT 1.0",
            [],
        )?;
    }

    // 2)+3) 两张新表 + 索引
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS probe_sampling_config (
            event_type  TEXT NOT NULL PRIMARY KEY,
            sample_rate REAL NOT NULL DEFAULT 1.0 CHECK (sample_rate BETWEEN 0.0 AND 1.0),
            enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
            locked      INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
            priority    INTEGER NOT NULL DEFAULT 100,
            updated_at  TEXT NOT NULL,
            updated_by  TEXT
        );

        CREATE TABLE IF NOT EXISTS probe_sampling_audit (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type  TEXT NOT NULL,
            action      TEXT NOT NULL CHECK (action IN ('add', 'mod', 'del', 'pause')),
            old_value   TEXT,
            new_value   TEXT,
            admin_id    TEXT,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_probe_sampling_audit_time
            ON probe_sampling_audit(created_at DESC);",
    )?;

    // seed:'*' 兜底 + 'periodic' 真实 gate + 两个 locked 核心事件。
    // priority 越小越优先(命中行 > '*' 行)。locked 行 sample_rate 恒 1.0。
    conn.execute_batch(
        "INSERT OR IGNORE INTO probe_sampling_config
            (event_type, sample_rate, enabled, locked, priority, updated_at, updated_by)
         VALUES
            ('*',             1.0, 1, 0, 1000, datetime('now'), 'seed'),
            ('periodic',      1.0, 1, 0,  100, datetime('now'), 'seed'),
            ('on_demand',     1.0, 1, 1,   10, datetime('now'), 'seed'),
            ('session_start', 1.0, 1, 1,   10, datetime('now'), 'seed');",
    )?;

    Ok(())
}

/// m031 down:DROP 两表 + telemetry_sample_rate 列。仅 dev/test。
fn m031_probe_telemetry_sampling_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_probe_sampling_audit_time;
         DROP TABLE IF EXISTS probe_sampling_audit;
         DROP TABLE IF EXISTS probe_sampling_config;",
    )?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "telemetry_sample_rate");
    if has_col {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN telemetry_sample_rate",
            [],
        )?;
    }
    Ok(())
}

/// m032:系统广播历史表 —— 设计稿 broadcast.html 依赖的"近 30 天广播列表 + 统计"。
/// 每次发送一条广播写一行;已读/发送量供看板算 readRate。
fn m032_broadcasts_history(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS broadcasts (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            message     TEXT NOT NULL,
            admin_id    TEXT NOT NULL,
            sent_count  INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_broadcasts_created_at
            ON broadcasts(created_at DESC);",
    )?;
    Ok(())
}

/// m032 down:DROP 索引 + broadcasts 表。仅 dev/test。
fn m032_broadcasts_history_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_broadcasts_created_at;
         DROP TABLE IF EXISTS broadcasts;",
    )?;
    Ok(())
}

/// m033:可扩展设置配置存储 + 快照表。settings.html 的 11 个面板各按 section 持久化
/// 为 JSON;快照表存全部 section 的聚合用于 rollback。
fn m033_settings_config(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings_config (
            section     TEXT PRIMARY KEY,
            json        TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings_snapshots (
            id          TEXT PRIMARY KEY,
            label       TEXT,
            json        TEXT NOT NULL,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_settings_snapshots_created_at
            ON settings_snapshots(created_at DESC);",
    )?;
    Ok(())
}

/// m033 down:DROP 索引 + 两张设置表。仅 dev/test。
fn m033_settings_config_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_settings_snapshots_created_at;
         DROP TABLE IF EXISTS settings_snapshots;
         DROP TABLE IF EXISTS settings_config;",
    )?;
    Ok(())
}

/// m034:settings.html「管理员与角色」+「API 密钥」落地。
///   1. admins.role —— RBAC 角色('super_admin' / 'admin'),既有行默认 'super_admin'
///      (首个/历史 admin 视为超管,避免迁移后无人能管角色)。
///   2. api_keys —— 服务端集成密钥。明文仅生成时返回一次;库里只存 argon2 hash + 前缀掩码。
fn m034_admin_rbac_api_keys(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;

    // 1) admins.role(历史行回填 super_admin)
    let has_role: bool = conn
        .prepare("PRAGMA table_info(admins)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "role");
    if !has_role {
        conn.execute(
            "ALTER TABLE admins ADD COLUMN role TEXT NOT NULL DEFAULT 'super_admin'
             CHECK (role IN ('super_admin','admin'))",
            [],
        )?;
    }

    // 2) api_keys
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS api_keys (
            id           TEXT NOT NULL PRIMARY KEY,
            name         TEXT NOT NULL,
            scope        TEXT NOT NULL CHECK (scope IN ('read','write','admin')),
            prefix       TEXT NOT NULL,
            hash         TEXT NOT NULL,
            created_at   TEXT NOT NULL,
            created_by   TEXT,
            expires_at   TEXT,
            last_used_at TEXT,
            revoked_at   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_api_keys_created
            ON api_keys(created_at DESC);",
    )?;
    Ok(())
}

fn m034_admin_rbac_api_keys_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_api_keys_created;
         DROP TABLE IF EXISTS api_keys;",
    )?;
    let has_role: bool = conn
        .prepare("PRAGMA table_info(admins)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "role");
    if has_role {
        conn.execute("ALTER TABLE admins DROP COLUMN role", [])?;
    }
    Ok(())
}

/// m035:灰度发布人群过滤(crowd-filter)——amas_canary_config 增 crowd_filters TEXT 列
/// (JSON: minAccountAgeDays/preferActive/webOnly/autoScale24h)。列守卫幂等。
fn m035_amas_canary_crowd_filters(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(amas_canary_config)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "crowd_filters");
    if !has_col {
        conn.execute(
            "ALTER TABLE amas_canary_config ADD COLUMN crowd_filters TEXT",
            [],
        )?;
    }
    Ok(())
}

/// m036:反馈中心公告/FAQ（feedback.html "新建公告 / FAQ" 入口，存为草稿）
/// + 回复草稿持久化（composer "存为草稿" 按钮，每工单一份）。两张新表，IF NOT EXISTS 幂等。
fn m036_feedback_announcements(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feedback_announcements (
            id          TEXT NOT NULL PRIMARY KEY,
            title       TEXT NOT NULL,
            body        TEXT NOT NULL,
            kind        TEXT NOT NULL DEFAULT 'announcement'
                        CHECK (kind IN ('announcement', 'faq')),
            published   INTEGER NOT NULL DEFAULT 0 CHECK (published IN (0, 1)),
            author_id   TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_announcements_kind
            ON feedback_announcements(kind, created_at DESC);

        CREATE TABLE IF NOT EXISTS feedback_reply_drafts (
            feedback_id TEXT NOT NULL PRIMARY KEY,
            body        TEXT NOT NULL,
            push_inapp  INTEGER NOT NULL DEFAULT 0,
            cc_email    INTEGER NOT NULL DEFAULT 0,
            author_id   TEXT,
            updated_at  TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// m036 down:DROP 两表 + 索引。仅 dev/test。
fn m036_feedback_announcements_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS feedback_reply_drafts;
         DROP INDEX IF EXISTS idx_feedback_announcements_kind;
         DROP TABLE IF EXISTS feedback_announcements;",
    )?;
    Ok(())
}

/// m037:系统告警表(AMAS 数据软拦截告警的可写载体)。admin 无应用内通知箱,
/// 失败告警落此表 + /api/admin/monitoring/events 时间线透出。
/// dedup key=(source,kind):同源同类失败合并计数,防 worker 周期失败打爆表。
fn m037_system_alerts(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS system_alerts (
            id            TEXT NOT NULL,
            source        TEXT NOT NULL,
            kind          TEXT NOT NULL,
            severity      TEXT NOT NULL,
            title         TEXT NOT NULL,
            message       TEXT NOT NULL DEFAULT '',
            count         INTEGER NOT NULL DEFAULT 1,
            first_seen_at TEXT NOT NULL,
            last_seen_at  TEXT NOT NULL,
            PRIMARY KEY (id),
            UNIQUE (source, kind)
        );
        CREATE INDEX IF NOT EXISTS idx_system_alerts_last_seen
            ON system_alerts(last_seen_at DESC);",
    )?;
    Ok(())
}

/// m037 down:DROP 表 + 索引。仅 dev/test。
fn m037_system_alerts_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_system_alerts_last_seen;
         DROP TABLE IF EXISTS system_alerts;",
    )?;
    Ok(())
}

/// m038:client_devices 加 model 列(遥测硬识别上报的设备型号)。幂等加列。
fn m038_client_devices_model(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "model") {
        conn.execute(
            "ALTER TABLE client_devices ADD COLUMN model TEXT DEFAULT NULL",
            [],
        )?;
    }
    Ok(())
}

/// m038 down:DROP model 列。仅 dev/test。
fn m038_client_devices_model_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if cols.iter().any(|c| c == "model") {
        conn.execute("ALTER TABLE client_devices DROP COLUMN model", [])?;
    }
    Ok(())
}

/// m039:HTTP 可用率小时滚动桶持久化(D3)。让登录页 SLO 30d 跨重启可达——
/// 内存 RollingStore 重启清零且 HOUR_CAP 原仅 7d,本表按小时落 5xx/total(+延迟桶)
/// 供启动回灌 + 每 5 分钟 flush。hour_key=unix/3600,buckets 为 JSON 数组。
fn m039_availability_rollup(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS availability_rollup (
            hour_key  INTEGER NOT NULL PRIMARY KEY,
            count     INTEGER NOT NULL DEFAULT 0,
            err5xx    INTEGER NOT NULL DEFAULT 0,
            bytes_in  INTEGER NOT NULL DEFAULT 0,
            buckets   TEXT    NOT NULL DEFAULT '[]'
        );",
    )?;
    Ok(())
}

/// m039 down:DROP 表。仅 dev/test。
fn m039_availability_rollup_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS availability_rollup;")?;
    Ok(())
}

/// m044:领域事件 outbox + 死信表(S2-1)。outbox 持久化 records→AMAS 领域事件,供
/// outbox_processor worker 异步消费(指数退避重试 + 死信兜底)。默认 records 仍走同步
/// 老路(RECORDS_OUTBOX_ASYNC=false),异步路径 opt-in;切默认/删手动 rollback 待跨仓协同。
fn m044_outbox_event_processing(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS outbox (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type    TEXT NOT NULL,
            payload       TEXT NOT NULL,
            attempts      INTEGER NOT NULL DEFAULT 0,
            next_retry_at TEXT NOT NULL,
            last_error    TEXT,
            created_at    TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_outbox_due ON outbox(next_retry_at);

        CREATE TABLE IF NOT EXISTS events_dead_letter (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type  TEXT NOT NULL,
            payload     TEXT NOT NULL,
            attempts    INTEGER NOT NULL,
            last_error  TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL,
            dead_at     TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_dead_letter_dead_at ON events_dead_letter(dead_at DESC);",
    )?;
    Ok(())
}

/// m044 down:DROP 两表 + 索引。仅 dev/test。
fn m044_outbox_event_processing_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_dead_letter_dead_at;
         DROP TABLE IF EXISTS events_dead_letter;
         DROP INDEX IF EXISTS idx_outbox_due;
         DROP TABLE IF EXISTS outbox;",
    )?;
    Ok(())
}

/// m045:records→AMAS 领域事件幂等账本(W1-1)。以 (user_id, client_record_id) 为主键标记
/// "此事件的 AMAS 状态已应用"。标记与 AMAS 状态在 persist_engine_state_atomic 同 tx 原子提交,
/// 路由在 process_event 前预检命中即短路跳过 AMAS,把"重启不丢"补成 AMAS"精确一次"——
/// 为后续删手动 rollback + 切默认 async 铺第一块基石。默认同步老路下本表仍随每条记录写入。
fn m045_processed_events(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS processed_events (
            user_id          TEXT NOT NULL,
            client_record_id TEXT NOT NULL,
            processed_at     TEXT NOT NULL,
            PRIMARY KEY (user_id, client_record_id)
        );",
    )?;
    Ok(())
}

/// m045 down:DROP 幂等账本表。仅 dev/test。
fn m045_processed_events_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS processed_events;")?;
    Ok(())
}

/// m046:scheduled_broadcasts 的 status CHECK 加 'canceled' 枚举(W2-2 定时广播取消)。
/// SQLite 无法 ALTER CHECK 约束,须重建表(CREATE new + INSERT SELECT + DROP + RENAME)。
/// 列定义与 m042 一致,仅 CHECK 增 'canceled';索引重建。一次性迁移,版本门控保证只跑一次。
fn m046_scheduled_broadcasts_canceled(store: &Store) -> Result<(), StoreError> {
    // 原子表重建:整批 DDL 包在单事务内,迁移中途硬崩溃(OOM/SIGKILL)时 SQLite 回滚不留半成品,
    // 重启重跑可干净恢复。开头 DROP IF EXISTS _new 清除"修复前的旧版本"可能遗留的残留临时表,
    // 双保险闭合"CREATE TABLE _new 因表已存在而失败 → 启动 boot loop"的 brick 窗口。
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS scheduled_broadcasts_new;
        CREATE TABLE scheduled_broadcasts_new (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL,
            message           TEXT NOT NULL,
            admin_id          TEXT NOT NULL,
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            user_ids          TEXT,
            scheduled_at      TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'sent', 'failed', 'canceled')),
            sent_count        INTEGER,
            error             TEXT,
            created_at        TEXT NOT NULL,
            sent_at           TEXT
        );
        INSERT INTO scheduled_broadcasts_new
            (id, title, message, admin_id, platforms, version_min, last_active_days,
             user_ids, scheduled_at, status, sent_count, error, created_at, sent_at)
        SELECT id, title, message, admin_id, platforms, version_min, last_active_days,
               user_ids, scheduled_at, status, sent_count, error, created_at, sent_at
          FROM scheduled_broadcasts;
        DROP TABLE scheduled_broadcasts;
        ALTER TABLE scheduled_broadcasts_new RENAME TO scheduled_broadcasts;
        CREATE INDEX IF NOT EXISTS idx_scheduled_broadcasts_due
            ON scheduled_broadcasts(status, scheduled_at);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m047:离站备份各 target 运行时状态(W2-4 可观测)。与 backup-policy 配置 section 解耦——
/// 后者是 admin 可编辑配置,运行时状态混入会与 PATCH 覆写冲突,故独立轻量状态表。
/// 每 target 记 last_ok_at/last_bytes(成功)与 last_error/last_attempt_at,admin settings
/// BackupRenderer 据此把灾备从「配了不知有没有用」变为可验证。
fn m047_backup_target_status(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS backup_target_status (
            name             TEXT NOT NULL PRIMARY KEY,
            uri              TEXT NOT NULL,
            last_ok_at       TEXT,
            last_bytes       INTEGER,
            last_error       TEXT,
            last_attempt_at  TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// m047 down:DROP 状态表。仅 dev/test。
fn m047_backup_target_status_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS backup_target_status;")?;
    Ok(())
}

/// m046 down:重建回不含 'canceled' 的 CHECK。仅 dev/test;已 canceled 的行会被丢弃
///（否则触发旧 CHECK 违反）。
fn m046_scheduled_broadcasts_canceled_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE scheduled_broadcasts_old (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL,
            message           TEXT NOT NULL,
            admin_id          TEXT NOT NULL,
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            user_ids          TEXT,
            scheduled_at      TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'sent', 'failed')),
            sent_count        INTEGER,
            error             TEXT,
            created_at        TEXT NOT NULL,
            sent_at           TEXT
        );
        INSERT INTO scheduled_broadcasts_old
            (id, title, message, admin_id, platforms, version_min, last_active_days,
             user_ids, scheduled_at, status, sent_count, error, created_at, sent_at)
        SELECT id, title, message, admin_id, platforms, version_min, last_active_days,
               user_ids, scheduled_at, status, sent_count, error, created_at, sent_at
          FROM scheduled_broadcasts WHERE status != 'canceled';
        DROP TABLE scheduled_broadcasts;
        ALTER TABLE scheduled_broadcasts_old RENAME TO scheduled_broadcasts;
        CREATE INDEX IF NOT EXISTS idx_scheduled_broadcasts_due
            ON scheduled_broadcasts(status, scheduled_at);",
    )?;
    Ok(())
}

/// m048:scheduled_broadcasts 的 status CHECK 加 'sending' 中间态(W2-2 取消/下发原子抢占)。
/// worker fan-out 前先把行从 'pending' 原子抢占为 'sending',与 cancel 的 WHERE status='pending'
/// 互斥,闭合「取消返回成功但已群发」TOCTOU。SQLite 无法 ALTER CHECK,须重建表。
fn m048_scheduled_broadcasts_sending(store: &Store) -> Result<(), StoreError> {
    // 原子表重建,理由同 m046:单事务 + 开头 DROP IF EXISTS _new,闭合迁移中途崩溃 brick 窗口。
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS scheduled_broadcasts_new;
        CREATE TABLE scheduled_broadcasts_new (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL,
            message           TEXT NOT NULL,
            admin_id          TEXT NOT NULL,
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            user_ids          TEXT,
            scheduled_at      TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'sending', 'sent', 'failed', 'canceled')),
            sent_count        INTEGER,
            error             TEXT,
            created_at        TEXT NOT NULL,
            sent_at           TEXT
        );
        INSERT INTO scheduled_broadcasts_new
            (id, title, message, admin_id, platforms, version_min, last_active_days,
             user_ids, scheduled_at, status, sent_count, error, created_at, sent_at)
        SELECT id, title, message, admin_id, platforms, version_min, last_active_days,
               user_ids, scheduled_at, status, sent_count, error, created_at, sent_at
          FROM scheduled_broadcasts;
        DROP TABLE scheduled_broadcasts;
        ALTER TABLE scheduled_broadcasts_new RENAME TO scheduled_broadcasts;
        CREATE INDEX IF NOT EXISTS idx_scheduled_broadcasts_due
            ON scheduled_broadcasts(status, scheduled_at);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m048 down:重建回不含 'sending' 的 CHECK;残留 'sending' 行重置为 'pending' 以免丢失。仅 dev/test。
fn m048_scheduled_broadcasts_sending_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE scheduled_broadcasts_old (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL,
            message           TEXT NOT NULL,
            admin_id          TEXT NOT NULL,
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            user_ids          TEXT,
            scheduled_at      TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'sent', 'failed', 'canceled')),
            sent_count        INTEGER,
            error             TEXT,
            created_at        TEXT NOT NULL,
            sent_at           TEXT
        );
        INSERT INTO scheduled_broadcasts_old
            (id, title, message, admin_id, platforms, version_min, last_active_days,
             user_ids, scheduled_at, status, sent_count, error, created_at, sent_at)
        SELECT id, title, message, admin_id, platforms, version_min, last_active_days,
               user_ids, scheduled_at,
               CASE WHEN status = 'sending' THEN 'pending' ELSE status END,
               sent_count, error, created_at, sent_at
          FROM scheduled_broadcasts;
        DROP TABLE scheduled_broadcasts;
        ALTER TABLE scheduled_broadcasts_old RENAME TO scheduled_broadcasts;
        CREATE INDEX IF NOT EXISTS idx_scheduled_broadcasts_due
            ON scheduled_broadcasts(status, scheduled_at);",
    )?;
    Ok(())
}

/// m049:wb_center_imports 主键 (prefix, remote_id) → (prefix, remote_id, user_id)。
/// 此前多个 end-user 导入同一 center URL+remote_id 时第二人被 409 永久挡住、且 upsert 用
/// excluded.user_id 覆盖前主归属。改为按用户分行各自独立。user_id 的 NULL 归一为 ''
///(SQLite 多列主键视 NULL 互不等,会破坏 admin/system(原 NULL)重复导入的 upsert 去重)。
/// 原子表重建,理由同 m046:单事务 + 开头 DROP IF EXISTS _new,闭合迁移中途崩溃 brick 窗口。
fn m049_wb_center_imports_user_pk(store: &Store) -> Result<(), StoreError> {
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS wb_center_imports_new;
        CREATE TABLE wb_center_imports_new (
            source_url_hash_prefix TEXT NOT NULL,
            source_url TEXT NOT NULL,
            remote_id TEXT NOT NULL,
            local_wordbook_id TEXT NOT NULL,
            version TEXT NOT NULL,
            user_id TEXT NOT NULL DEFAULT '',
            imported_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            word_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (source_url_hash_prefix, remote_id, user_id)
        );
        INSERT INTO wb_center_imports_new
            (source_url_hash_prefix, source_url, remote_id, local_wordbook_id, version,
             user_id, imported_at, updated_at, word_count)
        SELECT source_url_hash_prefix, source_url, remote_id, local_wordbook_id, version,
               COALESCE(user_id, ''), imported_at, updated_at, word_count
          FROM wb_center_imports;
        DROP TABLE wb_center_imports;
        ALTER TABLE wb_center_imports_new RENAME TO wb_center_imports;
        CREATE INDEX IF NOT EXISTS idx_wb_center_imports_source_url ON wb_center_imports(source_url);
        CREATE INDEX IF NOT EXISTS idx_wb_center_imports_user ON wb_center_imports(user_id, updated_at DESC);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m049 down:回退到不含 user_id 的主键 (prefix, remote_id)。仅 dev/test。
/// 新表若已有多用户同 (prefix, remote_id) 行,旧主键容不下,按 (prefix, remote_id) 去重保留任一行;
/// '' 归还 NULL。
fn m049_wb_center_imports_user_pk_down(store: &Store) -> Result<(), StoreError> {
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS wb_center_imports_old;
        CREATE TABLE wb_center_imports_old (
            source_url_hash_prefix TEXT NOT NULL,
            source_url TEXT NOT NULL,
            remote_id TEXT NOT NULL,
            local_wordbook_id TEXT NOT NULL,
            version TEXT NOT NULL,
            user_id TEXT DEFAULT NULL,
            imported_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            word_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (source_url_hash_prefix, remote_id)
        );
        INSERT INTO wb_center_imports_old
            (source_url_hash_prefix, source_url, remote_id, local_wordbook_id, version,
             user_id, imported_at, updated_at, word_count)
        SELECT source_url_hash_prefix, source_url, remote_id, local_wordbook_id, version,
               NULLIF(user_id, ''), imported_at, updated_at, word_count
          FROM wb_center_imports
         GROUP BY source_url_hash_prefix, remote_id;
        DROP TABLE wb_center_imports;
        ALTER TABLE wb_center_imports_old RENAME TO wb_center_imports;
        CREATE INDEX IF NOT EXISTS idx_wb_center_imports_source_url ON wb_center_imports(source_url);
        CREATE INDEX IF NOT EXISTS idx_wb_center_imports_user ON wb_center_imports(user_id, updated_at DESC);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m050:#14 抗投毒——per-(user,word) 累计净位移账本。
/// word_elo 是全局共享状态(主键仅 word_id),被全员选词读取;单设备反复同向上报会无界推动
/// 该词全局评分污染他人排序。此表记录"单用户对某词全局评分的累计净贡献",persist_engine_state_atomic
/// 据此把单用户净位移钳在硬上限内,封死投毒路径(详见 store/operations/engine.rs::apply_elo_in_tx)。
fn m050_word_elo_user_contrib(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS word_elo_user_contrib (
            user_id          TEXT NOT NULL,
            word_id          TEXT NOT NULL,
            net_displacement REAL NOT NULL DEFAULT 0.0,
            PRIMARY KEY (user_id, word_id)
        );",
    )?;
    Ok(())
}

/// m050 down:DROP 账本表。仅 dev/test。
fn m050_word_elo_user_contrib_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP TABLE IF EXISTS word_elo_user_contrib;")?;
    Ok(())
}

/// m051:#9 灰度迁出态——把 'in_canary' 纳入 amas_tuning_suggestions.status 的 CHECK 白名单。
/// create_canary_and_claim_suggestion 把 Pending→InCanary('in_canary') 收进建灰度同一事务,
/// 防建议在灰度期间被 approve/approve-all 二次全量应用;但旧库的 CHECK 不含该值,UPDATE 直接约束违例。
/// SQLite 无法在位 ALTER CHECK,故整表重建(12 步式)迁移已有数据并刷新索引。
fn m051_suggestion_status_in_canary(store: &Store) -> Result<(), StoreError> {
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS amas_tuning_suggestions_new;
        CREATE TABLE amas_tuning_suggestions_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at TEXT NOT NULL,
            based_on_version_hash TEXT NOT NULL,
            patch_json TEXT NOT NULL,
            rationale TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            status TEXT NOT NULL CHECK (status IN ('pending','in_canary','approved','rejected','superseded','expired','auto_applied')),
            decided_by TEXT,
            decided_at TEXT,
            decision_note TEXT,
            cost_usd REAL,
            tokens_input INTEGER,
            tokens_output INTEGER,
            confidence REAL,
            base_values_json TEXT DEFAULT NULL
        );
        INSERT INTO amas_tuning_suggestions_new
            (id, created_at, based_on_version_hash, patch_json, rationale, evidence_json,
             status, decided_by, decided_at, decision_note, cost_usd, tokens_input,
             tokens_output, confidence, base_values_json)
        SELECT id, created_at, based_on_version_hash, patch_json, rationale, evidence_json,
               status, decided_by, decided_at, decision_note, cost_usd, tokens_input,
               tokens_output, confidence, base_values_json
          FROM amas_tuning_suggestions;
        DROP TABLE amas_tuning_suggestions;
        ALTER TABLE amas_tuning_suggestions_new RENAME TO amas_tuning_suggestions;
        CREATE INDEX IF NOT EXISTS idx_amas_suggestions_status_time
            ON amas_tuning_suggestions(status, created_at DESC);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m051 down:回退到不含 'in_canary' 的 CHECK。仅 dev/test。
/// 旧 CHECK 容不下 'in_canary',按其来源态把残留行映射回 'pending'。
fn m051_suggestion_status_in_canary_down(store: &Store) -> Result<(), StoreError> {
    let mut conn = store.conn()?;
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DROP TABLE IF EXISTS amas_tuning_suggestions_old;
        CREATE TABLE amas_tuning_suggestions_old (
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
            confidence REAL,
            base_values_json TEXT DEFAULT NULL
        );
        INSERT INTO amas_tuning_suggestions_old
            (id, created_at, based_on_version_hash, patch_json, rationale, evidence_json,
             status, decided_by, decided_at, decision_note, cost_usd, tokens_input,
             tokens_output, confidence, base_values_json)
        SELECT id, created_at, based_on_version_hash, patch_json, rationale, evidence_json,
               CASE WHEN status = 'in_canary' THEN 'pending' ELSE status END,
               decided_by, decided_at, decision_note, cost_usd, tokens_input,
               tokens_output, confidence, base_values_json
          FROM amas_tuning_suggestions;
        DROP TABLE amas_tuning_suggestions;
        ALTER TABLE amas_tuning_suggestions_old RENAME TO amas_tuning_suggestions;
        CREATE INDEX IF NOT EXISTS idx_amas_suggestions_status_time
            ON amas_tuning_suggestions(status, created_at DESC);",
    )?;
    tx.commit()?;
    Ok(())
}

/// m052:FSRS-6 升级——LLM 调参白名单补 `memoryModel.w[20]`（遗忘曲线 decay）。
/// 已部署库的 amas_tuning_whitelist 在首次启动时 seed 过 11 条,不会再吸收 const 新增条目;
/// 表非空时幂等补行(表空留给启动 seed 全量插入 12 条)。
fn m052_whitelist_add_w20_decay(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute(
        "INSERT INTO amas_tuning_whitelist (path, min_safe, max_safe, created_at, created_by)
         SELECT 'memoryModel.w[20]', 0.1, 0.8, ?1, 'migration:m052'
         WHERE EXISTS (SELECT 1 FROM amas_tuning_whitelist)
           AND NOT EXISTS (
               SELECT 1 FROM amas_tuning_whitelist WHERE path = 'memoryModel.w[20]'
           );",
        rusqlite::params![chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// m052 down:仅移除本迁移补的行。仅 dev/test。
fn m052_whitelist_add_w20_decay_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute(
        "DELETE FROM amas_tuning_whitelist
          WHERE path = 'memoryModel.w[20]' AND created_by = 'migration:m052';",
        [],
    )?;
    Ok(())
}

/// m053:回填 engine_user_states 的标量计数列。
/// 引擎计数真值历来只写进 state_json,标量列 total_event_count/session_event_count 恒为 DEFAULT 0,
/// 导致设备数据状态面板 AMAS 通道恒判 nil(黄)。写路径已改为同步两列,此处回填存量行,
/// 使历史设备无需再答一题即立即转 uploaded(绿)。
fn m053_backfill_engine_event_counts(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute(
        // 仅当 JSON 值为数值(integer/real)时回填;字段缺失或为非数值文本时回退现列,
        // 避免 CAST('非数字' AS INTEGER)=0 误覆盖既有真值。
        "UPDATE engine_user_states
            SET total_event_count = COALESCE(
                    CASE WHEN typeof(json_extract(state_json, '$.totalEventCount')) IN ('integer','real')
                         THEN CAST(json_extract(state_json, '$.totalEventCount') AS INTEGER) END,
                    total_event_count),
                session_event_count = COALESCE(
                    CASE WHEN typeof(json_extract(state_json, '$.sessionEventCount')) IN ('integer','real')
                         THEN CAST(json_extract(state_json, '$.sessionEventCount') AS INTEGER) END,
                    session_event_count)
          WHERE json_valid(state_json);",
        [],
    )?;
    Ok(())
}

/// m053 down:纯数据修正,无结构变更;把列清回 0 会破坏真值,故为 no-op。
fn m053_backfill_engine_event_counts_down(_store: &Store) -> Result<(), StoreError> {
    Ok(())
}

/// m040:system_settings 加 canary 自动回滚两阈值列(E3,收尾 C6)。
/// 原 canary_monitor 写死常量 0.05,迁到 system_settings 让 admin 在线调参。幂等加列。
fn m040_canary_thresholds(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "canary_reward_drop_threshold") {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN canary_reward_drop_threshold REAL NOT NULL DEFAULT 0.05",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "canary_anomaly_rise_threshold") {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN canary_anomaly_rise_threshold REAL NOT NULL DEFAULT 0.05",
            [],
        )?;
    }
    Ok(())
}

/// m040 down:DROP 两列。仅 dev/test。
fn m040_canary_thresholds_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if cols.iter().any(|c| c == "canary_reward_drop_threshold") {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN canary_reward_drop_threshold",
            [],
        )?;
    }
    if cols.iter().any(|c| c == "canary_anomaly_rise_threshold") {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN canary_anomaly_rise_threshold",
            [],
        )?;
    }
    Ok(())
}

/// m041:system_alerts 加 admin 收件箱已读/确认状态列(D1)。
/// 原表仅作 /admin/monitoring/events 派生轮询载体,无已读态;补 read_at/acked_by
/// 让 admin 收件箱可标记已读 + 未读计数。**绝不复用 end-user notifications 表
/// (按 user_id 键控,维度不同)**。幂等加列。
fn m041_system_alerts_inbox(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_alerts)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "read_at") {
        conn.execute(
            "ALTER TABLE system_alerts ADD COLUMN read_at TEXT DEFAULT NULL",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "acked_by") {
        conn.execute(
            "ALTER TABLE system_alerts ADD COLUMN acked_by TEXT DEFAULT NULL",
            [],
        )?;
    }
    Ok(())
}

/// m041 down:DROP 两列。仅 dev/test。
fn m041_system_alerts_inbox_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_alerts)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if cols.iter().any(|c| c == "read_at") {
        conn.execute("ALTER TABLE system_alerts DROP COLUMN read_at", [])?;
    }
    if cols.iter().any(|c| c == "acked_by") {
        conn.execute("ALTER TABLE system_alerts DROP COLUMN acked_by", [])?;
    }
    Ok(())
}

/// m042:设备推送「投递时机调度」+「草稿存储」(D2)。
/// scheduled_broadcasts —— 延时/指定时间下发的广播队列(受众过滤 platforms/version_min/
/// last_active_days/user_ids 存 JSON,None 走全员; + scheduled_at RFC3339 + status
/// pending/sent/failed; 定时 worker 扫到期行 fan-out)。push_drafts —— 推送编辑器「保存草稿」
/// (仿 m036 feedback_reply_drafts,全局单份 id='default' 单行 upsert)。
fn m042_scheduled_broadcasts_drafts(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scheduled_broadcasts (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL,
            message           TEXT NOT NULL,
            admin_id          TEXT NOT NULL,
            -- 受众过滤(JSON 文本;NULL = 该维度不过滤,四项全 NULL = 全员)
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            user_ids          TEXT,
            scheduled_at      TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'sent', 'failed')),
            sent_count        INTEGER,
            error             TEXT,
            created_at        TEXT NOT NULL,
            sent_at           TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_scheduled_broadcasts_due
            ON scheduled_broadcasts(status, scheduled_at);

        CREATE TABLE IF NOT EXISTS push_drafts (
            id                TEXT NOT NULL PRIMARY KEY,
            title             TEXT NOT NULL DEFAULT '',
            message           TEXT NOT NULL DEFAULT '',
            platforms         TEXT,
            version_min       TEXT,
            last_active_days  INTEGER,
            author_id         TEXT,
            updated_at        TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// m042 down:DROP 两表 + 索引。仅 dev/test。
fn m042_scheduled_broadcasts_drafts_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS push_drafts;
         DROP INDEX IF EXISTS idx_scheduled_broadcasts_due;
         DROP TABLE IF EXISTS scheduled_broadcasts;",
    )?;
    Ok(())
}

/// m043:system_settings 加客户端最低版本门控两列(D4)。
///   - min_client_version TEXT —— admin 运行时可配的最低客户端 semver,优先级高于
///     env MIN_CLIENT_VERSION;NULL 表示未设置(回落 env)。
///   - version_gate_enabled INTEGER —— 版本门控开关。开启后 strict-mode 即使 enabled=false
///     也对低于阈值的客户端返回 CLIENT_OUTDATED(发布切流场景独立于全量契约校验)。幂等加列。
fn m043_min_client_version_gate(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "min_client_version") {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN min_client_version TEXT DEFAULT NULL",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "version_gate_enabled") {
        conn.execute(
            "ALTER TABLE system_settings ADD COLUMN version_gate_enabled INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

/// m043 down:DROP 两列。仅 dev/test。
fn m043_min_client_version_gate_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(system_settings)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if cols.iter().any(|c| c == "min_client_version") {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN min_client_version",
            [],
        )?;
    }
    if cols.iter().any(|c| c == "version_gate_enabled") {
        conn.execute(
            "ALTER TABLE system_settings DROP COLUMN version_gate_enabled",
            [],
        )?;
    }
    Ok(())
}

/// m035 down:DROP crowd_filters 列。仅 dev/test。
fn m035_amas_canary_crowd_filters_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let has_col: bool = conn
        .prepare("PRAGMA table_info(amas_canary_config)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|n| n == "crowd_filters");
    if has_col {
        conn.execute(
            "ALTER TABLE amas_canary_config DROP COLUMN crowd_filters",
            [],
        )?;
    }
    Ok(())
}

/// m054:client_devices 加 4 列关联风控标记(B 层封禁绕过缓解)。
/// 封禁某设备时,自动给共享出口 IP / 同账号的其它设备置 risk_flag=1 供 admin 复核,
/// 不硬封(避免 CGNAT/共享网络误伤)。幂等加列 + partial index。
fn m054_client_devices_risk_flag(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (col, ddl) in [
        (
            "risk_flag",
            "INTEGER NOT NULL DEFAULT 0 CHECK (risk_flag IN (0, 1))",
        ),
        ("risk_reason", "TEXT DEFAULT NULL"),
        ("risk_flagged_at", "TEXT DEFAULT NULL"),
        ("risk_related_device", "TEXT DEFAULT NULL"),
    ] {
        if !cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE client_devices ADD COLUMN {col} {ddl}"),
                [],
            )?;
        }
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_client_devices_risk
            ON client_devices(risk_flagged_at DESC) WHERE risk_flag = 1",
        [],
    )?;
    Ok(())
}

fn m054_client_devices_risk_flag_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch("DROP INDEX IF EXISTS idx_client_devices_risk;")?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in [
        "risk_related_device",
        "risk_flagged_at",
        "risk_reason",
        "risk_flag",
    ] {
        if cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE client_devices DROP COLUMN {col}"),
                [],
            )?;
        }
    }
    Ok(())
}

/// m055:client_devices 加 fp_strong / fp_coarse 两列(web 设备封禁=浏览器指纹)。
/// fp_strong 高熵→精确匹配自动硬封;fp_coarse 硬件低熵→跨浏览器模糊匹配进 risk_flag。
/// 仅对已封行建 partial index,使请求路径的指纹匹配只扫被封设备。幂等。
fn m055_client_devices_fingerprint(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["fp_strong", "fp_coarse"] {
        if !cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE client_devices ADD COLUMN {col} TEXT DEFAULT NULL"),
                [],
            )?;
        }
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_client_devices_fp_strong
            ON client_devices(fp_strong) WHERE is_banned = 1 AND fp_strong IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_client_devices_fp_coarse
            ON client_devices(fp_coarse) WHERE is_banned = 1 AND fp_coarse IS NOT NULL;",
    )?;
    Ok(())
}

fn m055_client_devices_fingerprint_down(store: &Store) -> Result<(), StoreError> {
    let conn = store.conn()?;
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_client_devices_fp_coarse;
         DROP INDEX IF EXISTS idx_client_devices_fp_strong;",
    )?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(client_devices)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for col in ["fp_coarse", "fp_strong"] {
        if cols.iter().any(|c| c == col) {
            conn.execute(
                &format!("ALTER TABLE client_devices DROP COLUMN {col}"),
                [],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m033_creates_settings_config_tables() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let conn = store.conn().unwrap();
        for tbl in ["settings_config", "settings_snapshots"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![tbl],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {tbl} missing");
        }
    }

    #[test]
    fn m025_creates_advisor_tables_and_column() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let conn = store.conn().unwrap();
        // 两张新表存在
        for tbl in ["amas_tuning_whitelist", "amas_patch_canary"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![tbl],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {tbl} missing");
        }
        // system_settings.llm_advisor_enabled 列存在
        let has_col = conn
            .prepare("PRAGMA table_info(system_settings)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .any(|c| c == "llm_advisor_enabled");
        assert!(has_col, "llm_advisor_enabled column missing");
    }

    #[test]
    fn m032_creates_broadcasts_table_and_index() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let conn = store.conn().unwrap();
        let table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='broadcasts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table, 1, "broadcasts table missing");
        let index: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_broadcasts_created_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index, 1, "idx_broadcasts_created_at index missing");
    }

    /// 单列签名:(name, type, notnull, dflt_value, pk)。
    type ColSig = (String, String, i64, Option<String>, i64);
    /// schema 快照:表名→列签名列表 + 索引名集合。
    type SchemaSnapshot = (
        std::collections::BTreeMap<String, Vec<ColSig>>,
        std::collections::BTreeSet<String>,
    );

    /// 转储 user 表结构(表→列签名)与索引名集合,排除 schema_version(迁移记账)与
    /// sqlite 内部对象。
    fn dump_schema(conn: &rusqlite::Connection) -> SchemaSnapshot {
        let tables: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table'
                 AND name NOT LIKE 'sqlite_%' AND name <> 'schema_version' ORDER BY name",
            )
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        let mut cols = std::collections::BTreeMap::new();
        for t in &tables {
            let sig: Vec<_> = conn
                .prepare(&format!("PRAGMA table_info({t})"))
                .unwrap()
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(1)?,         // name
                        r.get::<_, String>(2)?,         // type
                        r.get::<_, i64>(3)?,            // notnull
                        r.get::<_, Option<String>>(4)?, // dflt_value
                        r.get::<_, i64>(5)?,            // pk
                    ))
                })
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            cols.insert(t.clone(), sig);
        }
        let indexes: std::collections::BTreeSet<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='index'
                 AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        (cols, indexes)
    }

    /// 升级幂等性回归:已跑完全部迁移的库,再次 run_migrations 不得改变任何表/列/索引。
    /// 这是本架构下可靠可测的核心不变式 —— init_schema 的 DDL 是 bootstrap,migrations
    /// 是 schema 演进的唯一权威,二者叠加后必须收敛且重入安全(防迁移把已建对象改坏)。
    /// (注:全新安装与升级部署都会跑全部迁移,故二者 schema 一致;"仅 DDL vs DDL+迁移"
    ///  并非两条真实部署路径,base 表只由 schema.rs 建、m001 为 no-op,无法从空库重建。)
    #[test]
    fn migrated_schema_is_stable_on_rerun() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let before = dump_schema(&store.conn().unwrap());
        store.run_migrations().unwrap();
        let after = dump_schema(&store.conn().unwrap());
        assert_eq!(before.0, after.0, "重跑迁移改变了表/列结构");
        assert_eq!(before.1, after.1, "重跑迁移改变了索引集合");
    }

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
        assert_eq!(
            has_record_type, 0,
            "init_schema 不应在已有 schema_version 表时追加新列"
        );
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
