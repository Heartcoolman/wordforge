use std::sync::Arc;

use axum::Router;
use tempfile::TempDir;
use tokio::sync::broadcast;

use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::config::Config;
use learning_backend::routes::build_router;
use learning_backend::state::AppState;
use learning_backend::store::Store;

pub struct TestApp {
    pub app: Router,
    pub state: AppState,
    pub config: Config,
    _temp_dir: TempDir,
}

/// 自定义 strict_mode 配置的 TestApp，便于 strict_mode_http.rs 集成测试。
pub async fn spawn_test_app_with_strict_mode(
    strict: learning_backend::config::StrictModeConfig,
) -> TestApp {
    spawn_with_limits_inner(100, 10, strict).await
}

/// 启用远程探针的 TestApp。spawn_with_limits_inner_with_probe 把 probe.enabled
/// 注入 Config，直接构造 AppState + Router；不复用主路径以避免 Arc<Config> 改写难题。
pub async fn spawn_test_app_with_probe_enabled() -> TestApp {
    spawn_with_limits_inner_with_probe(100, 10, Default::default(), true).await
}

async fn spawn_with_limits(api_limit: u64, auth_limit: u64) -> TestApp {
    spawn_with_limits_inner(api_limit, auth_limit, Default::default()).await
}

async fn spawn_with_limits_inner_with_probe(
    api_limit: u64,
    auth_limit: u64,
    strict_mode: learning_backend::config::StrictModeConfig,
    probe_enabled: bool,
) -> TestApp {
    let mut probe = learning_backend::config::ProbeConfig::default();
    probe.enabled = probe_enabled;
    spawn_with_full_config(api_limit, auth_limit, strict_mode, probe).await
}

async fn spawn_with_limits_inner(
    api_limit: u64,
    auth_limit: u64,
    strict_mode: learning_backend::config::StrictModeConfig,
) -> TestApp {
    spawn_with_full_config(api_limit, auth_limit, strict_mode, Default::default()).await
}

async fn spawn_with_full_config(
    api_limit: u64,
    auth_limit: u64,
    strict_mode: learning_backend::config::StrictModeConfig,
    probe: learning_backend::config::ProbeConfig,
) -> TestApp {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    // 直接构造 Config，避免使用 set_var 造成多线程测试环境变量竞态
    let test_secret = format!("integration-test-jwt-secret-{}", uuid::Uuid::new_v4());
    let test_admin_secret = format!("integration-test-admin-secret-{}", uuid::Uuid::new_v4());
    let test_refresh_secret = format!("integration-test-refresh-secret-{}", uuid::Uuid::new_v4());

    let config = Config {
        host: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        port: 3000,
        log_level: "info".to_string(),
        enable_file_logs: false,
        log_dir: "./logs".to_string(),
        database_url: temp_dir
            .path()
            .join("test.db")
            .to_string_lossy()
            .to_string(),
        api_only: false,
        sqlite_busy_timeout_ms: 5000,
        // CI 上 ubuntu 磁盘 IO 较慢，并发测试时 pool 等连接 250ms 不够导致 Pool(Error(None)) panic
        sqlite_connection_timeout_ms: 2000,
        sqlite_pool_size: 2,
        jwt_secret: test_secret,
        refresh_jwt_secret: test_refresh_secret,
        jwt_expires_in_hours: 24,
        refresh_token_expires_in_hours: 168,
        admin_jwt_secret: test_admin_secret,
        admin_jwt_expires_in_hours: 2,
        cors_origin: "http://localhost:5173".to_string(),
        trust_proxy: false,
        cookie_secure: false,
        self_watchdog: Default::default(),
        rate_limit: learning_backend::config::RateLimitConfig {
            window_secs: 60,
            max_requests: api_limit,
        },
        auth_rate_limit: learning_backend::config::AuthRateLimitConfig {
            window_secs: 60,
            max_requests: auth_limit,
        },
        worker: learning_backend::config::WorkerConfig {
            is_leader: false,
            enable_llm_advisor: false,
            enable_monitoring: false,
        },
        amas: learning_backend::config::AMASEnvConfig {
            ensemble_enabled: true,
            monitor_sample_rate: 0.05,
        },
        // 重定向到 tempdir，避免任何写回到生产 amas_config.toml
        amas_config_file: Some(
            temp_dir
                .path()
                .join("amas_config.toml")
                .to_string_lossy()
                .to_string(),
        ),
        llm: learning_backend::config::LLMConfig {
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
        update_check: learning_backend::config::UpdateCheckConfig {
            api_url: String::new(),
            cache_ttl_secs: 3600,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: None,
            max_tarball_bytes: 200 * 1024 * 1024,
            download_mirror_prefix: None,
        },
        pagination: Default::default(),
        limits: Default::default(),
        strict_mode,
        probe,
    };

    let store = Arc::new(
        Store::open_with_connection_timeout(
            &config.database_url,
            config.sqlite_busy_timeout_ms,
            config.sqlite_pool_size,
            config.sqlite_connection_timeout_ms,
        )
        .expect("open store"),
    );
    store.run_migrations().expect("run migrations");

    let amas_engine = Arc::new(AMASEngine::new(
        AMASConfig::from_env(&config.amas),
        store.clone(),
    ));
    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let state = AppState::new(store, amas_engine, &config, shutdown_tx, false);

    let app = build_router(state.clone());

    TestApp {
        app,
        state,
        config,
        _temp_dir: temp_dir,
    }
}

pub async fn spawn_test_app() -> TestApp {
    spawn_with_limits(100, 10).await
}

pub async fn spawn_test_server() -> TestApp {
    spawn_test_app().await
}

pub async fn spawn_test_server_with_limits(api_limit: u64, _auth_limit: u64) -> TestApp {
    spawn_with_limits(api_limit, _auth_limit).await
}
