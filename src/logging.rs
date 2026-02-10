use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub log_level: String,
    pub enable_file_logs: bool,
    pub log_dir: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            enable_file_logs: false,
            log_dir: "./logs".to_string(),
        }
    }
}

pub fn init_tracing(config: &LogConfig) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let stdout_layer = fmt::layer().with_target(true).with_thread_ids(false);

    let registry = Registry::default().with(env_filter).with(stdout_layer);

    if config.enable_file_logs {
        let file_appender = rolling::daily(&config.log_dir, "learning-backend.log");
        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_ansi(false)
            .json();
        if let Err(e) = registry.with(file_layer).try_init() {
            eprintln!("Failed to initialize tracing with file logs: {e}");
        }
    } else if let Err(e) = registry.try_init() {
        eprintln!("Failed to initialize tracing: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        let cfg = LogConfig::default();
        init_tracing(&cfg);
        init_tracing(&cfg);
    }
}
