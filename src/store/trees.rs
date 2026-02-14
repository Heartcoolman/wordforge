/// Sled tree 名称常量。
///
/// 注意：部分常量名与实际 tree 名称不完全一致，这是有意为之：
/// - `ENGINE_ALGORITHM_STATES` -> tree 名 `"engine_algo_states"`（缩写避免过长 key）
/// - `ENGINE_MONITORING_EVENTS` -> tree 名 `"engine_monitoring"`（缩写）
/// - `ALGORITHM_METRICS_DAILY` -> tree 名 `"algo_metrics_daily"`（缩写）
///
/// 常量名使用完整拼写以提高代码可读性，tree 名使用缩写以节省存储空间。
/// 修改 tree 名称会导致数据不可访问，请勿随意更改。

pub const USERS: &str = "users";
pub const SESSIONS: &str = "sessions";
pub const ADMIN_SESSIONS: &str = "admin_sessions";
pub const WORDS: &str = "words";
pub const RECORDS: &str = "records";
pub const LEARNING_SESSIONS: &str = "learning_sessions";
pub const ENGINE_USER_STATES: &str = "engine_user_states";
/// 常量名 ENGINE_ALGORITHM_STATES，tree 名缩写为 engine_algo_states
pub const ENGINE_ALGORITHM_STATES: &str = "engine_algo_states";
/// 常量名 ENGINE_MONITORING_EVENTS，tree 名缩写为 engine_monitoring
pub const ENGINE_MONITORING_EVENTS: &str = "engine_monitoring";
/// 常量名 ALGORITHM_METRICS_DAILY，tree 名缩写为 algo_metrics_daily
pub const ALGORITHM_METRICS_DAILY: &str = "algo_metrics_daily";
pub const PASSWORD_RESET_TOKENS: &str = "password_reset_tokens";
pub const CONFIG_VERSIONS: &str = "config_versions";

// P0 new trees
pub const ADMINS: &str = "admins";
pub const WORDBOOKS: &str = "wordbooks";
pub const WORDBOOK_WORDS: &str = "wordbook_words";
pub const WORD_LEARNING_STATES: &str = "word_learning_states";
pub const WORD_DUE_INDEX: &str = "word_due_index";
pub const STUDY_CONFIGS: &str = "study_configs";

// P4 trees
pub const USER_PROFILES: &str = "user_profiles";
pub const HABIT_PROFILES: &str = "habit_profiles";
pub const NOTIFICATIONS: &str = "notifications";
pub const BADGES: &str = "badges";
pub const USER_PREFERENCES: &str = "user_preferences";
pub const ETYMOLOGIES: &str = "etymologies";
pub const WORD_MORPHEMES: &str = "word_morphemes";
pub const CONFUSION_PAIRS: &str = "confusion_pairs";
pub const WB_CENTER_IMPORTS: &str = "wb_center_imports";

pub const WORDBOOK_TYPE_INDEX: &str = "idx_wordbook_type";

// Secondary index trees (performance optimization)
pub const USERS_BY_CREATED_AT: &str = "idx_users_by_created";
pub const WORDS_BY_CREATED_AT: &str = "idx_words_by_created";
pub const RECORDS_BY_TIME: &str = "idx_records_by_time";
pub const WORD_REFERENCES: &str = "idx_word_refs";
pub const USER_STATS: &str = "idx_user_stats";
