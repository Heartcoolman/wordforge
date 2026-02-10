use std::env;
use std::net::{IpAddr, Ipv4Addr};
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
    pub jwt_expires_in_hours: u64,
    pub admin_jwt_secret: String,
    pub cors_origin: String,
    pub trust_proxy: bool,
    pub rate_limit: RateLimitConfig,
    pub worker: WorkerConfig,
    pub amas: AMASEnvConfig,
    pub llm: LLMConfig,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_secs: u64,
    pub max_requests: u64,
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
            .field("jwt_expires_in_hours", &self.jwt_expires_in_hours)
            .field("admin_jwt_secret", &"***REDACTED***")
            .field("cors_origin", &self.cors_origin)
            .field("trust_proxy", &self.trust_proxy)
            .field("rate_limit", &self.rate_limit)
            .field("worker", &self.worker)
            .field("amas", &self.amas)
            .field("llm", &self.llm)
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

impl Config {
    pub fn from_env() -> Self {
        Self {
            host: env_or_parse("HOST", IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            port: env_or_parse("PORT", 3000_u16),
            log_level: env_or("RUST_LOG", "info"),
            enable_file_logs: env_or_bool("ENABLE_FILE_LOGS", false),
            log_dir: env_or("LOG_DIR", "./logs"),
            sled_path: env_or("SLED_PATH", "./data/learning.sled"),
            jwt_secret: env_or(
                "JWT_SECRET",
                "change_me_to_random_64_chars_change_me_to_random_64_chars",
            ),
            jwt_expires_in_hours: env_or_parse("JWT_EXPIRES_IN_HOURS", 24_u64),
            admin_jwt_secret: env_or(
                "ADMIN_JWT_SECRET",
                "change_me_to_another_random_64_chars_change_me_to_another",
            ),
            cors_origin: env_or("CORS_ORIGIN", "http://localhost:5173"),
            trust_proxy: env_or_bool("TRUST_PROXY", false),
            rate_limit: RateLimitConfig {
                window_secs: env_or_parse("RATE_LIMIT_WINDOW_SECS", 900_u64),
                max_requests: env_or_parse("RATE_LIMIT_MAX", 500_u64),
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
        }
    }
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
        ]
    }

    fn clear_keys(keys: &[&str]) {
        for key in keys {
            env::remove_var(key);
        }
    }

    #[test]
    fn loads_defaults_when_missing() {
        let _guard = env_lock().lock().expect("env lock");
        clear_keys(managed_keys());

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

        env::set_var("LLM_ENABLED", "true");
        env::set_var("LLM_MOCK", "false");

        let cfg = Config::from_env();
        assert!(cfg.llm.enabled);
        assert!(!cfg.llm.mock);
    }
}
