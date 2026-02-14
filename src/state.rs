use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{broadcast, RwLock};

use crate::amas::engine::AMASEngine;
use crate::config::Config;
use crate::middleware::rate_limit::{AuthRateLimitState, RateLimitState};
use crate::store::Store;

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
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
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
        let store =
            Arc::new(Store::open(tmp.path().join("state_atomic.sled").to_str().unwrap()).unwrap());
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx);

        state.runtime().llm_enabled.store(true, Ordering::Relaxed);
        assert!(state.runtime().is_llm_enabled());
    }

    #[tokio::test]
    async fn shutdown_receiver_can_clone() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(tmp.path().join("state_shutdown.sled").to_str().unwrap()).unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx.clone());

        let mut rx1 = state.shutdown_rx();
        let mut rx2 = state.shutdown_rx();
        tx.send(()).unwrap();
        rx1.recv().await.unwrap();
        rx2.recv().await.unwrap();
    }
}
