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
    /// 自更新 worker 探测到 GitHub Releases 有新二进制版本（与 broadcast_update 的
    /// `update_available` 区分，后者是给所有用户"刷新页面"的通知）。
    /// v0.6.0-beta.3：payload 含 `channel`（stable / beta），前端 admin 后台据此
    /// 在对应通道卡片亮 badge。
    #[serde(rename = "release_available")]
    ReleaseAvailable {
        #[serde(rename = "latestTag")]
        latest_tag: String,
        channel: crate::services::updater::Channel,
    },
    /// 一键更新执行过程中推给前端的阶段进度（0–100）
    #[serde(rename = "update_progress")]
    UpdateProgress { phase: String, percent: u8 },
    /// 远程探针下发：admin 通过 POST /api/admin/probe 派发到指定客户端，
    /// 客户端 Worker 沙箱里 eval base64 解码后的 script，传入白名单 ctx 快照，
    /// 通过 POST /api/probe/results 回传结果。
    #[serde(rename = "probe_request")]
    ProbeRequest {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "batchId")]
        batch_id: String,
        #[serde(rename = "scriptB64")]
        script_b64: String,
        #[serde(rename = "timeoutMs")]
        timeout_ms: u32,
        #[serde(rename = "ctxVersion")]
        ctx_version: u32,
    },
    /// 远程探针二次确认：当 script 调用 ctx.cmd.*（受控写）时，客户端首次回
    /// `confirm_required`，admin 输对 device 后 5 位后后端推 ProbeConfirm，
    /// 客户端用同一 ctx 快照重跑并执行 _actions。
    #[serde(rename = "probe_confirm")]
    ProbeConfirm {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "confirmToken")]
        confirm_token: String,
    },
    /// M0-P4：5xx 错误率超阈值，admin 告警事件。
    /// error_rate_watchdog 在滚动 5 分钟内 5xx/total > 1% 时广播给所有 admin SSE 连接。
    /// 同一时间窗口内不重复推送（dedup 5 分钟）。
    #[serde(rename = "incident")]
    Incident {
        /// 滚动 5 分钟内 5xx 错误率，范围 [0.0, 1.0]
        #[serde(rename = "errorRate")]
        error_rate: f64,
        /// 计算窗口长度（秒）
        #[serde(rename = "windowSecs")]
        window_secs: u64,
    },
    /// M1-G2：LLM advisor 月度人民币成本超上限，当月 worker 已自动停跑。
    #[serde(rename = "llm_budget_exceeded")]
    LlmBudgetExceeded {
        #[serde(rename = "spentYuan")]
        spent_yuan: f64,
        #[serde(rename = "capYuan")]
        cap_yuan: f64,
        #[serde(rename = "resumeMonth")]
        resume_month: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatePayload {
    pub version: String,
    pub message: String,
}

/// v0.5.2 加固：admin 一键升级异步执行的共享状态。
///
/// handler spawn 后台 task 后立即返回，前端通过 `/api/admin/updates/status` 轮询
/// 拿到当前 phase / percent / error；避免前端 fetch 超时（HTTP 499）中断 axum
/// handler 进而打断升级流程的设计缺陷（v0.5.1 之前的实现）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTaskStatus {
    pub task_id: String,
    /// `pending` | `downloading` | `verifying` | `extracting` | `backing_up_db`
    /// | `swapping` | `restarting` | `completed` | `failed`
    pub phase: String,
    pub percent: u8,
    pub target_version: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ApplyTaskStatus {
    pub fn is_running(&self) -> bool {
        self.completed_at.is_none() && self.error.is_none()
    }
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
    updater: Arc<RwLock<Option<Arc<crate::services::updater::Updater>>>>,
    /// v0.5.2 apply 后台 task 状态；sink 同步写、HTTP 同步读，故用 std::sync::Mutex
    apply_task: Arc<std::sync::Mutex<Option<ApplyTaskStatus>>>,
    probe_service: Arc<crate::services::probe::ProbeService>,
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
            updater: Arc::new(RwLock::new(None)),
            apply_task: Arc::new(std::sync::Mutex::new(None)),
            probe_service: Arc::new(crate::services::probe::ProbeService::new()),
        }
    }

    pub fn probe_service(&self) -> &crate::services::probe::ProbeService {
        &self.probe_service
    }

    /// 读取当前 apply 后台 task 状态（若无返回 None）。
    pub fn apply_task_snapshot(&self) -> Option<ApplyTaskStatus> {
        self.apply_task
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// 替换 apply task 状态；保留以便 handler/sink 显式覆盖。
    pub fn set_apply_task(&self, status: Option<ApplyTaskStatus>) {
        if let Ok(mut guard) = self.apply_task.lock() {
            *guard = status;
        }
    }

    /// 原地更新当前 apply task 状态，仅当存在时执行 mutator。
    pub fn update_apply_task<F: FnOnce(&mut ApplyTaskStatus)>(&self, f: F) {
        if let Ok(mut guard) = self.apply_task.lock() {
            if let Some(t) = guard.as_mut() {
                f(t);
            }
        }
    }

    /// 在 main 启动期注入 Updater；测试和老路径可以保持为 None。
    pub fn set_updater(&self, updater: Arc<crate::services::updater::Updater>) {
        if let Ok(mut slot) = self.updater.try_write() {
            *slot = Some(updater);
        }
    }

    pub async fn updater(&self) -> Option<Arc<crate::services::updater::Updater>> {
        self.updater.read().await.clone()
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

    /// 把同一个 SseEvent 广播给当前所有 SSE 连接。
    /// 用于 admin 级别的全局通知（update_available / update_progress 等）。
    /// 非 admin 前端没有对应 handler，收到只会忽略。
    pub fn broadcast_to_all_sse(&self, event: SseEvent) {
        for entry in self.active_sse.iter() {
            for conn in entry.value() {
                let _ = conn.tx.send(event.clone());
            }
        }
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
