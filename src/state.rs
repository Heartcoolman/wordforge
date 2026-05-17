use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::amas::engine::AMASEngine;
use crate::config::Config;
use crate::middleware::rate_limit::{AuthRateLimitState, RateLimitState};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SseClientInfo {
    pub conn_id: String,
    pub user_id: String,
    pub platform: String,
    pub connected_at: Instant,
    pub tx: mpsc::UnboundedSender<SseEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    #[serde(rename = "maintenance")]
    Maintenance { active: bool },
    #[serde(rename = "telemetry_request")]
    TelemetryRequest {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    #[serde(rename = "banned")]
    Banned,
    #[serde(rename = "unbanned")]
    Unbanned,
    #[serde(rename = "data_corrupted")]
    DataCorrupted,
    /// PR-7 D4: 新的 LLM 调参建议到达，前端 advisor 页可立即刷新
    #[serde(rename = "new_llm_suggestion")]
    NewLlmSuggestion {
        #[serde(rename = "suggestionId")]
        suggestion_id: i64,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatePayload {
    pub version: String,
    pub message: String,
}

#[derive(Clone)]
pub struct AppState {
    store: Arc<Store>,
    amas_engine: Arc<AMASEngine>,
    runtime: Arc<RuntimeConfig>,
    rate_limit: Arc<RateLimitState>,
    auth_rate_limit: Arc<AuthRateLimitState>,
    config: Arc<Config>,
    shutdown_tx: broadcast::Sender<()>,
    started_at: Instant,
    update_cache: Arc<RwLock<Option<(Instant, serde_json::Value)>>>,
    maintenance_mode: Arc<AtomicBool>,
    maintenance_tx: broadcast::Sender<bool>,
    update_tx: broadcast::Sender<UpdatePayload>,
    active_sse: Arc<DashMap<String, Vec<SseClientInfo>>>,
    last_heartbeat: Arc<DashMap<String, Instant>>,
    heartbeat_miss_count: Arc<DashMap<String, u8>>,
}

pub struct RuntimeConfig {
    pub llm_enabled: AtomicBool,
    pub llm_mock: AtomicBool,
}

impl AppState {
    pub fn new(
        store: Arc<Store>,
        amas_engine: Arc<AMASEngine>,
        config: &Config,
        shutdown_tx: broadcast::Sender<()>,
        initial_maintenance: bool,
    ) -> Self {
        let runtime = Arc::new(RuntimeConfig::from_config(config));
        let rate_limit = Arc::new(RateLimitState::new(
            config.rate_limit.window_secs,
            config.rate_limit.max_requests,
        ));
        let auth_rate_limit = Arc::new(AuthRateLimitState::new(
            config.auth_rate_limit.window_secs,
            config.auth_rate_limit.max_requests,
        ));
        let (maintenance_tx, _) = broadcast::channel(16);
        let (update_tx, _) = broadcast::channel(16);

        Self {
            store,
            amas_engine,
            runtime,
            rate_limit,
            auth_rate_limit,
            config: Arc::new(config.clone()),
            shutdown_tx,
            started_at: Instant::now(),
            update_cache: Arc::new(RwLock::new(None)),
            maintenance_mode: Arc::new(AtomicBool::new(initial_maintenance)),
            maintenance_tx,
            update_tx,
            active_sse: Arc::new(DashMap::new()),
            last_heartbeat: Arc::new(DashMap::new()),
            heartbeat_miss_count: Arc::new(DashMap::new()),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub async fn run_store_task<T, F>(
        &self,
        task_name: &'static str,
        f: F,
    ) -> Result<T, crate::blocking::BlockingTaskError>
    where
        F: FnOnce(Store) -> T + Send + 'static,
        T: Send + 'static,
    {
        let store = self.store().clone();
        crate::blocking::run_blocking(task_name, move || f(store)).await
    }

    pub fn amas(&self) -> &AMASEngine {
        &self.amas_engine
    }

    pub fn runtime(&self) -> &RuntimeConfig {
        &self.runtime
    }

    pub fn rate_limit(&self) -> &Arc<RateLimitState> {
        &self.rate_limit
    }

    pub fn auth_rate_limit(&self) -> &Arc<AuthRateLimitState> {
        &self.auth_rate_limit
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn shutdown_rx(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown_tx(&self) -> &broadcast::Sender<()> {
        &self.shutdown_tx
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn update_cache(&self) -> &RwLock<Option<(Instant, serde_json::Value)>> {
        &self.update_cache
    }

    pub fn is_maintenance(&self) -> bool {
        self.maintenance_mode.load(Ordering::Relaxed)
    }

    pub fn set_maintenance(&self, value: bool) {
        self.maintenance_mode.store(value, Ordering::Relaxed);
        let _ = self.maintenance_tx.send(value);
    }

    pub fn maintenance_rx(&self) -> broadcast::Receiver<bool> {
        self.maintenance_tx.subscribe()
    }

    pub fn update_rx(&self) -> broadcast::Receiver<UpdatePayload> {
        self.update_tx.subscribe()
    }

    pub fn broadcast_update(&self, version: Option<&str>, message: Option<&str>) {
        let version = version
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(env!("CARGO_PKG_VERSION"))
            .to_string();
        let message = message
            .unwrap_or("有新版本可用，请刷新页面获取最新内容")
            .to_string();
        let _ = self.update_tx.send(UpdatePayload { version, message });
    }

    pub fn active_sse(&self) -> &DashMap<String, Vec<SseClientInfo>> {
        &self.active_sse
    }

    pub fn last_heartbeat(&self) -> &DashMap<String, Instant> {
        &self.last_heartbeat
    }

    pub fn heartbeat_miss_count(&self) -> &DashMap<String, u8> {
        &self.heartbeat_miss_count
    }
}

impl RuntimeConfig {
    pub fn from_config(config: &Config) -> Self {
        Self {
            llm_enabled: AtomicBool::new(config.llm.enabled),
            llm_mock: AtomicBool::new(config.llm.mock),
        }
    }

    pub fn is_llm_enabled(&self) -> bool {
        self.llm_enabled.load(Ordering::Relaxed)
    }

    pub fn is_llm_mock(&self) -> bool {
        self.llm_mock.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::broadcast;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::Config;
    use crate::store::Store;

    use super::*;

    #[tokio::test]
    async fn runtime_config_switch_is_atomic() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("state_atomic.db").to_str().unwrap(),
                5000,
                4,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx, false);

        state.runtime().llm_enabled.store(true, Ordering::Relaxed);
        assert!(state.runtime().is_llm_enabled());
    }

    #[tokio::test]
    async fn shutdown_receiver_can_clone() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("state_shutdown.db").to_str().unwrap(),
                5000,
                4,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx.clone(), false);

        let mut rx1 = state.shutdown_rx();
        let mut rx2 = state.shutdown_rx();
        tx.send(()).unwrap();
        rx1.recv().await.unwrap();
        rx2.recv().await.unwrap();
    }
}
