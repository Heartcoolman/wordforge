use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{extract::State, Router};
use futures::Stream;

use crate::auth::AuthUser;
use crate::response::AppError;
use crate::state::{AppState, SseClientInfo};

pub(crate) static SSE_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

/// admin SSE（/api/admin/realtime/events）全局并发连接数。与用户通道计数独立：
/// 用户连接风暴不会挤占 admin 观测通道，反之亦然。
pub(crate) static ADMIN_SSE_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

/// admin SSE 全局并发上限。admin 会话数量级远小于用户（每个后台 tab 2-3 条流），
/// 64 足以覆盖多管理员多 tab，同时防止 admin 凭据泄漏后被用作连接放大。
const MAX_ADMIN_SSE_CONNECTIONS: usize = 64;

/// SseEvent → SSE `event:` 字段名。用户通道与 admin 通道共用，保证两端 wire 名一致。
fn sse_event_name(event: &crate::state::SseEvent) -> &'static str {
    use crate::state::SseEvent;
    match event {
        SseEvent::Maintenance { .. } => "maintenance",
        SseEvent::TelemetryRequest { .. } => "telemetry_request",
        SseEvent::Banned => "banned",
        SseEvent::Unbanned => "unbanned",
        SseEvent::DataCorrupted => "data_corrupted",
        SseEvent::NewLlmSuggestion { .. } => "new_llm_suggestion",
        SseEvent::ReleaseAvailable { .. } => "release_available",
        SseEvent::UpdateProgress { .. } => "update_progress",
        SseEvent::ProbeRequest { .. } => "probe_request",
        SseEvent::ProbeConfirm { .. } => "probe_confirm",
        SseEvent::Incident { .. } => "incident",
        SseEvent::WorkerMissed { .. } => "worker_missed",
        SseEvent::LlmBudgetExceeded { .. } => "llm_budget_exceeded",
        SseEvent::ResourcePackAvailable { .. } => "resource_pack_available",
        SseEvent::UpgradeRequired { .. } => "upgrade_required",
        SseEvent::UpgradeCleared { .. } => "upgrade_cleared",
    }
}

struct SseGuard {
    state: AppState,
    /// 仅当本连接成功登记进 active_sse（归属核验通过）后才置 Some(did)；
    /// 在那之前保持 None，使核验 .await 被取消时 Drop 不会误删他人条目。
    device_id: Option<String>,
    conn_id: String,
    /// 本连接所属 user_id。P0：CAS 自增成功后立即随 guard 记录，保证任何 .await
    /// 取消点 drop 都能对称释放全局计数与 per-user 计数。
    user_id: String,
    /// P1：per-user 配额是否已占用（try_acquire 成功才置 true，Drop 时据此释放）。
    user_sse_acquired: bool,
}

impl Drop for SseGuard {
    fn drop(&mut self) {
        // P0：无条件解扣全局 SSE 计数（CAS 自增后 guard 即建立，故取消安全）。
        let _ = SSE_CONNECTION_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            count.checked_sub(1)
        });
        // P1：对称释放 per-user 配额；仅当这是该用户最后一条 SSE 连接时才清缓存，
        // 否则兄弟连接下次 tick 会 cache miss 多查一次库（P2 自愈优化）。
        if self.user_sse_acquired && self.state.release_user_sse(&self.user_id) {
            self.state.evict_sse_user_state(&self.user_id);
        }

        if let Some(ref did) = self.device_id {
            // 从该 device 的连接列表移除本连接；移除后若列表空则原子删除整个条目。
            let removed_entry = if let Some(mut conns) = self.state.active_sse().get_mut(did) {
                conns.retain(|c| c.conn_id != self.conn_id);
                if conns.is_empty() {
                    drop(conns);
                    // remove_if 仅在仍为空时删除：若期间新连接 push 进来则非空、不删，
                    // 此时心跳条目归新连接所有，不应清理。
                    self.state
                        .active_sse()
                        .remove_if(did, |_, conns| conns.is_empty())
                        .is_some()
                } else {
                    false
                }
            } else {
                false
            };

            // P2：跨 map 清理心跳前再次确认该 did 已无 active 连接条目。即使上面成功
            // remove_if 删除了空条目，仍可能有新连接在删除后瞬间用同一 did 重建条目并
            // 写入新的 last_heartbeat/miss_count；此时跳过清理，避免误删新连接的心跳基线
            // 导致 watchdog 误报 DataCorrupted。
            if removed_entry && self.state.active_sse().get(did).is_none() {
                self.state.last_heartbeat().remove(did);
                self.state.heartbeat_miss_count().remove(did);
            }
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/events", get(sse_handler))
}

/// admin 专用 SSE 路由（挂载到 /api/admin/realtime）。
pub fn admin_router() -> Router<AppState> {
    Router::new().route("/events", get(admin_sse_handler))
}

/// admin SSE 连接守卫：Drop 时解扣全局计数并从 admin_sse 注册表移除本连接。
/// 与用户通道 SseGuard 同构，但无 per-user 配额 / 心跳基线需要清理。
struct AdminSseGuard {
    state: AppState,
    /// admin_sse 注册键（device_id 或回退 conn_id）；登记成功后才置 Some，
    /// 使登记前任何取消路径的 Drop 不触碰注册表。
    registry_key: Option<String>,
    conn_id: String,
}

impl Drop for AdminSseGuard {
    fn drop(&mut self) {
        let _ =
            ADMIN_SSE_CONNECTION_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_sub(1)
            });
        if let Some(ref key) = self.registry_key {
            if let Some(mut conns) = self.state.admin_sse().get_mut(key) {
                conns.retain(|c| c.conn_id != self.conn_id);
            }
            // 与用户通道相同的 remove_if 语义：仅在仍为空时删除条目，
            // 避免误删并发新连接刚建立的列表。
            self.state
                .admin_sse()
                .remove_if(key, |_, conns| conns.is_empty());
        }
    }
}

/// GET /api/admin/realtime/events —— admin 后台专用 SSE（跨端契约 B）。
///
/// 与用户通道（/api/realtime/events）的差异：
/// - AdminAuthUser 鉴权（admin token + admin_sessions），用户 token 一律 401；
/// - 独立注册表 admin_sse 与独立全局配额，不占用户连接池，也不登记心跳基线
///   （heartbeat watchdog 不感知 admin 连接，不会误报 data_corrupted）；
/// - 无 AMAS 用户态轮询（admin 页面走 REST），仅转发 maintenance / update 广播
///   与 per-conn 定向/运维事件；
/// - 不做设备归属核验：admin 属可信运维会话，其声明的 x-device-id 仅用于
///   探针桥/遥测桥的定向投递寻址。
pub async fn admin_sse_handler(
    admin: crate::auth::AdminAuthUser,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    loop {
        let current = ADMIN_SSE_CONNECTION_COUNT.load(Ordering::SeqCst);
        if current >= MAX_ADMIN_SSE_CONNECTIONS {
            return Err(AppError::too_many_requests("SSE连接数过多"));
        }
        match ADMIN_SSE_CONNECTION_COUNT.compare_exchange(
            current,
            current + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(_) => continue,
        }
    }

    // CAS 成功后立即建 guard（registry_key=None），任何后续取消路径都能解扣计数。
    let conn_id = uuid::Uuid::new_v4().to_string();
    let mut guard = AdminSseGuard {
        state: state.clone(),
        registry_key: None,
        conn_id: conn_id.clone(),
    };

    // device_id 口径与用户通道一致：格式非法视为缺失，回退 conn_id 作注册键
    // （定向投递自然不命中，仅收广播/运维事件）。
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| crate::middleware::device::is_valid_device_id(v))
        .map(String::from);
    let platform = headers
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // 先订阅广播再登记，避免登记后、订阅前的事件窗口漏收（与用户通道同一考量）。
    let mut maintenance_rx = state.maintenance_rx();
    let mut update_rx = state.update_rx();
    let initial_maintenance = state.is_maintenance();

    let (per_conn_tx, mut per_conn_rx) =
        tokio::sync::mpsc::channel(crate::state::SSE_CONN_CHANNEL_CAP);

    let registry_key = device_id.unwrap_or_else(|| conn_id.clone());
    state
        .admin_sse()
        .entry(registry_key.clone())
        .or_default()
        .push(SseClientInfo {
            conn_id: conn_id.clone(),
            user_id: admin.admin_id.clone(),
            platform,
            connected_at: Instant::now(),
            tx: per_conn_tx,
        });
    guard.registry_key = Some(registry_key);

    let mut shutdown_rx = state.shutdown_rx();

    let stream = async_stream::stream! {
        let _guard = guard;

        // 连接即下发当前维护状态（与用户通道对齐，admin-ui 据此点亮维护横幅）。
        if let Ok(json) =
            serde_json::to_string(&serde_json::json!({ "type": "maintenance", "active": initial_maintenance }))
        {
            yield Ok(Event::default().event("maintenance").data(json));
        }

        loop {
            tokio::select! {
                result = maintenance_rx.recv() => {
                    if let Ok(active) = result {
                        let data = serde_json::json!({ "type": "maintenance", "active": active });
                        if let Ok(json) = serde_json::to_string(&data) {
                            yield Ok(Event::default().event("maintenance").data(json));
                        }
                    }
                }
                result = update_rx.recv() => {
                    if let Ok(payload) = result {
                        if let Ok(json) = serde_json::to_string(&payload) {
                            yield Ok(Event::default().event("update_available").data(json));
                        }
                    }
                }
                msg = per_conn_rx.recv() => {
                    match msg {
                        Some(event) => {
                            if let Ok(json) = serde_json::to_string(&event) {
                                yield Ok(Event::default().event(sse_event_name(&event)).data(json));
                            }
                        }
                        None => break,
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keepalive"),
    ))
}

pub async fn sse_handler(
    auth: AuthUser,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let max_sse = state.config().limits.max_sse_connections;
    loop {
        let current = SSE_CONNECTION_COUNT.load(Ordering::SeqCst);
        if current >= max_sse {
            return Err(AppError::too_many_requests("SSE连接数过多"));
        }
        match SSE_CONNECTION_COUNT.compare_exchange(
            current,
            current + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(_) => continue,
        }
    }

    // P0：CAS 自增成功后**立即**构造 SseGuard（device_id=None，user_sse 未占）。此后任何
    // .await 取消点（per-user 限额、归属核验）被 drop 都能解扣全局计数，杜绝永久泄漏。
    let conn_id = uuid::Uuid::new_v4().to_string();
    let mut guard = SseGuard {
        state: state.clone(),
        device_id: None,
        conn_id: conn_id.clone(),
        user_id: auth.user_id.clone(),
        user_sse_acquired: false,
    };

    // P1：per-user 并发上限。auth 必经 AuthUser 提取器，故无匿名连接需处理（user_id 恒
    // 非空）；超限返回 429，沿用全局上限的 too_many_requests 文案风格。占用成功记入 guard，
    // Drop 时对称释放。
    let max_per_user = state.config().limits.max_sse_connections_per_user;
    if !state.try_acquire_user_sse(&auth.user_id, max_per_user) {
        return Err(AppError::too_many_requests("SSE连接数过多"));
    }
    guard.user_sse_acquired = true;

    // 与 telemetry / device_middleware 一致校验 x-device-id 格式：格式非法的 id 视为 None，
    // 不登记进 active_sse / last_heartbeat，避免心跳刷新路径(telemetry 会先 400 拒绝同一 id)
    // 永远刷不到它而触发卡死的 heartbeat key + 误报 data_corrupted。
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| crate::middleware::device::is_valid_device_id(v))
        .map(String::from);
    let platform = headers
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // 在归属核验 await 之前先订阅 maintenance/update 广播：broadcast 只投递 subscribe()
    // 之后发出的消息，若在下方 owner-check await 期间触发 set_maintenance/broadcast_update，
    // 晚订阅会漏收该事件，使刚连上的客户端停留在过期维护状态。
    let mut maintenance_rx = state.maintenance_rx();
    let mut update_rx = state.update_rx();
    // 连接时的当前维护状态，作为初始事件下发，避免连上即维护却收不到任何事件的盲区。
    let initial_maintenance = state.is_maintenance();

    // 归属核验:device_id 取自客户端可控 x-device-id 头,登记进 active_sse 后会收到
    // admin 定向事件(探针脚本 / 封禁令牌等)。仅当设备未被他人认领(owner 为 NULL 或
    // 等于当前 user,或设备尚未注册)才登记,否则丢弃 device_id —— 不登记、不 500 整条流,
    // 通用事件(amas_state / maintenance / update)仍正常下发。
    let device_id = match device_id {
        Some(did) => {
            let did_for_task = did.clone();
            let owner = state
                .run_store_task("realtime.sse.check_owner", move |store| {
                    store.get_client_device_owner(&did_for_task)
                })
                .await;
            match owner {
                // 已注册且归属他人 → 拒绝登记(不暴露定向通道)
                Ok(Ok(Some(Some(uid)))) if uid != auth.user_id => None,
                // 未注册 / 未认领 / 归属本人 → 允许登记
                Ok(Ok(_)) => Some(did),
                // 查询失败:保守起见不登记定向通道,但保留流
                Ok(Err(e)) => {
                    tracing::error!(error = %e, device_id = %did, "SSE 设备归属核验失败");
                    None
                }
                Err(e) => {
                    tracing::error!(error = %e, device_id = %did, "SSE 设备归属核验任务失败");
                    None
                }
            }
        }
        None => None,
    };

    let (per_conn_tx, mut per_conn_rx) =
        tokio::sync::mpsc::channel(crate::state::SSE_CONN_CHANNEL_CAP);

    // device_id 为 None（无/非法头、归属他人、核验失败）时不登记 active_sse，但仍须保留
    // per_conn_tx 存活至流结束，否则唯一 Sender 被 drop → per_conn_rx.recv() 立即返回 None →
    // 流在首轮 select 即 break 关闭，违反「保留流、通用事件仍下发」契约。
    let keepalive_tx;
    if let Some(ref did) = device_id {
        keepalive_tx = None;
        let info = SseClientInfo {
            conn_id: conn_id.clone(),
            user_id: auth.user_id.clone(),
            platform: platform.clone(),
            connected_at: Instant::now(),
            tx: per_conn_tx,
        };
        state
            .active_sse()
            .entry(did.clone())
            .or_default()
            .push(info);
        // Initialize heartbeat timestamp to prevent cold-start miss accumulation
        state.last_heartbeat().insert(did.clone(), Instant::now());
        state.heartbeat_miss_count().insert(did.clone(), 0);
        // P0：核验通过且登记完成后才把 did 交给 guard，确保此前取消路径不会触及 active_sse。
        // 此处至 stream 起始之间均为同步操作，无 .await，故安全。
        guard.device_id = Some(did.clone());
    } else {
        keepalive_tx = Some(per_conn_tx);
    }

    let mut shutdown_rx = state.shutdown_rx();
    let user_id = auth.user_id.clone();

    let stream = async_stream::stream! {
        let _guard = guard;
        // 未登记时持有 per_conn_tx 存活，使 per_conn_rx 永不因 Sender 全 drop 而提前关闭流。
        let _keepalive_tx = keepalive_tx;

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_event_count: u64 = 0;
        // P1：进程级短 TTL 缓存窗口。略短于 5s 轮询周期，使同一 user 的并发连接在窗口内
        // 共享首个连接的查库结果，避免每连接每 5s 各打一次库饱和 BLOCKING_SEMAPHORE。
        // 降级语义保留：未命中即回落查库并回填。完整事件驱动推送为后续项。
        const SSE_STATE_TTL: Duration = Duration::from_millis(4500);

        // 缓存优先读：命中未过期缓存免查库，否则查库并回填。
        let initial_state = match state.get_sse_user_state_cached(&user_id, SSE_STATE_TTL) {
            Some(s) => Some(s),
            None => match state.amas().get_user_state_async(&user_id).await {
                Ok(s) => {
                    state.put_sse_user_state(&user_id, s.clone());
                    Some(s)
                }
                Err(_) => None,
            },
        };
        if let Some(user_state) = initial_state {
            last_event_count = user_state.total_event_count;
        }

        // 连接即下发当前维护状态，保证新连接客户端不会停留在过期维护状态（即使连接时
        // 维护已开启且后续无 toggle）。
        if let Ok(json) =
            serde_json::to_string(&serde_json::json!({ "type": "maintenance", "active": initial_maintenance }))
        {
            yield Ok(Event::default().event("maintenance").data(json));
        }

        // 设备在连接存活期间被封时置 true,解封置回 false。封禁期间停发业务状态推送
        // (amas_state / maintenance / update_available 及定向探针等),仅保留 banned/unbanned,
        // 避免被封设备(尤其逆向客户端忽略 banned 事件者)继续经此长连接窃取实时状态。
        // 注:连接建立时已被封的设备会被 device_middleware 403 拦在建连前,故初始恒为 false。
        let mut banned = false;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if banned { continue; }
                    let polled = match state.get_sse_user_state_cached(&user_id, SSE_STATE_TTL) {
                        Some(s) => Some(s),
                        None => match state.amas().get_user_state_async(&user_id).await {
                            Ok(s) => {
                                state.put_sse_user_state(&user_id, s.clone());
                                Some(s)
                            }
                            Err(_) => None,
                        },
                    };
                    if let Some(user_state) = polled {
                        if user_state.total_event_count > last_event_count {
                            let event_data = serde_json::json!({
                                "type": "state_change",
                                "attention": user_state.attention,
                                "fatigue": user_state.fatigue,
                                "motivation": user_state.motivation,
                                "confidence": user_state.confidence,
                                "sessionEventCount": user_state.session_event_count,
                                "totalEventCount": user_state.total_event_count,
                            });
                            if let Ok(json) = serde_json::to_string(&event_data) {
                                yield Ok(Event::default().event("amas_state").data(json));
                            }
                            last_event_count = user_state.total_event_count;
                        }
                    }
                }
                result = maintenance_rx.recv() => {
                    if banned { continue; }
                    if let Ok(active) = result {
                        let data = serde_json::json!({ "type": "maintenance", "active": active });
                        if let Ok(json) = serde_json::to_string(&data) {
                            yield Ok(Event::default().event("maintenance").data(json));
                        }
                    }
                }
                result = update_rx.recv() => {
                    if banned { continue; }
                    if let Ok(payload) = result {
                        if let Ok(json) = serde_json::to_string(&payload) {
                            yield Ok(Event::default().event("update_available").data(json));
                        }
                    }
                }
                msg = per_conn_rx.recv() => {
                    match msg {
                        Some(event) => {
                            // 封禁状态机:Banned 置位、Unbanned 复位;二者本身恒下发。
                            match &event {
                                crate::state::SseEvent::Banned => banned = true,
                                crate::state::SseEvent::Unbanned => banned = false,
                                _ => {}
                            }
                            // 封禁期间抑制除 banned/unbanned 外的一切定向事件,防状态泄漏。
                            let suppressed = banned
                                && !matches!(
                                    &event,
                                    crate::state::SseEvent::Banned
                                        | crate::state::SseEvent::Unbanned
                                );
                            if !suppressed {
                                if let Ok(json) = serde_json::to_string(&event) {
                                    yield Ok(Event::default().event(sse_event_name(&event)).data(json));
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        // v1.1-P2.4：15s → 10s，配合 max_sse_connections=5000 更快感知死连接
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keepalive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::{
        AMASEnvConfig, AuthRateLimitConfig, Config, LLMConfig, RateLimitConfig, UpdateCheckConfig,
        WorkerConfig,
    };
    use crate::store::Store;

    fn test_state() -> AppState {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = Config {
            host: "127.0.0.1".parse().unwrap(),
            port: 3000,
            log_level: "info".to_string(),
            enable_file_logs: false,
            log_dir: "./logs".to_string(),
            database_url: tmp
                .path()
                .join("realtime_guard.db")
                .to_string_lossy()
                .to_string(),
            api_only: false,
            sqlite_busy_timeout_ms: 5000,
            sqlite_connection_timeout_ms: 250,
            sqlite_pool_size: 4,
            records_outbox_async: false,
            jwt_secret: "test-jwt-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            refresh_jwt_secret: "test-refresh-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            jwt_expires_in_hours: 24,
            refresh_token_expires_in_hours: 168,
            admin_jwt_secret: "test-admin-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            admin_jwt_expires_in_hours: 2,
            cors_origin: "http://localhost:5173".to_string(),
            trust_proxy: false,
            resource_pack_trusted_hosts: Vec::new(),
            cookie_secure: false,
            self_watchdog: Default::default(),
            rate_limit: RateLimitConfig {
                window_secs: 60,
                max_requests: 100,
                anonymous_max_requests: 0,
                authenticated_max_requests: 0,
            },
            auth_rate_limit: AuthRateLimitConfig::default(),
            telemetry_rate_limit: AuthRateLimitConfig::default(),
            worker: WorkerConfig {
                is_leader: false,
                enable_llm_advisor: false,
                enable_monitoring: false,
            },
            amas: AMASEnvConfig {
                ensemble_enabled: true,
                monitor_sample_rate: 0.05,
                rejection_sample_rate: 0.0,
            },
            amas_config_file: None,
            llm: LLMConfig {
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
            update_check: UpdateCheckConfig {
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
            strict_mode: Default::default(),
            probe: Default::default(),
            limits: Default::default(),
            telemetry_retention_days: 90,
        };
        let store = Arc::new(
            Store::open(
                &config.database_url,
                config.sqlite_busy_timeout_ms,
                config.sqlite_pool_size,
            )
            .expect("open test store"),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(4);
        AppState::new(store, amas, &config, shutdown_tx, false)
    }

    #[test]
    fn sse_guard_drop_removes_empty_device_without_deadlocking() {
        let state = test_state();
        let device_id = "device-1".to_string();
        let conn_id = "conn-1".to_string();
        let (tx, _) = tokio::sync::mpsc::channel(crate::state::SSE_CONN_CHANNEL_CAP);

        state.active_sse().insert(
            device_id.clone(),
            vec![SseClientInfo {
                conn_id: conn_id.clone(),
                user_id: "user-1".to_string(),
                platform: "test".to_string(),
                connected_at: Instant::now(),
                tx,
            }],
        );
        state
            .last_heartbeat()
            .insert(device_id.clone(), Instant::now());
        state.heartbeat_miss_count().insert(device_id.clone(), 0);
        SSE_CONNECTION_COUNT.store(1, Ordering::SeqCst);

        let guard = SseGuard {
            state: state.clone(),
            device_id: Some(device_id.clone()),
            conn_id,
            user_id: "user-1".to_string(),
            user_sse_acquired: false,
        };
        drop(guard);

        assert_eq!(SSE_CONNECTION_COUNT.load(Ordering::SeqCst), 0);
        assert!(state.active_sse().get(&device_id).is_none());
        assert!(state.last_heartbeat().get(&device_id).is_none());
        assert!(state.heartbeat_miss_count().get(&device_id).is_none());
    }

    // P2：Drop 清理心跳前若同一 did 已被新连接重建条目，必须跳过清理，避免误删
    // 新连接的心跳基线导致 watchdog 误报 DataCorrupted。
    #[test]
    fn sse_guard_drop_skips_heartbeat_cleanup_when_device_reconnected() {
        let state = test_state();
        let device_id = "device-reconnect".to_string();
        let old_conn = "conn-old".to_string();
        let new_conn = "conn-new".to_string();
        let (tx_new, _) = tokio::sync::mpsc::channel(crate::state::SSE_CONN_CHANNEL_CAP);

        // 模拟：旧连接已从列表移除（列表当前只剩新连接），新连接刚写入心跳基线。
        state.active_sse().insert(
            device_id.clone(),
            vec![SseClientInfo {
                conn_id: new_conn.clone(),
                user_id: "user-1".to_string(),
                platform: "test".to_string(),
                connected_at: Instant::now(),
                tx: tx_new,
            }],
        );
        let new_hb = Instant::now();
        state.last_heartbeat().insert(device_id.clone(), new_hb);
        state.heartbeat_miss_count().insert(device_id.clone(), 0);

        // 旧连接的 guard drop：retain 后列表仍非空（新连接在），不应删除条目或心跳。
        let guard = SseGuard {
            state: state.clone(),
            device_id: Some(device_id.clone()),
            conn_id: old_conn,
            user_id: "user-1".to_string(),
            user_sse_acquired: false,
        };
        drop(guard);

        assert!(
            state.active_sse().get(&device_id).is_some(),
            "新连接条目应保留"
        );
        assert!(
            state.last_heartbeat().get(&device_id).is_some(),
            "新连接心跳基线不应被误删"
        );
        assert!(state.heartbeat_miss_count().get(&device_id).is_some());
    }

    // P1：per-user 上限到达后拒绝，释放后恢复；Drop 对称释放配额。
    #[test]
    fn per_user_sse_limit_acquire_release() {
        let state = test_state();
        let uid = "user-limit";
        assert!(state.try_acquire_user_sse(uid, 2));
        assert!(state.try_acquire_user_sse(uid, 2));
        assert!(!state.try_acquire_user_sse(uid, 2), "第三个应超限");

        state.release_user_sse(uid);
        assert!(
            state.try_acquire_user_sse(uid, 2),
            "释放一个后应可再次占用"
        );

        // Drop 释放：guard.user_sse_acquired=true 时归还配额。
        state.release_user_sse(uid);
        let guard = SseGuard {
            state: state.clone(),
            device_id: None,
            conn_id: "c".to_string(),
            user_id: uid.to_string(),
            user_sse_acquired: true,
        };
        // 当前占用 1（上一轮 acquire 后未释放）；drop 后归还，应能再占到上限。
        drop(guard);
        assert!(state.try_acquire_user_sse(uid, 1));
    }

    // P1：max=0 视为不限额。
    #[test]
    fn per_user_sse_limit_zero_is_unlimited() {
        let state = test_state();
        let uid = "user-unlimited";
        for _ in 0..10 {
            assert!(state.try_acquire_user_sse(uid, 0));
        }
    }
}
