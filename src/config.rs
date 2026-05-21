use std::env;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use std::fmt;

#[derive(Clone)]
pub struct Config {
    pub host: IpAddr,
    pub port: u16,
    pub log_level: String,
    pub enable_file_logs: bool,
    pub log_dir: String,
    pub database_url: String,
    pub api_only: bool,
    pub sqlite_busy_timeout_ms: u64,
    pub sqlite_connection_timeout_ms: u64,
    pub sqlite_pool_size: u32,
    pub jwt_secret: String,
    pub refresh_jwt_secret: String,
    pub jwt_expires_in_hours: u64,
    pub refresh_token_expires_in_hours: u64,
    pub admin_jwt_secret: String,
    pub admin_jwt_expires_in_hours: u64,
    pub cors_origin: String,
    pub trust_proxy: bool,
    pub cookie_secure: bool,
    pub self_watchdog: SelfWatchdogConfig,
    pub rate_limit: RateLimitConfig,
    pub auth_rate_limit: AuthRateLimitConfig,
    pub worker: WorkerConfig,
    pub amas: AMASEnvConfig,
    pub amas_config_file: Option<String>,
    pub llm: LLMConfig,
    pub update_check: UpdateCheckConfig,
    pub pagination: PaginationConfig,
    pub limits: LimitsConfig,
    pub strict_mode: StrictModeConfig,
    pub probe: ProbeConfig,
}

/// 远程探针配置：默认 enabled=false，避免未明确开启时被误用。
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub enabled: bool,
    pub rate_limit_per_min: u32,
    pub max_timeout_ms: u32,
    pub default_timeout_ms: u32,
    pub retention_days: u32,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_limit_per_min: 10,
            max_timeout_ms: 10_000,
            default_timeout_ms: 3_000,
            retention_days: 60,
        }
    }
}

/// §12 strict-mode 协议配置：
/// - `enabled=false`：完全跳过（默认，保留向后兼容）
/// - `enabled=true` + `hard_block=false`：仅 tracing warn，不拒绝请求
/// - `enabled=true` + `hard_block=true`：违规返回 400 + 错误码
/// - `min_client_version=Some("1.0.0")`：低于该版本的客户端被 CLIENT_OUTDATED 拒绝
#[derive(Debug, Clone, Default)]
pub struct StrictModeConfig {
    pub enabled: bool,
    pub hard_block: bool,
    pub min_client_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PaginationConfig {
    pub default_page_size: u64,
    pub max_page_size: u64,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_page_size: 20,
            max_page_size: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LimitsConfig {
    pub max_batch_size: usize,
    pub max_sse_connections: usize,
    pub max_exclude_word_ids: usize,
    pub max_word_fetch: usize,
    pub max_import_words: usize,
    pub max_records_fetch: usize,
    pub max_stats_records: usize,
    pub candidate_word_pool_size: usize,
    pub rate_limit_max_entries: usize,
    pub rate_limit_cleanup_interval_secs: u64,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 500,
            max_sse_connections: 1000,
            max_exclude_word_ids: 1000,
            max_word_fetch: 500,
            max_import_words: 5000,
            max_records_fetch: 10000,
            max_stats_records: 5000,
            candidate_word_pool_size: 500,
            rate_limit_max_entries: 100_000,
            rate_limit_cleanup_interval_secs: 300,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SelfWatchdogConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub failure_threshold: u32,
}

impl Default for SelfWatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 15,
            failure_threshold: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_secs: u64,
    pub max_requests: u64,
}

#[derive(Debug, Clone)]
pub struct AuthRateLimitConfig {
    pub window_secs: u64,
    pub max_requests: u64,
}

impl Default for AuthRateLimitConfig {
    fn default() -> Self {
        Self {
            window_secs: 60,
            max_requests: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub is_leader: bool,
    pub enable_llm_advisor: bool,
    pub enable_monitoring: bool,
}

#[derive(Debug, Clone)]
pub struct AMASEnvConfig {
    pub ensemble_enabled: bool,
    pub monitor_sample_rate: f64,
}

#[derive(Clone)]
pub struct LLMConfig {
    pub enabled: bool,
    pub mock: bool,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
    pub daily_cost_cap_usd: f64,
    pub input_price_per_mtok_usd: f64,
    pub output_price_per_mtok_usd: f64,
    pub max_cost_per_month_yuan: f64,
    pub usd_to_cny_rate: f64,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckConfig {
    pub api_url: String,
    pub cache_ttl_secs: u64,
    /// 后台 worker 是否周期性预热缓存
    pub worker_enabled: bool,
    /// worker 探测间隔（秒）
    pub worker_interval_secs: u64,
    /// 可选 GitHub PAT，限额从 60/h 升到 5000/h
    pub github_token: Option<String>,
    /// 默认拒绝 latest_tag ≤ current_tag；置 true 允许回滚到旧版本
    pub allow_downgrade: bool,
    /// 自更新安装目录；为 None 时取 current_exe 的父目录
    pub install_dir: Option<PathBuf>,
    /// 下载产物上限字节数，超出直接拒绝
    pub max_tarball_bytes: u64,
    /// v0.5.4：可选 GitHub 下载镜像前缀（如 `https://gh-proxy.com/`）；
    /// 国内服务器访问 release CDN 速度极慢（实测阿里云 22 KB/s），
    /// 配置后所有 release tarball / sha256 URL 会拼到 prefix 后访问。
    /// None 时走原 URL。
    pub download_mirror_prefix: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("enable_file_logs", &self.enable_file_logs)
            .field("log_dir", &self.log_dir)
            .field("database_url", &self.database_url)
            .field("api_only", &self.api_only)
            .field("sqlite_busy_timeout_ms", &self.sqlite_busy_timeout_ms)
            .field(
                "sqlite_connection_timeout_ms",
                &self.sqlite_connection_timeout_ms,
            )
            .field("sqlite_pool_size", &self.sqlite_pool_size)
            .field("jwt_secret", &"***REDACTED***")
            .field("refresh_jwt_secret", &"***REDACTED***")
            .field("jwt_expires_in_hours", &self.jwt_expires_in_hours)
            .field(
                "refresh_token_expires_in_hours",
                &self.refresh_token_expires_in_hours,
            )
            .field("admin_jwt_secret", &"***REDACTED***")
            .field(
                "admin_jwt_expires_in_hours",
                &self.admin_jwt_expires_in_hours,
            )
            .field("cors_origin", &self.cors_origin)
            .field("trust_proxy", &self.trust_proxy)
            .field("cookie_secure", &self.cookie_secure)
            .field("self_watchdog", &self.self_watchdog)
            .field("rate_limit", &self.rate_limit)
            .field("auth_rate_limit", &self.auth_rate_limit)
            .field("worker", &self.worker)
            .field("amas", &self.amas)
            .field("llm", &self.llm)
            .field("update_check", &self.update_check)
            .field("pagination", &self.pagination)
            .field("limits", &self.limits)
            .finish()
    }
}

impl fmt::Debug for LLMConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LLMConfig")
            .field("enabled", &self.enabled)
            .field("mock", &self.mock)
            .field("api_url", &self.api_url)
            .field("api_key", &"***REDACTED***")
            .field("model", &self.model)
            .field("timeout_secs", &self.timeout_secs)
            .field("daily_cost_cap_usd", &self.daily_cost_cap_usd)
            .finish()
    }
}

const DEFAULT_JWT_SECRET: &str = "change_me_to_random_64_chars_change_me_to_random_64_chars";
const DEFAULT_ADMIN_JWT_SECRET: &str = "change_me_to_another_random_64_chars_change_me_to_another";

impl Config {
    pub fn from_env() -> Self {
        let jwt_secret = env_or("JWT_SECRET", DEFAULT_JWT_SECRET);
        let refresh_jwt_secret = match env::var("REFRESH_JWT_SECRET") {
            Ok(val) if !val.is_empty() => val,
            _ => {
                // 使用 HMAC-SHA256 从 jwt_secret 派生独立的 refresh secret
                use hmac::{Hmac, Mac};
                type HmacSha256 = Hmac<sha2::Sha256>;
                let mut mac = HmacSha256::new_from_slice(jwt_secret.as_bytes())
                    .expect("HMAC can accept any key length");
                mac.update(b"refresh_token_secret_derivation");
                let result = mac.finalize();
                let derived = hex::encode(result.into_bytes());
                tracing::warn!(
                    "REFRESH_JWT_SECRET 未设置，已自动派生。生产环境请设置独立的 REFRESH_JWT_SECRET"
                );
                derived
            }
        };

        let config = Self {
            host: env_or_parse("HOST", IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            port: env_or_parse("PORT", 3000_u16),
            log_level: env_or("RUST_LOG", "info"),
            enable_file_logs: env_or_bool("ENABLE_FILE_LOGS", false),
            log_dir: env_or("LOG_DIR", "./logs"),
            database_url: normalized_db_path(&env_or("DATABASE_URL", "./data/learning.db")),
            api_only: env_or_bool("API_ONLY", false),
            sqlite_busy_timeout_ms: env_or_parse("SQLITE_BUSY_TIMEOUT_MS", 5000_u64),
            sqlite_connection_timeout_ms: env_or_parse("SQLITE_CONNECTION_TIMEOUT_MS", 250_u64),
            sqlite_pool_size: env_or_parse("SQLITE_POOL_SIZE", 16_u32),
            jwt_secret,
            refresh_jwt_secret,
            jwt_expires_in_hours: env_or_parse("JWT_EXPIRES_IN_HOURS", 24_u64),
            refresh_token_expires_in_hours: env_or_parse("REFRESH_TOKEN_EXPIRES_IN_HOURS", 168_u64),
            admin_jwt_secret: env_or("ADMIN_JWT_SECRET", DEFAULT_ADMIN_JWT_SECRET),
            admin_jwt_expires_in_hours: env_or_parse("ADMIN_JWT_EXPIRES_IN_HOURS", 2_u64),
            cors_origin: env_or("CORS_ORIGIN", "http://localhost:5173"),
            trust_proxy: env_or_bool("TRUST_PROXY", false),
            cookie_secure: env_or_bool("COOKIE_SECURE", false),
            self_watchdog: SelfWatchdogConfig {
                enabled: env_or_bool("ENABLE_SELF_WATCHDOG", false),
                interval_secs: env_or_parse("SELF_WATCHDOG_INTERVAL_SECS", 15_u64),
                failure_threshold: env_or_parse("SELF_WATCHDOG_FAILURE_THRESHOLD", 3_u32),
            },
            rate_limit: RateLimitConfig {
                window_secs: env_or_parse("RATE_LIMIT_WINDOW_SECS", 900_u64),
                max_requests: env_or_parse("RATE_LIMIT_MAX", 500_u64),
            },
            auth_rate_limit: AuthRateLimitConfig {
                window_secs: env_or_parse("AUTH_RATE_LIMIT_WINDOW_SECS", 60_u64),
                max_requests: env_or_parse("AUTH_RATE_LIMIT_MAX", 10_u64),
            },
            worker: WorkerConfig {
                is_leader: env_or_bool("WORKER_LEADER", true),
                enable_llm_advisor: env_or_bool("ENABLE_LLM_ADVISOR_WORKER", false),
                enable_monitoring: env_or_bool("ENABLE_ENGINE_MONITORING_WORKER", true),
            },
            amas: AMASEnvConfig {
                ensemble_enabled: env_or_bool("AMAS_ENSEMBLE_ENABLED", true),
                monitor_sample_rate: env_or_parse("AMAS_MONITOR_SAMPLE_RATE", 0.05_f64),
            },
            amas_config_file: env::var("AMAS_CONFIG_FILE").ok().filter(|s| !s.is_empty()),
            llm: LLMConfig {
                enabled: env_or_bool("LLM_ENABLED", false),
                mock: env_or_bool("LLM_MOCK", true),
                api_url: env_or("LLM_API_URL", "https://api.deepseek.com"),
                api_key: env_or("LLM_API_KEY", ""),
                model: env_or("LLM_MODEL", "deepseek-reasoner"),
                timeout_secs: env_or_parse("LLM_TIMEOUT_SECS", 60_u64),
                daily_cost_cap_usd: env_or_parse("LLM_DAILY_COST_CAP_USD", 1.0_f64),
                input_price_per_mtok_usd: env_or_parse("LLM_INPUT_PRICE_PER_MTOK_USD", 0.55_f64),
                output_price_per_mtok_usd: env_or_parse("LLM_OUTPUT_PRICE_PER_MTOK_USD", 2.19_f64),
                max_cost_per_month_yuan: env_or_parse("LLM_MAX_COST_PER_MONTH_YUAN", 100.0_f64),
                usd_to_cny_rate: env_or_parse("LLM_USD_TO_CNY_RATE", 7.3_f64),
            },
            update_check: UpdateCheckConfig {
                // v0.6.0-beta.3：list 端点用于后端单次拉取后分流 stable / beta latest，
                // /releases/latest 会跳过所有 prerelease，beta 通道拿不到 — 见
                // docs/superpowers/specs/2026-05-20-admin-beta-channel-design.md
                api_url: env_or(
                    "UPDATE_CHECK_API_URL",
                    "https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10",
                ),
                cache_ttl_secs: env_or_parse("UPDATE_CHECK_CACHE_TTL_SECS", 3600_u64),
                worker_enabled: env_or_bool("ENABLE_UPDATE_CHECKER_WORKER", true),
                worker_interval_secs: env_or_parse("UPDATE_CHECKER_INTERVAL_SECS", 3600_u64),
                github_token: env::var("WORDFORGE_GITHUB_TOKEN").ok().filter(|s| !s.is_empty()),
                allow_downgrade: env_or_bool("UPDATE_ALLOW_DOWNGRADE", false),
                install_dir: env::var("UPDATE_INSTALL_DIR")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
                max_tarball_bytes: env_or_parse("UPDATE_MAX_TARBALL_BYTES", 200 * 1024 * 1024_u64),
                download_mirror_prefix: env::var("GITHUB_DOWNLOAD_MIRROR_PREFIX")
                    .ok()
                    .filter(|s| !s.is_empty()),
            },
            pagination: PaginationConfig {
                default_page_size: env_or_parse("PAGINATION_DEFAULT_SIZE", 20_u64),
                max_page_size: env_or_parse("PAGINATION_MAX_SIZE", 100_u64),
            },
            strict_mode: StrictModeConfig {
                enabled: env_or_bool("STRICT_MODE_ENABLED", false),
                hard_block: env_or_bool("STRICT_MODE_HARD_BLOCK", false),
                min_client_version: env::var("MIN_CLIENT_VERSION")
                    .ok()
                    .filter(|s| !s.is_empty()),
            },
            probe: ProbeConfig {
                enabled: env_or_bool("PROBE_ENABLED", false),
                rate_limit_per_min: env_or_parse("PROBE_RATE_LIMIT_PER_MIN", 10_u32),
                max_timeout_ms: env_or_parse("PROBE_MAX_TIMEOUT_MS", 10_000_u32),
                default_timeout_ms: env_or_parse("PROBE_DEFAULT_TIMEOUT_MS", 3_000_u32),
                retention_days: env_or_parse("PROBE_RETENTION_DAYS", 60_u32),
            },
            limits: LimitsConfig {
                max_batch_size: env_or_parse("LIMITS_MAX_BATCH_SIZE", 500_usize),
                max_sse_connections: env_or_parse("LIMITS_MAX_SSE_CONNECTIONS", 1000_usize),
                max_exclude_word_ids: env_or_parse("LIMITS_MAX_EXCLUDE_WORD_IDS", 1000_usize),
                max_word_fetch: env_or_parse("LIMITS_MAX_WORD_FETCH", 500_usize),
                max_import_words: env_or_parse("LIMITS_MAX_IMPORT_WORDS", 5000_usize),
                max_records_fetch: env_or_parse("LIMITS_MAX_RECORDS_FETCH", 10000_usize),
                max_stats_records: env_or_parse("LIMITS_MAX_STATS_RECORDS", 5000_usize),
                candidate_word_pool_size: env_or_parse(
                    "LIMITS_CANDIDATE_WORD_POOL_SIZE",
                    500_usize,
                ),
                rate_limit_max_entries: env_or_parse(
                    "LIMITS_RATE_LIMIT_MAX_ENTRIES",
                    100_000_usize,
                ),
                rate_limit_cleanup_interval_secs: env_or_parse(
                    "LIMITS_RATE_LIMIT_CLEANUP_INTERVAL_SECS",
                    300_u64,
                ),
            },
        };

        config.validate_secrets();
        config
    }

    const INSECURE_MARKER: &str = "change_me";
    const MUST_CHANGE_MARKER: &str = "MUST_CHANGE";

    pub fn validate_secrets(&self) {
        if self.jwt_secret.contains(Self::INSECURE_MARKER)
            || self.jwt_secret.contains(Self::MUST_CHANGE_MARKER)
        {
            panic!(
                "FATAL: JWT_SECRET contains insecure default value. \
                 Set a strong random secret via the JWT_SECRET environment variable."
            );
        }
        if self.admin_jwt_secret.contains(Self::INSECURE_MARKER)
            || self.admin_jwt_secret.contains(Self::MUST_CHANGE_MARKER)
        {
            panic!(
                "FATAL: ADMIN_JWT_SECRET contains insecure default value. \
                 Set a strong random secret via the ADMIN_JWT_SECRET environment variable."
            );
        }
        if self.jwt_secret.len() < 32 {
            panic!(
                "FATAL: JWT_SECRET is too short (minimum 32 bytes). \
                 Set a strong random secret via the JWT_SECRET environment variable."
            );
        }
        if self.admin_jwt_secret.len() < 32 {
            panic!(
                "FATAL: ADMIN_JWT_SECRET is too short (minimum 32 bytes). \
                 Set a strong random secret via the ADMIN_JWT_SECRET environment variable."
            );
        }
        if self.refresh_jwt_secret.contains(Self::INSECURE_MARKER)
            || self.refresh_jwt_secret.contains(Self::MUST_CHANGE_MARKER)
        {
            panic!(
                "FATAL: REFRESH_JWT_SECRET contains insecure default value. \
                 Set a strong random secret via the REFRESH_JWT_SECRET environment variable."
            );
        }
        if self.refresh_jwt_secret.len() < 32 {
            panic!(
                "FATAL: REFRESH_JWT_SECRET is too short (minimum 32 bytes). \
                 Set a strong random secret via the REFRESH_JWT_SECRET environment variable."
            );
        }

        if self.refresh_jwt_secret == self.jwt_secret {
            tracing::warn!(
                "REFRESH_JWT_SECRET 与 JWT_SECRET 相同，降低了安全性。建议设置独立的 REFRESH_JWT_SECRET"
            );
        }

        // 生产环境下警告 CORS 通配符配置
        let rust_env = env::var("RUST_ENV")
            .or_else(|_| env::var("ENV"))
            .unwrap_or_default();
        if rust_env == "production" && self.cors_origin == "*" {
            tracing::warn!("生产环境下 CORS_ORIGIN 设置为 '*' 存在安全风险，建议限制为具体域名");
        }

        // 非开发/测试环境下，如果 secret 仍使用默认值则直接 panic
        if rust_env != "development" && rust_env != "test" && !rust_env.is_empty() {
            if self.jwt_secret == DEFAULT_JWT_SECRET {
                panic!(
                    "FATAL: JWT_SECRET is still set to the default value in {rust_env} environment. \
                     Set a strong random secret via the JWT_SECRET environment variable."
                );
            }
            if self.admin_jwt_secret == DEFAULT_ADMIN_JWT_SECRET {
                panic!(
                    "FATAL: ADMIN_JWT_SECRET is still set to the default value in {rust_env} environment. \
                     Set a strong random secret via the ADMIN_JWT_SECRET environment variable."
                );
            }
        }
    }
}

fn normalized_db_path(raw: &str) -> String {
    let path = Path::new(raw);
    if path.is_absolute() {
        return path.to_string_lossy().to_string();
    }

    let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    project_root.join(path).to_string_lossy().to_string()
}

pub fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn env_or_parse<T>(key: &str, default: T) -> T
where
    T: FromStr + Copy,
{
    match env::var(key) {
        Ok(raw) => match raw.parse::<T>() {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(
                    key,
                    value = %raw,
                    "Failed to parse env var, using default"
                );
                default
            }
        },
        Err(_) => default,
    }
}

pub fn env_or_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn managed_keys() -> &'static [&'static str] {
        &[
            "HOST",
            "PORT",
            "RUST_LOG",
            "RATE_LIMIT_MAX",
            "LLM_ENABLED",
            "LLM_TIMEOUT_SECS",
            "LLM_MOCK",
            "UPDATE_CHECK_API_URL",
            "UPDATE_CHECK_CACHE_TTL_SECS",
            "JWT_SECRET",
            "ADMIN_JWT_SECRET",
            "REFRESH_JWT_SECRET",
        ]
    }

    fn clear_keys(keys: &[&str]) {
        for key in keys {
            env::remove_var(key);
        }
    }

    /// 设置测试中必需的安全 secret 环境变量
    fn set_test_secrets() {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        env::set_var("JWT_SECRET", secret);
        env::set_var("ADMIN_JWT_SECRET", secret);
        env::set_var("REFRESH_JWT_SECRET", secret);
    }

    #[test]
    fn loads_defaults_when_missing() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();

        let cfg = Config::from_env();
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.rate_limit.max_requests, 500);
        assert!(!cfg.llm.enabled);
    }

    #[test]
    fn parses_numeric_values() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();

        env::set_var("PORT", "4000");
        env::set_var("RATE_LIMIT_MAX", "100");
        env::set_var("LLM_TIMEOUT_SECS", "42");

        let cfg = Config::from_env();
        assert_eq!(cfg.port, 4000);
        assert_eq!(cfg.rate_limit.max_requests, 100);
        assert_eq!(cfg.llm.timeout_secs, 42);
    }

    #[test]
    fn invalid_values_fall_back() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();

        env::set_var("PORT", "bad");
        env::set_var("RATE_LIMIT_MAX", "x");

        let cfg = Config::from_env();
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.rate_limit.max_requests, 500);
    }

    #[test]
    fn feature_flags_isolation() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();

        env::set_var("LLM_ENABLED", "true");
        env::set_var("LLM_MOCK", "false");

        let cfg = Config::from_env();
        assert!(cfg.llm.enabled);
        assert!(!cfg.llm.mock);
    }

    #[test]
    fn env_or_bool_accepts_common_truthy_falsy_synonyms() {
        let _guard = env_lock().lock().expect("env lock");
        for v in ["1", "true", "yes", "on", "TRUE", "On"] {
            env::set_var("__TEST_BOOL_T", v);
            assert!(env_or_bool("__TEST_BOOL_T", false), "v={v}");
        }
        for v in ["0", "false", "no", "off", "FALSE", "Off"] {
            env::set_var("__TEST_BOOL_T", v);
            assert!(!env_or_bool("__TEST_BOOL_T", true), "v={v}");
        }
        // 无法识别 -> 取 default
        env::set_var("__TEST_BOOL_T", "maybe");
        assert!(env_or_bool("__TEST_BOOL_T", true));
        assert!(!env_or_bool("__TEST_BOOL_T", false));
        env::remove_var("__TEST_BOOL_T");
        assert!(env_or_bool("__TEST_BOOL_T", true));
    }

    #[test]
    fn env_or_returns_default_when_missing() {
        let _guard = env_lock().lock().expect("env lock");
        env::remove_var("__TEST_MISSING");
        assert_eq!(env_or("__TEST_MISSING", "default-val"), "default-val");
    }

    #[test]
    fn env_or_parse_falls_back_on_bad_value() {
        let _guard = env_lock().lock().expect("env lock");
        env::set_var("__TEST_PARSE_BAD", "not-a-u64");
        let v = env_or_parse::<u64>("__TEST_PARSE_BAD", 99);
        assert_eq!(v, 99);
        env::remove_var("__TEST_PARSE_BAD");
    }

    #[test]
    fn config_debug_redacts_secrets() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();
        let cfg = Config::from_env();
        let dbg = format!("{:?}", cfg);
        assert!(dbg.contains("***REDACTED***"));
        assert!(!dbg.contains("test_secret_that_is_at_least_32_characters_long_ok"));
        // LLMConfig 也覆盖
        let llm_dbg = format!("{:?}", cfg.llm);
        assert!(llm_dbg.contains("***REDACTED***"));
    }

    #[test]
    fn loads_absolute_database_path_unchanged() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();
        env::set_var("DATABASE_URL", "/tmp/wordforge-test-abs.db");
        let cfg = Config::from_env();
        assert_eq!(cfg.database_url, "/tmp/wordforge-test-abs.db");
        env::remove_var("DATABASE_URL");
    }

    #[test]
    fn cors_wildcard_in_production_does_not_panic() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();
        env::set_var("CORS_ORIGIN", "*");
        env::set_var("RUST_ENV", "production");
        // 只是 warn，不应 panic
        let cfg = Config::from_env();
        assert_eq!(cfg.cors_origin, "*");
        env::remove_var("CORS_ORIGIN");
        env::remove_var("RUST_ENV");
    }

    #[test]
    fn refresh_secret_auto_derives_when_empty() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        // 仅设置 JWT_SECRET 与 ADMIN_JWT_SECRET，留空 REFRESH_JWT_SECRET
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        env::set_var("JWT_SECRET", secret);
        env::set_var("ADMIN_JWT_SECRET", secret);
        env::set_var("REFRESH_JWT_SECRET", ""); // 空字符串触发派生分支
        let cfg = Config::from_env();
        // 派生出的 hex 长度 = 64
        assert_eq!(cfg.refresh_jwt_secret.len(), 64);
        assert_ne!(cfg.refresh_jwt_secret, secret);
        env::remove_var("REFRESH_JWT_SECRET");
    }

    #[test]
    fn refresh_secret_uses_env_when_set() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());
        set_test_secrets();
        env::set_var(
            "REFRESH_JWT_SECRET",
            "explicit-refresh-secret-that-is-long-enough-ok",
        );
        let cfg = Config::from_env();
        assert_eq!(
            cfg.refresh_jwt_secret,
            "explicit-refresh-secret-that-is-long-enough-ok"
        );
    }

    #[test]
    #[should_panic(expected = "JWT_SECRET contains insecure default value")]
    fn validate_panics_on_insecure_jwt_secret() {
        let cfg = make_test_cfg(|c| {
            c.jwt_secret = "change_me_to_something_better_with_enough_padding".into();
        });
        cfg.validate_secrets();
    }

    #[test]
    #[should_panic(expected = "JWT_SECRET is too short")]
    fn validate_panics_on_short_jwt_secret() {
        let cfg = make_test_cfg(|c| c.jwt_secret = "short".into());
        cfg.validate_secrets();
    }

    #[test]
    #[should_panic(expected = "ADMIN_JWT_SECRET contains insecure default value")]
    fn validate_panics_on_insecure_admin_secret() {
        let cfg = make_test_cfg(|c| {
            c.admin_jwt_secret = "change_me_admin_with_lots_of_padding_for_min_len".into();
        });
        cfg.validate_secrets();
    }

    #[test]
    #[should_panic(expected = "ADMIN_JWT_SECRET is too short")]
    fn validate_panics_on_short_admin_secret() {
        let cfg = make_test_cfg(|c| c.admin_jwt_secret = "short".into());
        cfg.validate_secrets();
    }

    #[test]
    #[should_panic(expected = "REFRESH_JWT_SECRET contains insecure default value")]
    fn validate_panics_on_insecure_refresh_secret() {
        let cfg = make_test_cfg(|c| {
            c.refresh_jwt_secret = "change_me_refresh_with_enough_padding_for_min_len".into();
        });
        cfg.validate_secrets();
    }

    #[test]
    #[should_panic(expected = "REFRESH_JWT_SECRET is too short")]
    fn validate_panics_on_short_refresh_secret() {
        let cfg = make_test_cfg(|c| c.refresh_jwt_secret = "short".into());
        cfg.validate_secrets();
    }

    /// 构造一份"全部 secret 合法"的 Config，再由闭包注入待测违例字段。
    fn make_test_cfg<F: FnOnce(&mut Config)>(mutate: F) -> Config {
        let good = "test_secret_that_is_at_least_32_characters_long_ok".to_string();
        let mut cfg = Config {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 3000,
            log_level: "info".into(),
            enable_file_logs: false,
            log_dir: "./logs".into(),
            database_url: ":memory:".into(),
            api_only: false,
            sqlite_busy_timeout_ms: 5000,
            sqlite_connection_timeout_ms: 250,
            sqlite_pool_size: 2,
            jwt_secret: good.clone(),
            refresh_jwt_secret: good.clone(),
            jwt_expires_in_hours: 1,
            refresh_token_expires_in_hours: 24,
            admin_jwt_secret: good,
            admin_jwt_expires_in_hours: 1,
            cors_origin: "http://localhost".into(),
            trust_proxy: false,
            cookie_secure: false,
            self_watchdog: SelfWatchdogConfig::default(),
            rate_limit: RateLimitConfig {
                window_secs: 60,
                max_requests: 100,
            },
            auth_rate_limit: AuthRateLimitConfig::default(),
            worker: WorkerConfig {
                is_leader: false,
                enable_llm_advisor: false,
                enable_monitoring: false,
            },
            amas: AMASEnvConfig {
                ensemble_enabled: true,
                monitor_sample_rate: 0.05,
            },
            amas_config_file: None,
            llm: LLMConfig {
                enabled: false,
                mock: true,
                api_url: String::new(),
                api_key: String::new(),
                model: String::new(),
                timeout_secs: 30,
                daily_cost_cap_usd: 1.0,
                input_price_per_mtok_usd: 0.55,
                output_price_per_mtok_usd: 2.19,
                max_cost_per_month_yuan: 100.0,
                usd_to_cny_rate: 7.3,
            },
            update_check: UpdateCheckConfig {
                api_url: String::new(),
                cache_ttl_secs: 3600,
                worker_enabled: false,
                worker_interval_secs: 3600,
                github_token: None,
                allow_downgrade: false,
                install_dir: None,
                max_tarball_bytes: 1024,
                download_mirror_prefix: None,
            },
            pagination: PaginationConfig::default(),
            strict_mode: StrictModeConfig::default(),
            probe: ProbeConfig::default(),
            limits: LimitsConfig::default(),
        };
        mutate(&mut cfg);
        cfg
    }
}
