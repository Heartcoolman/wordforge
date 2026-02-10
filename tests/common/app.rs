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

    let mut config = Config::from_env();
    config.sled_path = sled_path.to_string_lossy().to_string();
    config.jwt_secret = format!("jwt-secret-{}", uuid::Uuid::new_v4());
    config.admin_jwt_secret = format!("admin-secret-{}", uuid::Uuid::new_v4());
    config.rate_limit.max_requests = api_limit;
    config.rate_limit.window_secs = 60;
    config.worker.is_leader = false;
    config.trust_proxy = false;

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
