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

async fn spawn_with_limits(api_limit: u64) -> TestApp {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let sled_path = temp_dir.path().join("learning-test.sled");

    // 直接构造 Config，避免使用 set_var 造成多线程测试环境变量竞态
    let test_secret = format!("integration-test-jwt-secret-{}", uuid::Uuid::new_v4());
    let test_admin_secret = format!("integration-test-admin-secret-{}", uuid::Uuid::new_v4());
    let test_refresh_secret = format!("integration-test-refresh-secret-{}", uuid::Uuid::new_v4());

    let mut config = Config {
        host: std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        port: 3000,
        log_level: "info".to_string(),
        enable_file_logs: false,
        log_dir: "./logs".to_string(),
        sled_path: sled_path.to_string_lossy().to_string(),
        jwt_secret: test_secret,
        refresh_jwt_secret: test_refresh_secret,
        jwt_expires_in_hours: 24,
        refresh_token_expires_in_hours: 168,
        admin_jwt_secret: test_admin_secret,
        admin_jwt_expires_in_hours: 2,
        cors_origin: "http://localhost:5173".to_string(),
        trust_proxy: false,
        rate_limit: learning_backend::config::RateLimitConfig {
            window_secs: 60,
            max_requests: api_limit,
        },
        auth_rate_limit: Default::default(),
        worker: learning_backend::config::WorkerConfig {
            is_leader: false,
            enable_llm_advisor: false,
            enable_monitoring: false,
        },
        amas: learning_backend::config::AMASEnvConfig {
            ensemble_enabled: true,
            monitor_sample_rate: 0.05,
        },
        llm: learning_backend::config::LLMConfig {
            enabled: false,
            mock: true,
            api_url: String::new(),
            api_key: String::new(),
            timeout_secs: 30,
        },
        pagination: Default::default(),
        limits: Default::default(),
    };

    let store = Arc::new(Store::open(&config.sled_path).expect("open store"));
    store.run_migrations().expect("run migrations");

    let amas_engine = Arc::new(AMASEngine::new(
        AMASConfig::from_env(&config.amas),
        store.clone(),
    ));
    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let state = AppState::new(store, amas_engine, &config, shutdown_tx);

    let app = build_router(state.clone());

    TestApp {
        app,
        state,
        config,
        _temp_dir: temp_dir,
    }
}

pub async fn spawn_test_app() -> TestApp {
    spawn_with_limits(100).await
}

pub async fn spawn_test_server() -> TestApp {
    spawn_test_app().await
}

pub async fn spawn_test_server_with_limits(api_limit: u64, _auth_limit: u64) -> TestApp {
    spawn_with_limits(api_limit).await
}
