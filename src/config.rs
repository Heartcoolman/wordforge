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
    pub sled_path: String,
    pub jwt_secret: String,
    pub refresh_jwt_secret: String,
    pub jwt_expires_in_hours: u64,
    pub refresh_token_expires_in_hours: u64,
    pub admin_jwt_secret: String,
    pub admin_jwt_expires_in_hours: u64,
    pub cors_origin: String,
    pub trust_proxy: bool,
    pub rate_limit: RateLimitConfig,
    pub auth_rate_limit: AuthRateLimitConfig,
    pub worker: WorkerConfig,
    pub amas: AMASEnvConfig,
    pub llm: LLMConfig,
    pub pagination: PaginationConfig,
    pub limits: LimitsConfig,
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
    pub timeout_secs: u64,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("enable_file_logs", &self.enable_file_logs)
            .field("log_dir", &self.log_dir)
            .field("sled_path", &self.sled_path)
            .field("jwt_secret", &"***REDACTED***")
            .field("refresh_jwt_secret", &"***REDACTED***")
            .field("jwt_expires_in_hours", &self.jwt_expires_in_hours)
            .field("refresh_token_expires_in_hours", &self.refresh_token_expires_in_hours)
            .field("admin_jwt_secret", &"***REDACTED***")
            .field(
                "admin_jwt_expires_in_hours",
                &self.admin_jwt_expires_in_hours,
            )
            .field("cors_origin", &self.cors_origin)
            .field("trust_proxy", &self.trust_proxy)
            .field("rate_limit", &self.rate_limit)
            .field("auth_rate_limit", &self.auth_rate_limit)
            .field("worker", &self.worker)
            .field("amas", &self.amas)
            .field("llm", &self.llm)
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
            .field("timeout_secs", &self.timeout_secs)
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
            sled_path: normalized_sled_path(&env_or("SLED_PATH", "./data/learning.sled")),
            jwt_secret,
            refresh_jwt_secret,
            jwt_expires_in_hours: env_or_parse("JWT_EXPIRES_IN_HOURS", 24_u64),
            refresh_token_expires_in_hours: env_or_parse("REFRESH_TOKEN_EXPIRES_IN_HOURS", 168_u64),
            admin_jwt_secret: env_or("ADMIN_JWT_SECRET", DEFAULT_ADMIN_JWT_SECRET),
            admin_jwt_expires_in_hours: env_or_parse("ADMIN_JWT_EXPIRES_IN_HOURS", 2_u64),
            cors_origin: env_or("CORS_ORIGIN", "http://localhost:5173"),
            trust_proxy: env_or_bool("TRUST_PROXY", false),
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
            llm: LLMConfig {
                enabled: env_or_bool("LLM_ENABLED", false),
                mock: env_or_bool("LLM_MOCK", true),
                api_url: env_or("LLM_API_URL", ""),
                api_key: env_or("LLM_API_KEY", ""),
                timeout_secs: env_or_parse("LLM_TIMEOUT_SECS", 30_u64),
            },
            pagination: PaginationConfig {
                default_page_size: env_or_parse("PAGINATION_DEFAULT_SIZE", 20_u64),
                max_page_size: env_or_parse("PAGINATION_MAX_SIZE", 100_u64),
            },
            limits: LimitsConfig {
                max_batch_size: env_or_parse("LIMITS_MAX_BATCH_SIZE", 500_usize),
                max_sse_connections: env_or_parse("LIMITS_MAX_SSE_CONNECTIONS", 1000_usize),
                max_exclude_word_ids: env_or_parse("LIMITS_MAX_EXCLUDE_WORD_IDS", 1000_usize),
                max_word_fetch: env_or_parse("LIMITS_MAX_WORD_FETCH", 500_usize),
                max_import_words: env_or_parse("LIMITS_MAX_IMPORT_WORDS", 5000_usize),
                max_records_fetch: env_or_parse("LIMITS_MAX_RECORDS_FETCH", 10000_usize),
                max_stats_records: env_or_parse("LIMITS_MAX_STATS_RECORDS", 5000_usize),
                candidate_word_pool_size: env_or_parse("LIMITS_CANDIDATE_WORD_POOL_SIZE", 500_usize),
                rate_limit_max_entries: env_or_parse("LIMITS_RATE_LIMIT_MAX_ENTRIES", 100_000_usize),
                rate_limit_cleanup_interval_secs: env_or_parse("LIMITS_RATE_LIMIT_CLEANUP_INTERVAL_SECS", 300_u64),
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

fn normalized_sled_path(raw: &str) -> String {
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
}
