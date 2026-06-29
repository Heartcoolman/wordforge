use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub log_level: String,
    /// P1：是否启用每日归档文件落盘。实际消费者已下移到 `logging_buffer`（经 env
    /// `ENABLE_FILE_LOGS` 读取），由环形缓冲 Layer 落**已脱敏**记录，避免明文密钥落盘。
    /// 本字段保留以兼容 main.rs 构造（来自 Config）。
    pub enable_file_logs: bool,
    /// P1：归档目录。同 `enable_file_logs`，实际由 `logging_buffer` 经 env `LOG_DIR` 读取。
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

    // M0-P5：进程内日志环形缓冲（admin 监控页「实时日志」面板 + SSE 实时流数据源）。
    // 放在 env_filter 之后，复用同一全局级别过滤，开销受控。
    // P1：文件归档由本 Layer 单点负责（写已脱敏记录），故不再叠加独立的 fmt json 文件 appender，
    // 杜绝明文密钥落盘。归档启停由 logging_buffer 经 env ENABLE_FILE_LOGS / LOG_DIR 读取。
    let registry = Registry::default()
        .with(env_filter)
        .with(crate::logging_buffer::layer())
        .with(stdout_layer);

    // try_init 在全局 subscriber 已设置时返回错误，属于正常情况（如测试环境）；
    // 但在生产首次启动时失败则说明配置有误，应立即终止。
    if let Err(e) = registry.try_init() {
        let msg = e.to_string();
        if !msg.contains("already been set") {
            panic!("Failed to initialize tracing: {e}");
        }
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
