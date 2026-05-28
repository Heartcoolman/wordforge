pub const DDL: &str = r#"
-- Core user management
CREATE TABLE IF NOT EXISTS users (
    id TEXT NOT NULL,
    email TEXT NOT NULL COLLATE NOCASE,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    is_banned INTEGER NOT NULL DEFAULT 0 CHECK (is_banned IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    failed_login_count INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT DEFAULT NULL,
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user','staff','admin')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','inactive','suspended')),
    last_login_at TEXT DEFAULT NULL,
    referrer_source TEXT DEFAULT NULL,
    PRIMARY KEY (id),
    UNIQUE (email)
);
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at DESC);

CREATE TABLE IF NOT EXISTS admins (
    id TEXT NOT NULL,
    email TEXT NOT NULL COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    failed_login_count INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT DEFAULT NULL,
    PRIMARY KEY (id),
    UNIQUE (email)
);

CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    token_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
    PRIMARY KEY (token_hash)
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS admin_sessions (
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    token_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
    PRIMARY KEY (token_hash)
);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_user ON admin_sessions(user_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires_at ON admin_sessions(expires_at);

CREATE TABLE IF NOT EXISTS password_reset_tokens (
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (token_hash)
);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_expires_at ON password_reset_tokens(expires_at);

-- User profile tables
CREATE TABLE IF NOT EXISTS reward_preferences (
    user_id TEXT NOT NULL,
    reward_type TEXT NOT NULL DEFAULT 'standard',
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS user_avatars (
    user_id TEXT NOT NULL,
    avatar_url TEXT NOT NULL,
    filename TEXT NOT NULL,
    extension TEXT NOT NULL,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS habit_profiles (
    user_id TEXT NOT NULL,
    preferred_hours_json TEXT NOT NULL DEFAULT '[9,14,20]',
    median_session_length_mins REAL NOT NULL DEFAULT 15.0,
    sessions_per_day REAL NOT NULL DEFAULT 1.0,
    temporal_hourly_stats_json TEXT NOT NULL DEFAULT '[]',
    temporal_total_sessions INTEGER NOT NULL DEFAULT 0,
    daily_goal_words INTEGER NOT NULL DEFAULT 30,
    daily_goal_minutes INTEGER NOT NULL DEFAULT 25,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS user_preferences (
    user_id TEXT NOT NULL,
    theme TEXT NOT NULL DEFAULT 'light',
    language TEXT NOT NULL DEFAULT 'en',
    notification_enabled INTEGER NOT NULL DEFAULT 1 CHECK (notification_enabled IN (0, 1)),
    sound_enabled INTEGER NOT NULL DEFAULT 1 CHECK (sound_enabled IN (0, 1)),
    wordbook_center_url TEXT DEFAULT NULL,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS notifications (
    user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    notification_type TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    message TEXT NOT NULL DEFAULT '',
    word_id TEXT DEFAULT NULL,
    overdue_hours INTEGER DEFAULT NULL,
    read INTEGER NOT NULL DEFAULT 0 CHECK (read IN (0, 1)),
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, id)
);
CREATE INDEX IF NOT EXISTS idx_notifications_user_created_at ON notifications(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_user_unread ON notifications(user_id, read, created_at DESC);

CREATE TABLE IF NOT EXISTS badges (
    user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    unlocked INTEGER NOT NULL DEFAULT 0 CHECK (unlocked IN (0, 1)),
    progress REAL NOT NULL DEFAULT 0.0,
    unlocked_at TEXT DEFAULT NULL,
    PRIMARY KEY (user_id, id)
);

-- Content tables
CREATE TABLE IF NOT EXISTS words (
    id TEXT NOT NULL,
    text TEXT NOT NULL,
    meaning TEXT NOT NULL,
    pronunciation TEXT DEFAULT NULL,
    part_of_speech TEXT DEFAULT NULL,
    difficulty REAL NOT NULL DEFAULT 0.0,
    examples_json TEXT NOT NULL DEFAULT '[]',
    tags_json TEXT NOT NULL DEFAULT '[]',
    embedding_json TEXT DEFAULT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_words_created_at ON words(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_words_without_embedding ON words(created_at DESC)
    WHERE embedding_json IS NULL;

CREATE TABLE IF NOT EXISTS wordbooks (
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    book_type TEXT NOT NULL CHECK (book_type IN ('system', 'user')),
    user_id TEXT DEFAULT NULL,
    word_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_wordbooks_system ON wordbooks(name)
    WHERE book_type = 'system';
CREATE INDEX IF NOT EXISTS idx_wordbooks_user ON wordbooks(user_id, created_at DESC)
    WHERE book_type = 'user';

CREATE TABLE IF NOT EXISTS wordbook_words (
    wordbook_id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    added_at TEXT NOT NULL,
    PRIMARY KEY (wordbook_id, word_id)
);
CREATE INDEX IF NOT EXISTS idx_wordbook_words_word_id ON wordbook_words(word_id);

CREATE TABLE IF NOT EXISTS etymologies (
    word_id TEXT NOT NULL,
    word TEXT NOT NULL,
    etymology TEXT NOT NULL,
    roots_json TEXT NOT NULL DEFAULT '[]',
    generated INTEGER NOT NULL DEFAULT 0 CHECK (generated IN (0, 1)),
    source TEXT DEFAULT NULL,
    generated_at TEXT DEFAULT NULL,
    PRIMARY KEY (word_id)
);

CREATE TABLE IF NOT EXISTS word_morphemes (
    word_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    text TEXT NOT NULL,
    morpheme_type TEXT NOT NULL,
    meaning TEXT NOT NULL,
    PRIMARY KEY (word_id, position)
);

CREATE TABLE IF NOT EXISTS confusion_pairs (
    word_id_a TEXT NOT NULL,
    word_id_b TEXT NOT NULL,
    score REAL NOT NULL DEFAULT 0.0,
    updated_at TEXT DEFAULT NULL,
    PRIMARY KEY (word_id_a, word_id_b),
    CHECK (word_id_a < word_id_b)
);
CREATE INDEX IF NOT EXISTS idx_confusion_pairs_a ON confusion_pairs(word_id_a, score DESC);
CREATE INDEX IF NOT EXISTS idx_confusion_pairs_b ON confusion_pairs(word_id_b, score DESC);

CREATE TABLE IF NOT EXISTS wb_center_imports (
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
CREATE INDEX IF NOT EXISTS idx_wb_center_imports_source_url ON wb_center_imports(source_url);
CREATE INDEX IF NOT EXISTS idx_wb_center_imports_user ON wb_center_imports(user_id, updated_at DESC);

-- Learning data tables
CREATE TABLE IF NOT EXISTS study_configs (
    user_id TEXT NOT NULL,
    selected_wordbook_ids_json TEXT NOT NULL DEFAULT '[]',
    daily_word_count INTEGER NOT NULL DEFAULT 20,
    study_mode TEXT NOT NULL DEFAULT 'normal'
        CHECK (study_mode IN ('normal', 'intensive', 'review', 'casual')),
    daily_mastery_target INTEGER NOT NULL DEFAULT 10,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS learning_sessions (
    id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'completed', 'abandoned')),
    target_mastery_count INTEGER NOT NULL DEFAULT 0,
    total_questions INTEGER NOT NULL DEFAULT 0,
    actual_mastery_count INTEGER NOT NULL DEFAULT 0,
    context_shifts INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    summary_accuracy REAL DEFAULT NULL,
    summary_avg_response_time_ms INTEGER DEFAULT NULL,
    summary_mastered_word_ids_json TEXT NOT NULL DEFAULT '[]',
    summary_error_prone_word_ids_json TEXT NOT NULL DEFAULT '[]',
    summary_duration_secs INTEGER DEFAULT NULL,
    summary_hour_of_day INTEGER DEFAULT NULL,
    summary_final_difficulty REAL DEFAULT NULL,
    correct_count INTEGER NOT NULL DEFAULT 0,
    total_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_learning_sessions_user_status
    ON learning_sessions(user_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_learning_sessions_user_created_at
    ON learning_sessions(user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS session_shown_words (
    session_id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    shown_at INTEGER NOT NULL,
    batch_index INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (session_id, word_id)
);
CREATE INDEX IF NOT EXISTS idx_ssw_session_batch
    ON session_shown_words(session_id, batch_index);
CREATE INDEX IF NOT EXISTS idx_ssw_shown_at
    ON session_shown_words(shown_at);

CREATE TABLE IF NOT EXISTS learning_records (
    user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    is_correct INTEGER NOT NULL DEFAULT 0 CHECK (is_correct IN (0, 1)),
    response_time_ms INTEGER NOT NULL DEFAULT 0,
    session_id TEXT DEFAULT NULL,
    created_at TEXT NOT NULL,
    record_type TEXT NOT NULL DEFAULT 'all'
        CHECK (record_type IN ('learning', 'review', 'all')),
    self_rating INTEGER DEFAULT NULL
        CHECK (self_rating IS NULL OR self_rating BETWEEN 0 AND 3),
    PRIMARY KEY (user_id, id)
);
CREATE INDEX IF NOT EXISTS idx_learning_records_user_time
    ON learning_records(user_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_learning_records_time_user
    ON learning_records(created_at, user_id);
CREATE INDEX IF NOT EXISTS idx_learning_records_user_word
    ON learning_records(user_id, word_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_learning_records_user_session
    ON learning_records(user_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_learning_records_word_id
    ON learning_records(word_id);
CREATE INDEX IF NOT EXISTS idx_learning_records_user_type_time
    ON learning_records(user_id, record_type, created_at DESC);

CREATE TABLE IF NOT EXISTS word_learning_states (
    user_id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'NEW'
        CHECK (state IN ('NEW', 'LEARNING', 'REVIEWING', 'MASTERED', 'FORGOTTEN')),
    mastery_level REAL NOT NULL DEFAULT 0.0,
    next_review_date TEXT DEFAULT NULL,
    half_life REAL NOT NULL DEFAULT 24.0,
    correct_streak INTEGER NOT NULL DEFAULT 0,
    total_attempts INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, word_id)
);
CREATE INDEX IF NOT EXISTS idx_word_learning_states_due
    ON word_learning_states(user_id, next_review_date, word_id)
    WHERE next_review_date IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_word_learning_states_state
    ON word_learning_states(user_id, state);
CREATE INDEX IF NOT EXISTS idx_word_learning_states_word
    ON word_learning_states(word_id);

CREATE TABLE IF NOT EXISTS user_stats (
    user_id TEXT NOT NULL,
    total_records INTEGER NOT NULL DEFAULT 0,
    correct_records INTEGER NOT NULL DEFAULT 0,
    word_ids_json TEXT NOT NULL DEFAULT '[]',
    session_ids_json TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS word_favorites (
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
    ON wordbook_import_history(wordbook_id);

-- Engine/AMAS tables
CREATE TABLE IF NOT EXISTS engine_user_states (
    user_id TEXT NOT NULL,
    state_json TEXT NOT NULL DEFAULT '{}',
    attention REAL NOT NULL DEFAULT 0.7,
    fatigue REAL NOT NULL DEFAULT 0.0,
    motivation REAL NOT NULL DEFAULT 0.0,
    confidence REAL NOT NULL DEFAULT 0.1,
    last_active_at TEXT DEFAULT NULL,
    session_event_count INTEGER NOT NULL DEFAULT 0,
    total_event_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    cognitive_memory_capacity REAL NOT NULL DEFAULT 0.5,
    cognitive_processing_speed REAL NOT NULL DEFAULT 0.5,
    cognitive_stability REAL NOT NULL DEFAULT 0.5,
    trend_accuracy REAL NOT NULL DEFAULT 0.0,
    trend_speed REAL NOT NULL DEFAULT 0.0,
    trend_engagement REAL NOT NULL DEFAULT 0.0,
    habit_preferred_hours_json TEXT NOT NULL DEFAULT '[9,14,20]',
    habit_median_session_mins REAL NOT NULL DEFAULT 15.0,
    habit_sessions_per_day REAL NOT NULL DEFAULT 1.0,
    habit_hourly_stats_json TEXT NOT NULL DEFAULT '[]',
    habit_total_sessions INTEGER NOT NULL DEFAULT 0,
    last_session_id TEXT DEFAULT NULL,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS user_elo (
    user_id TEXT NOT NULL,
    rating REAL NOT NULL DEFAULT 1200.0,
    games INTEGER NOT NULL DEFAULT 0,
    sigma REAL NOT NULL DEFAULT 86.0,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS word_elo (
    word_id TEXT NOT NULL,
    rating REAL NOT NULL DEFAULT 1200.0,
    games INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (word_id)
);

CREATE TABLE IF NOT EXISTS mastery_states (
    user_id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    mdm_stability REAL NOT NULL DEFAULT 0.4,
    mdm_difficulty REAL NOT NULL DEFAULT 5.0,
    mdm_memory_strength REAL NOT NULL DEFAULT 0.0,
    mdm_last_review_at_ms INTEGER DEFAULT NULL,
    mdm_review_count INTEGER NOT NULL DEFAULT 0,
    mdm_short_term_strength REAL NOT NULL DEFAULT 0.0,
    PRIMARY KEY (user_id, word_id)
);
CREATE INDEX IF NOT EXISTS idx_mastery_states_word ON mastery_states(word_id);

CREATE TABLE IF NOT EXISTS engine_algo_states (
    user_id TEXT NOT NULL,
    algo_id TEXT NOT NULL,
    state_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (user_id, algo_id)
);

CREATE TABLE IF NOT EXISTS engine_monitoring_events (
    id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    event_type TEXT NOT NULL DEFAULT 'process_event',
    timestamp TEXT NOT NULL,
    latency_ms INTEGER NOT NULL DEFAULT 0,
    is_anomaly INTEGER NOT NULL DEFAULT 0 CHECK (is_anomaly IN (0, 1)),
    invariant_violations_json TEXT NOT NULL DEFAULT '[]',
    user_state_attention REAL NOT NULL DEFAULT 0.7,
    user_state_fatigue REAL NOT NULL DEFAULT 0.0,
    user_state_motivation REAL NOT NULL DEFAULT 0.0,
    user_state_confidence REAL NOT NULL DEFAULT 0.1,
    user_state_session_event_count INTEGER NOT NULL DEFAULT 0,
    user_state_total_event_count INTEGER NOT NULL DEFAULT 0,
    strategy_json TEXT NOT NULL DEFAULT '{}',
    reward_json TEXT NOT NULL DEFAULT '{}',
    cold_start_phase TEXT DEFAULT NULL,
    selection_constraints_met INTEGER NOT NULL DEFAULT 0 CHECK (selection_constraints_met IN (0, 1)),
    reward_value REAL NOT NULL DEFAULT 0.0,
    config_version TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_monitoring_events_timestamp
    ON engine_monitoring_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_monitoring_events_user
    ON engine_monitoring_events(user_id, timestamp DESC);

CREATE TABLE IF NOT EXISTS algorithm_metrics_daily (
    metric_date TEXT NOT NULL,
    algorithm_id TEXT NOT NULL,
    metrics_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (metric_date, algorithm_id)
);

CREATE TABLE IF NOT EXISTS alert_dedup (
    user_id TEXT NOT NULL,
    word_id TEXT NOT NULL,
    last_alerted_at_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, word_id)
);

CREATE TABLE IF NOT EXISTS monitoring_timeseries (
    period_id TEXT NOT NULL,
    data_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (period_id)
);

-- System tables
CREATE TABLE IF NOT EXISTS system_settings (
    singleton_id INTEGER NOT NULL DEFAULT 1 CHECK (singleton_id = 1),
    max_users INTEGER NOT NULL DEFAULT 10000,
    registration_enabled INTEGER NOT NULL DEFAULT 1 CHECK (registration_enabled IN (0, 1)),
    maintenance_mode INTEGER NOT NULL DEFAULT 0 CHECK (maintenance_mode IN (0, 1)),
    default_daily_words INTEGER NOT NULL DEFAULT 20,
    wordbook_center_url TEXT DEFAULT 'https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main',
    amas_auto_apply_enabled INTEGER NOT NULL DEFAULT 0 CHECK (amas_auto_apply_enabled IN (0, 1)),
    amas_auto_apply_max_per_day INTEGER NOT NULL DEFAULT 1,
    amas_auto_apply_min_confidence REAL NOT NULL DEFAULT 0.8,
    llm_advisor_max_cost_per_month_yuan REAL NOT NULL DEFAULT 100.0,
    llm_advisor_enabled INTEGER NOT NULL DEFAULT 0 CHECK (llm_advisor_enabled IN (0, 1)),
    amas_grayscale_steps TEXT NOT NULL DEFAULT '20,60,100',
    PRIMARY KEY (singleton_id)
);

-- m025:用户**自有**活动日志(区别于 admin_audit_log = admin 对用户的操作)
-- action 例:'user.login' / 'session.complete' / 'goal.update' / 'fatigue.alert'
CREATE TABLE IF NOT EXISTS user_activity_log (
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
    ON user_activity_log(action, created_at DESC);

-- Client management
CREATE TABLE IF NOT EXISTS client_devices (
    device_id TEXT NOT NULL,
    platform TEXT NOT NULL DEFAULT 'unknown',
    user_id TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    is_banned INTEGER NOT NULL DEFAULT 0 CHECK (is_banned IN (0, 1)),
    banned_at TEXT DEFAULT NULL,
    banned_by TEXT DEFAULT NULL,
    ban_reason TEXT DEFAULT NULL,
    app_version TEXT DEFAULT NULL,
    -- m024:GeoIP 反查的 ISO-3166-1 alpha-2 国家码(CN/US/...);last_ip 兼记本次 lookup 源
    country TEXT DEFAULT NULL,
    last_ip TEXT DEFAULT NULL,
    PRIMARY KEY (device_id)
);
CREATE INDEX IF NOT EXISTS idx_client_devices_user ON client_devices(user_id, last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_devices_active ON client_devices(last_seen_at DESC) WHERE is_banned = 0;
CREATE INDEX IF NOT EXISTS idx_client_devices_app_version ON client_devices(app_version) WHERE app_version IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_client_devices_platform ON client_devices(platform, last_seen_at DESC);

-- m024:强制升级策略(每平台一行)。min_version 以下启动拦截,suggested_version
-- 以下顶部黄条提示,grayscale_pct 控制灰度推送百分比,pwa_silent_update 仅 Web 有意义。
CREATE TABLE IF NOT EXISTS client_upgrade_policy (
    platform TEXT NOT NULL,
    min_version TEXT,
    suggested_version TEXT,
    grayscale_pct INTEGER NOT NULL DEFAULT 0 CHECK (grayscale_pct BETWEEN 0 AND 100),
    pwa_silent_update INTEGER NOT NULL DEFAULT 1 CHECK (pwa_silent_update IN (0, 1)),
    updated_at TEXT NOT NULL,
    updated_by TEXT,
    PRIMARY KEY (platform)
);
INSERT OR IGNORE INTO client_upgrade_policy (platform, updated_at)
    VALUES ('web', datetime('now')), ('ios', datetime('now')), ('android', datetime('now'));

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
CREATE INDEX IF NOT EXISTS idx_telemetry_server_ts ON telemetry_events(server_ts DESC);

CREATE TABLE IF NOT EXISTS feedback_items (
    id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    category TEXT DEFAULT NULL,
    body TEXT NOT NULL,
    route TEXT DEFAULT NULL,
    created_at TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    status TEXT NOT NULL DEFAULT 'open',
    assignee_admin_id INTEGER DEFAULT NULL,
    resolved_at TEXT DEFAULT NULL,
    resolution TEXT DEFAULT NULL,
    device_profile_json TEXT DEFAULT NULL,
    answer_snapshot_json TEXT DEFAULT NULL,
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_feedback_items_created_at ON feedback_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_feedback_items_user ON feedback_items(user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS telemetry_summaries (
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
    ON telemetry_summaries(device_id, server_ts DESC);

CREATE TABLE IF NOT EXISTS schema_version (
    singleton_id INTEGER NOT NULL DEFAULT 1 CHECK (singleton_id = 1),
    version INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (singleton_id)
);

-- AMAS 配置版本快照（每次保存 / 回滚均落一条），用于审计与对比
CREATE TABLE IF NOT EXISTS amas_config_versions (
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
    ON amas_config_versions(created_at DESC);

-- AMAS LLM 调参建议（人工审批 / 灰度自动应用）
CREATE TABLE IF NOT EXISTS amas_tuning_suggestions (
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

CREATE TABLE IF NOT EXISTS wordbook_local_tags (
    wordbook_id TEXT NOT NULL,
    tag         TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    created_by  TEXT,
    PRIMARY KEY (wordbook_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_wordbook_local_tags_tag ON wordbook_local_tags(tag);

CREATE TABLE IF NOT EXISTS amas_canary_config (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    version_hash    TEXT NOT NULL,
    percent         INTEGER NOT NULL CHECK (percent BETWEEN 0 AND 100),
    force_user_ids  TEXT NOT NULL DEFAULT '[]',
    active          INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0,1)),
    created_at      TEXT NOT NULL,
    created_by      TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_amas_canary_active ON amas_canary_config(active) WHERE active = 1;
CREATE INDEX IF NOT EXISTS idx_amas_canary_created ON amas_canary_config(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_amas_suggestions_status_time
    ON amas_tuning_suggestions(status, created_at DESC);

-- 远程探针执行审计表（admin REPL 下发 → 客户端 Worker 沙箱执行 → 回传结果）。
-- 全量留痕，公共 API 无 DELETE 入口；过期记录由 probe_cleanup cron 软删（≥retention_days）。
CREATE TABLE IF NOT EXISTS probe_executions (
    id TEXT PRIMARY KEY,                          -- = request_id
    batch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    admin_id TEXT NOT NULL,
    admin_username TEXT NOT NULL,                 -- 存 admin email（无 username 列时复用 email）
    script_body TEXT NOT NULL,                    -- 全量 script 留痕
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
    WHERE status IN ('pending', 'confirm_pending');

CREATE TABLE IF NOT EXISTS llm_advisor_cost_ledger (
    month TEXT NOT NULL,
    total_yuan REAL NOT NULL DEFAULT 0.0,
    last_updated_at TEXT NOT NULL,
    PRIMARY KEY (month)
);
"#;