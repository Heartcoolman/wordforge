use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use crate::auth::AdminAuthUser;
use crate::state::AppState;

use super::realtime::SSE_CONNECTION_COUNT;

fn startup_instant() -> &'static Instant {
    static INSTANCE: OnceLock<Instant> = OnceLock::new();
    INSTANCE.get_or_init(Instant::now)
}

/// sqlite 主库 + -wal + -shm 三个文件的磁盘占用之和（字节）。
/// 读取失败（路径缺失/`:memory:`/权限）返回 None，调用方应诚实展示为空，不伪造。
fn db_on_disk_size_bytes(state: &AppState) -> Option<u64> {
    let config = state.config();
    let main = std::path::Path::new(&config.database_url);
    let mut total = std::fs::metadata(main).ok()?.len();
    // sqlite WAL 模式下旁路文件 -wal / -shm 也计入实际占用
    let file_name = main.file_name().and_then(|n| n.to_str())?;
    for ext in ["-wal", "-shm"] {
        let sidecar = main.with_file_name(format!("{file_name}{ext}"));
        if let Ok(meta) = std::fs::metadata(&sidecar) {
            total += meta.len();
        }
    }
    Some(total)
}

async fn store_probe_ok(state: &AppState) -> bool {
    match state
        .run_store_task("health.db_ping", |store| store.db_ping())
        .await
    {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "Health store probe failed");
            false
        }
        Err(error) => {
            tracing::error!(error = %error, "Health store probe task failed");
            false
        }
    }
}

pub(crate) fn sse_probe_ok(state: &AppState) -> bool {
    let _ = state.maintenance_rx();
    let _ = state.update_rx();
    let _ = state.active_sse();
    let _ = state.last_heartbeat();
    let _ = state.heartbeat_miss_count();
    true
}

/// 词书中心探针结果缓存 TTL。请求路径命中即返回；超过 TTL 旁路触发一次后台刷新，
/// 当前请求仍用上次结果，境外 CDN 的网络往返永不阻塞 `/health`。
const WBC_PROBE_TTL: Duration = Duration::from_secs(30);

/// 词书中心健康探针（**非阻塞**）。返回 `(healthy, probe_skipped)`。
///
/// 原实现每次都同步发一次词书中心（境外 CDN）的 HTTPS 请求，大陆机房一次 TCP+TLS
/// 握手 0.5–2s，独力把 `/health` 与 admin `monitoring/health` 的 P99 顶到 1s+，而其余
/// 接口仅毫秒级。现改为读后台刷新的缓存：
/// - 命中未过期缓存 → 立即返回；
/// - 缓存过期 / 缺失 → 旁路 spawn 一次后台刷新（`wbc_probe_refreshing` 去重），当前请求
///   用上次结果，网络往返不进请求路径；
/// - 冷启动尚无任何样本 → 返回 `(true, true)`（乐观放行 + 标记未探测），避免冷启动窗口
///   误报 degraded；首个后台刷新落地后即转为真实结果。
pub(crate) async fn wordbook_center_probe(state: &AppState) -> (bool, bool) {
    let cached = *state.wbc_probe_cache().read().await;
    let stale = match cached {
        Some((at, ..)) => at.elapsed() >= WBC_PROBE_TTL,
        None => true,
    };
    if stale {
        trigger_wbc_probe_refresh(state.clone());
    }
    match cached {
        Some((_, healthy, skipped)) => (healthy, skipped),
        None => (true, true),
    }
}

/// 旁路触发一次词书中心探针刷新。`wbc_probe_refreshing` 保证同一时刻仅一个刷新在途；
/// 用 Drop guard 复位标志，确保探测即便 panic 也不会把刷新永久卡死。
fn trigger_wbc_probe_refresh(state: AppState) {
    if state.wbc_probe_refreshing().swap(true, Ordering::AcqRel) {
        return; // 已有刷新在途，跳过
    }
    tokio::spawn(async move {
        struct ResetGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for ResetGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _guard = ResetGuard(state.wbc_probe_refreshing().clone());
        let (healthy, skipped) = probe_wordbook_center_now(&state).await;
        *state.wbc_probe_cache().write().await = Some((Instant::now(), healthy, skipped));
    });
}

/// 实际执行一次词书中心探活（**含网络往返**），仅由 [`trigger_wbc_probe_refresh`] 的后台
/// 任务调用，不在请求路径。返回 `(healthy, probe_skipped)`；未配置 URL 时视为跳过探测的
/// 健康态 `(true, true)`。
pub(crate) async fn probe_wordbook_center_now(state: &AppState) -> (bool, bool) {
    let settings = state
        .run_store_task("health.wbc_settings", |store| store.get_system_settings())
        .await
        .ok()
        .and_then(Result::ok);

    let url = settings.and_then(|s| s.wordbook_center_url);
    let Some(base_url) = url else {
        return (true, true);
    };

    // connect_timeout 显式钳住"建连"阶段：黑洞地址（如不可达的词书中心）下,总 timeout 在
    // 部分平台不可靠地中断 TCP connect,会拖到 OS 默认连接超时（数秒~分钟）。加 3s
    // connect_timeout 让探针确定性快失败。注意:本函数运行在后台刷新任务里,即便慢到 5s
    // 也不进请求路径,不影响 /health 的 SLO。
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, false),
    };

    let probe_url = format!("{}/index.json", base_url.trim_end_matches('/'));
    match client.get(&probe_url).send().await {
        Ok(resp) if resp.status().is_success() => (true, false),
        _ => (false, false),
    }
}

pub fn router() -> Router<AppState> {
    let _ = startup_instant();

    Router::new()
        .route("/", get(health_check))
        .route("/live", get(liveness))
        .route("/ready", get(readiness))
        .route("/database", get(database_health))
        .route("/metrics", get(metrics))
}

pub async fn health_check(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    // M0-R4：apply swapping 期间维护模式激活，/health 返回 503 告知负载均衡器下线。
    // fork-exec 后新进程启动完成（.maintenance.flag 清理）再恢复 200。
    if state.is_maintenance() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "maintenance": true,
                "message": "服务器升级中，请稍后重试"
            })),
        )
            .into_response();
    }

    let store_healthy = store_probe_ok(&state).await;
    // 数据库磁盘占用：直接读 sqlite 主库文件大小（含 -wal/-shm 旁路文件）。
    // 失败（文件缺失/权限）时返回 None，前端展示 "—"，不伪造数据。
    let db_size_bytes = db_on_disk_size_bytes(&state);
    // SLO 可用性：源自 http_metrics 滚动聚合器（真实 5xx 错误率，非伪造）。
    // 请求 30d 窗口；hour 层 HOUR_CAP=30d 且经 D3 持久化(availability_rollup)+启动回灌，
    // 故跨重启可累积达 30d。仍按实际可得窗口(min(window, 自首条记录以来))透出 effectiveSecs，
    // 前端据此动态标注真实窗口（运行不足 30d 时如实降级，不冒充）。
    // 无任何请求样本时 pct 无意义，省略为 null，前端展示 "—"。
    let availability = {
        let agg = crate::middleware::http_metrics::aggregate(30 * 24 * 3600);
        if agg.total_requests > 0 {
            Some(serde_json::json!({
                "pct": agg.availability_pct,
                "effectiveSecs": agg.effective_secs,
                "totalRequests": agg.total_requests,
            }))
        } else {
            None
        }
    };
    let amas_healthy = state.amas().is_healthy();
    let sse_healthy = sse_probe_ok(&state);
    let (wbc_healthy, wbc_probe_skipped) = wordbook_center_probe(&state).await;
    let clock_status = state.clock_health().status().await;
    let clock_drift_secs = state.clock_health().drift_secs();
    let clock_last_check_at = state.clock_health().last_check_at();
    let clock_degraded = matches!(clock_status, crate::clock_health::ClockCheckStatus::Drifted);

    let status = if !store_healthy {
        "down"
    } else if !amas_healthy || !sse_healthy || !wbc_healthy || clock_degraded {
        "degraded"
    } else {
        "ok"
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": status,
            "version": env!("CARGO_PKG_VERSION"),
            "uptimeSecs": startup_instant().elapsed().as_secs(),
            "serverTime": chrono::Utc::now().timestamp(),
            // 数据库磁盘占用字节数；无法读取时为 null。
            "dbSizeBytes": db_size_bytes,
            // 可用性：真实 5xx 错误率聚合；含实际测量窗口 effectiveSecs（≤30d）。无样本时为 null。
            "availability": availability,
            "services": {
                "store": { "healthy": store_healthy },
                "amas": { "healthy": amas_healthy },
                "sse": {
                    "healthy": sse_healthy,
                    "activeConnections": SSE_CONNECTION_COUNT.load(Ordering::Relaxed),
                    "activeDevices": state.active_sse().len(),
                },
                "wordbookCenter": {
                    "healthy": wbc_healthy,
                    "probeSkipped": wbc_probe_skipped
                },
                // 时钟健康状态：driftSecs = 服务器时钟 - 公网时钟（HTTP Date 取样）
                // status=drifted 时说明本机 NTP 未同步，会造成 JWT 在客户端视角下失效
                "clock": {
                    "status": clock_status,
                    "driftSecs": clock_drift_secs,
                    "lastCheckAt": if clock_last_check_at == 0 { None } else { Some(clock_last_check_at) },
                    "thresholdSecs": crate::clock_health::DRIFT_DEGRADED_THRESHOLD_SECS,
                },
            }
        })),
    )
        .into_response()
}

pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

pub async fn readiness(State(state): State<AppState>) -> StatusCode {
    if store_probe_ok(&state).await {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

pub async fn database_health(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let start = Instant::now();
    let healthy = store_probe_ok(&state).await;
    let latency_us = start.elapsed().as_micros() as u64;

    Json(serde_json::json!({
        "healthy": healthy,
        "latencyUs": latency_us,
        // W5-1：本字段是「本次单次探活」的布尔投影（0=本次成功 / 1=本次失败），**不是**真实的
        // 连续失败计数——真实连续计数需 AppState 加共享计数器，价值与成本不匹配。原前端消费方
        // （admin-ui/src/api/health.ts）已是死代码并删除，对运维不可见。字段保守保留：本端点
        // /health/database 公开可达，删字段前须跨仓确认 wordforge-web / 监控脚本未直接读它。
        "consecutiveFailures": if healthy { 0 } else { 1 },
    }))
}

pub async fn metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let snapshot = state.amas().metrics_registry().snapshot();
    Json(serde_json::json!({
        "algorithms": snapshot,
    }))
}
