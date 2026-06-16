use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::amas::engine::AMASEngine;
use crate::config::Config;
use crate::middleware::rate_limit::{AuthRateLimitState, RateLimitState};
use crate::services::event_bus::{self, EventBus};
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
    /// M1-A5：worker 连续 3 个调度周期未上报，调度器健康告警。
    #[serde(rename = "worker_missed")]
    WorkerMissed {
        /// 连续未上报的 worker 名称
        #[serde(rename = "workerName")]
        worker_name: String,
        /// 已连续错过的调度次数
        #[serde(rename = "missCount")]
        miss_count: u32,
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
    /// v1.1-P0.4：admin 切换某 channel 的当前激活资源包后广播，客户端收到后
    /// 主动拉取 manifest 并下载安装。频率限制由 admin handler 自行 5 分钟内 dedup。
    /// 字段名对齐 `docs/backend-handoff-resource-pack-v1.1.md` §2.3。
    #[serde(rename = "resource_pack_available")]
    ResourcePackAvailable {
        #[serde(rename = "packId")]
        pack_id: String,
        version: String,
        channel: crate::store::operations::resource_packs::ResourcePackChannel,
    },
    /// m027:admin 对某平台老版本发起强制升级时,精确推送到符合受众的活跃 SSE 连接。
    /// 老客户端不识别该 type 时静默忽略,不破坏现有协议。
    #[serde(rename = "upgrade_required")]
    UpgradeRequired {
        #[serde(rename = "latestVersion")]
        latest_version: String,
        message: Option<String>,
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
    /// W3-1：遥测专用 per-user 限频器。独立实例（非复用 rate_limit），避免与业务流量
    /// 串味、cleanup 周期互相污染。
    telemetry_rate_limit: Arc<RateLimitState>,
    /// 运行时配置快照。RwLock<Arc<Config>> 支持热替换：admin 在系统设置改限流 /
    /// 配额 / 令牌 TTL 等「运行期每请求读取」的配置后 `swap_config` 即时生效，无需重启。
    /// `config()` 返回当时不可变 Arc 快照（读锁瞬时释放），既有 116 处读取点零感知。
    config: Arc<std::sync::RwLock<Arc<Config>>>,
    shutdown_tx: broadcast::Sender<()>,
    started_at: Instant,
    update_cache: Arc<RwLock<Option<(Instant, serde_json::Value)>>>,
    maintenance_mode: Arc<AtomicBool>,
    maintenance_tx: broadcast::Sender<bool>,
    update_tx: broadcast::Sender<UpdatePayload>,
    active_sse: Arc<DashMap<String, Vec<SseClientInfo>>>,
    last_heartbeat: Arc<DashMap<String, Instant>>,
    heartbeat_miss_count: Arc<DashMap<String, u8>>,
    /// P1：单用户并发 SSE 连接计数，防单账号占满全局连接池。键 = user_id（与
    /// `AuthUser.user_id` / `SseClientInfo.user_id` 一致为 String，非 i64）。
    /// 建连时 `try_acquire_user_sse` 自增，SseGuard Drop 时 `release_user_sse` 自减。
    sse_per_user: Arc<DashMap<String, u32>>,
    /// P1：SSE 轮询用户状态的进程级短 TTL 缓存（键 = user_id）。每连接每 5s 无条件
    /// 查库会饱和 BLOCKING_SEMAPHORE；命中未过期缓存即跳过查库。TTL ≈ 一个轮询周期。
    /// 降级语义保留：缓存过期或缺失时回落查库并回填。完整事件驱动推送为后续项。
    sse_user_state_cache: Arc<DashMap<String, (Instant, crate::amas::types::UserState)>>,
    updater: Arc<RwLock<Option<Arc<crate::services::updater::Updater>>>>,
    /// v0.5.2 apply 后台 task 状态；sink 同步写、HTTP 同步读，故用 std::sync::Mutex
    apply_task: Arc<std::sync::Mutex<Option<ApplyTaskStatus>>>,
    probe_service: Arc<crate::services::probe::ProbeService>,
    /// v1.1-P0.6：资源包 Ed25519 签名器。启动后由 main.rs 注入；
    /// 失败时为 None，相关端点返回 503 SERVICE_UNAVAILABLE。
    resource_pack_signer:
        Arc<RwLock<Option<Arc<crate::services::resource_pack_signing::ResourcePackSigner>>>>,
    /// v1.1-P0.7：同 pack 5 分钟 SSE dedup，键 = (packId, channel.as_str()).
    recently_broadcasted_packs: Arc<DashMap<(String, String), Instant>>,
    /// v1.1-P1 S2：领域事件总线。`event-bus` feature 开启时为 InMemoryEventBus，
    /// 关闭时为 NoopEventBus。handler 写库成功后旁路 emit，consumer 异步消费。
    event_bus: Arc<dyn EventBus>,
    /// 时钟健康探测器。启动时与每小时对比公网 HTTP Date header，识别本机时钟漂移。
    /// 漂移超阈时 `/health` 状态降级为 `degraded`，详见 `clock_health` 模块文档。
    clock_health: Arc<crate::clock_health::ClockHealth>,
    /// m027：GeoIP 反查器（可选）。data/GeoLite2-Country.mmdb 加载成功才有，
    /// 失败/缺失为 None，客户端 IP→country 写库降级为 NULL，不影响主链路。
    geoip: Option<Arc<crate::services::geoip::Reader>>,
    /// m027：强制升级策略 60s 内存缓存。device_middleware 每请求都要读策略比较
    /// x-app-version，纯 SQLite 查询 ~100µs 但量大时也会成为瓶颈；缓存键 = platform。
    /// 写入端点(`PUT /admin/clients/upgrade-policy/:platform`)调 invalidate_upgrade_cache。
    #[allow(clippy::type_complexity)]
    upgrade_policy_cache: Arc<
        std::sync::RwLock<
            Option<(
                Instant,
                std::collections::HashMap<
                    String,
                    crate::store::operations::clients::ClientUpgradePolicy,
                >,
            )>,
        >,
    >,
    /// D4：客户端最低版本门控运行时快照。strict_mode 中间件每请求读此处而非
    /// 不可变 config 快照，使 admin 写端点改动「即时生效」。60s TTL 兜底（DB 直改时
    /// 最终一致），写端点调 `refresh_version_gate` 立即刷新。
    /// 元组 = (enabled, min_client_version)。
    version_gate: Arc<std::sync::RwLock<VersionGate>>,
}

/// D4：版本门控运行时状态。`enabled=true` 时按 `min_client_version` 拒绝旧客户端。
#[derive(Debug, Clone, Default)]
pub struct VersionGate {
    pub enabled: bool,
    pub min_client_version: Option<String>,
    /// 上次从 DB 刷新的时间；None 表示尚未加载（首请求触发 reload）。
    refreshed_at: Option<Instant>,
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
        let telemetry_rate_limit = Arc::new(RateLimitState::new(
            config.telemetry_rate_limit.window_secs,
            config.telemetry_rate_limit.max_requests,
        ));
        let (maintenance_tx, _) = broadcast::channel(16);
        let (update_tx, _) = broadcast::channel(16);

        Self {
            store,
            amas_engine,
            runtime,
            rate_limit,
            auth_rate_limit,
            telemetry_rate_limit,
            config: Arc::new(std::sync::RwLock::new(Arc::new(config.clone()))),
            shutdown_tx,
            started_at: Instant::now(),
            update_cache: Arc::new(RwLock::new(None)),
            maintenance_mode: Arc::new(AtomicBool::new(initial_maintenance)),
            maintenance_tx,
            update_tx,
            active_sse: Arc::new(DashMap::new()),
            last_heartbeat: Arc::new(DashMap::new()),
            heartbeat_miss_count: Arc::new(DashMap::new()),
            sse_per_user: Arc::new(DashMap::new()),
            sse_user_state_cache: Arc::new(DashMap::new()),
            updater: Arc::new(RwLock::new(None)),
            apply_task: Arc::new(std::sync::Mutex::new(None)),
            probe_service: Arc::new(crate::services::probe::ProbeService::new()),
            resource_pack_signer: Arc::new(RwLock::new(None)),
            recently_broadcasted_packs: Arc::new(DashMap::new()),
            event_bus: event_bus::build_default_bus(),
            clock_health: Arc::new(crate::clock_health::ClockHealth::new()),
            geoip: None,
            upgrade_policy_cache: Arc::new(std::sync::RwLock::new(None)),
            version_gate: Arc::new(std::sync::RwLock::new(VersionGate::default())),
        }
    }

    /// D4：解析当前版本门控配置（settings 优先，回落 env MIN_CLIENT_VERSION）。
    /// strict_mode 中间件每请求调用：内存快照 60s TTL 命中即返回，否则从 DB reload。
    /// reload 失败不阻断请求，沿用上次快照（首次失败则等价于「门控关闭」）。
    pub fn get_version_gate(&self) -> VersionGate {
        const TTL_SECS: u64 = 60;
        if let Ok(guard) = self.version_gate.read() {
            if let Some(at) = guard.refreshed_at {
                if at.elapsed().as_secs() < TTL_SECS {
                    return guard.clone();
                }
            }
        }
        self.refresh_version_gate()
    }

    /// D4：从 system_settings 重载版本门控快照并写回缓存。
    /// admin 写端点改动后调此方法实现「即时生效」；同时供 TTL 过期时 lazy reload。
    /// settings.min_client_version 优先，回落 env（config.strict_mode.min_client_version）。
    pub fn refresh_version_gate(&self) -> VersionGate {
        let cfg = self.config();
        let gate = match self.store.get_system_settings() {
            Ok(s) => {
                let min = s
                    .min_client_version
                    .filter(|v| !v.is_empty())
                    .or_else(|| cfg.strict_mode.min_client_version.clone());
                VersionGate {
                    enabled: s.version_gate_enabled,
                    min_client_version: min,
                    refreshed_at: Some(Instant::now()),
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "version_gate reload 失败，沿用上次快照");
                if let Ok(guard) = self.version_gate.read() {
                    return guard.clone();
                }
                VersionGate::default()
            }
        };
        if let Ok(mut guard) = self.version_gate.write() {
            *guard = gate.clone();
        }
        gate
    }

    /// m027:60s TTL 升级策略缓存读取(读尝试 → 过期 / 缺失 → DB 重载)。
    /// 失败时返回空 HashMap,middleware 把 X-Upgrade-Hint 退化为不塞。
    pub fn get_upgrade_policies_cached(
        &self,
    ) -> std::collections::HashMap<String, crate::store::operations::clients::ClientUpgradePolicy>
    {
        const TTL_SECS: u64 = 60;
        if let Ok(guard) = self.upgrade_policy_cache.read() {
            if let Some((at, ref map)) = *guard {
                if at.elapsed().as_secs() < TTL_SECS {
                    return map.clone();
                }
            }
        }
        let rows = match self.store.list_upgrade_policies() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "upgrade_policy cache reload failed; serving empty map");
                return std::collections::HashMap::new();
            }
        };
        let mut map = std::collections::HashMap::with_capacity(rows.len());
        for p in rows {
            map.insert(p.platform.clone(), p);
        }
        if let Ok(mut guard) = self.upgrade_policy_cache.write() {
            *guard = Some((Instant::now(), map.clone()));
        }
        map
    }

    /// m027:`PUT /admin/clients/upgrade-policy/:platform` 写入后调,强制下次请求 reload。
    pub fn invalidate_upgrade_cache(&self) {
        if let Ok(mut guard) = self.upgrade_policy_cache.write() {
            *guard = None;
        }
    }

    /// 时钟健康探测器（供 main.rs 启动时启动周期任务、health endpoint 读取状态）。
    pub fn clock_health(&self) -> &Arc<crate::clock_health::ClockHealth> {
        &self.clock_health
    }

    /// m027：启动期注入 GeoIP reader。data/GeoLite2-Country.mmdb 缺失时不调用即可，
    /// 链路保持 None；可重复调用以热切换。
    pub fn set_geoip(&mut self, reader: Option<Arc<crate::services::geoip::Reader>>) {
        self.geoip = reader;
    }

    /// m027：GeoIP reader（可选）。client_track middleware 用它把 IP 译成 country。
    pub fn geoip(&self) -> Option<&Arc<crate::services::geoip::Reader>> {
        self.geoip.as_ref()
    }

    /// 测试 / 自定义场景下注入一个特定的 EventBus（如 NoopEventBus）。
    /// 生产路径默认由 `AppState::new` 按 feature flag 选取。
    #[cfg(any(test, feature = "event-bus"))]
    pub fn with_event_bus(mut self, bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = bus;
        self
    }

    /// 领域事件总线。handler 写库成功后调 `state.event_bus().emit(...)` 旁路投递。
    pub fn event_bus(&self) -> &Arc<dyn EventBus> {
        &self.event_bus
    }

    /// v1.1-P0.6：启动期注入资源包签名器。失败由 main.rs 处理（功能降级，端点返 503）。
    pub fn set_resource_pack_signer(
        &self,
        signer: Arc<crate::services::resource_pack_signing::ResourcePackSigner>,
    ) {
        if let Ok(mut slot) = self.resource_pack_signer.try_write() {
            *slot = Some(signer);
        }
    }

    /// 读取资源包签名器；未初始化返回 None。沿用 updater() 的 async 范式。
    pub async fn resource_pack_signer(
        &self,
    ) -> Option<Arc<crate::services::resource_pack_signing::ResourcePackSigner>> {
        self.resource_pack_signer.read().await.clone()
    }

    /// v1.1-P0.7：检查同 pack×channel 是否在 5 分钟内已广播过；
    /// 若否，记录时间戳并返回 true（允许广播），否则返回 false。
    pub fn try_mark_pack_broadcast(&self, pack_id: &str, channel: &str) -> bool {
        const DEDUP_WINDOW_SECS: u64 = 300;
        let key = (pack_id.to_string(), channel.to_string());
        let now = Instant::now();
        if let Some(prev) = self.recently_broadcasted_packs.get(&key) {
            if now.duration_since(*prev).as_secs() < DEDUP_WINDOW_SECS {
                return false;
            }
        }
        self.recently_broadcasted_packs.insert(key, now);
        true
    }

    pub fn probe_service(&self) -> &crate::services::probe::ProbeService {
        &self.probe_service
    }

    /// 读取当前 apply 后台 task 状态（若无返回 None）。
    pub fn apply_task_snapshot(&self) -> Option<ApplyTaskStatus> {
        self.apply_task.lock().ok().and_then(|guard| guard.clone())
    }

    /// 替换 apply task 状态；保留以便 handler/sink 显式覆盖。
    pub fn set_apply_task(&self, status: Option<ApplyTaskStatus>) {
        if let Ok(mut guard) = self.apply_task.lock() {
            *guard = status;
        }
    }

    /// 单次持锁的 compare-and-set：仅当当前无进行中任务时写入 `initial` 并返回 true，
    /// 否则返回 false（调用方应返回 409）。消除 apply/rollback/restore 的 check-then-act 竞态。
    pub fn try_begin_apply_task(&self, initial: ApplyTaskStatus) -> bool {
        let mut guard = match self.apply_task.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if guard.as_ref().is_some_and(|t| t.is_running()) {
            return false;
        }
        *guard = Some(initial);
        true
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

    /// W3-1：遥测专用 per-user 限频器。
    pub fn telemetry_rate_limit(&self) -> &Arc<RateLimitState> {
        &self.telemetry_rate_limit
    }

    /// 当前运行时配置快照。返回不可变 `Arc<Config>`（读锁瞬时释放后即用 Arc 引用计数
    /// 持有），既有读取点 `state.config().field` 经 Deref 透明工作。
    pub fn config(&self) -> Arc<Config> {
        self.config
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// 热替换运行时配置（admin 保存「即时生效」设置后调用）。除替换 Arc 快照外，
    /// 还同步 3 个限流器的窗口秒数——窗口烘焙在 `RateLimiter` 内（非每请求读 config），
    /// 不同步则 windowSecs 字段会变成"假生效"。配额 / 令牌 TTL / 连接上限等字段由各读取点
    /// 每请求读 `config()`，替换后自动生效。
    pub fn swap_config(&self, new_config: Config) {
        self.rate_limit
            .limiter
            .set_window_secs(new_config.rate_limit.window_secs);
        self.auth_rate_limit
            .limiter
            .set_window_secs(new_config.auth_rate_limit.window_secs);
        self.telemetry_rate_limit
            .limiter
            .set_window_secs(new_config.telemetry_rate_limit.window_secs);
        let mut guard = self.config.write().unwrap_or_else(|p| p.into_inner());
        *guard = Arc::new(new_config);
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

    /// P1：尝试为某 user 占用一个 SSE 连接配额。当前计数 < `max` 时自增并返回 true，
    /// 否则不变返回 true 之外的 false（调用方应返回 429）。`entry` API 保证 CAS 原子性。
    /// `max == 0` 视为不限额（直接放行）。
    pub fn try_acquire_user_sse(&self, user_id: &str, max: usize) -> bool {
        if max == 0 {
            // 0 = 不限额：仍登记计数以便 Drop 对称释放。
            *self.sse_per_user.entry(user_id.to_string()).or_insert(0) += 1;
            return true;
        }
        let mut slot = self.sse_per_user.entry(user_id.to_string()).or_insert(0);
        if (*slot as usize) >= max {
            return false;
        }
        *slot += 1;
        true
    }

    /// P1：释放某 user 的一个 SSE 连接配额（SseGuard Drop 调用）。计数归零时移除条目。
    /// 释放一条 per-user SSE 配额；返回 true 表示这是该用户最后一条连接（计数归零）。
    /// 调用方据此决定是否清理 user_state 轮询缓存（仍有兄弟连接时保留，避免其下次 tick 多查库）。
    pub fn release_user_sse(&self, user_id: &str) -> bool {
        if let Some(mut slot) = self.sse_per_user.get_mut(user_id) {
            *slot = slot.saturating_sub(1);
            if *slot == 0 {
                drop(slot);
                self.sse_per_user.remove_if(user_id, |_, &v| v == 0);
                return true;
            }
        }
        false
    }

    /// P1：SSE 轮询读用户状态，命中进程级短 TTL 缓存即免查库（缓解 BLOCKING_SEMAPHORE
    /// 饱和）。过期 / 缺失返回 None，调用方查库后用 `put_sse_user_state` 回填。
    pub fn get_sse_user_state_cached(
        &self,
        user_id: &str,
        ttl: std::time::Duration,
    ) -> Option<crate::amas::types::UserState> {
        if let Some(entry) = self.sse_user_state_cache.get(user_id) {
            let (at, ref state) = *entry;
            if at.elapsed() < ttl {
                return Some(state.clone());
            }
        }
        None
    }

    /// P1：回填 SSE 轮询用户状态缓存。
    pub fn put_sse_user_state(&self, user_id: &str, state: crate::amas::types::UserState) {
        self.sse_user_state_cache
            .insert(user_id.to_string(), (Instant::now(), state));
    }

    /// P1：连接断开时清理 SSE 用户状态缓存条目（无活跃连接时无需保留）。
    pub fn evict_sse_user_state(&self, user_id: &str) {
        self.sse_user_state_cache.remove(user_id);
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
