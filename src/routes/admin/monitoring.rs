use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::Router;
use chrono::{Duration as ChronoDuration, Utc};
use cron::Schedule;
use futures::Stream;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::json;
use sysinfo::{Disks, Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::sync::broadcast::error::RecvError;

use crate::auth::AdminAuthUser;
use crate::logging_buffer::{self, LogFilter};
use crate::response::{ok, AppError};
use crate::routes::health::{sse_probe_ok, wordbook_center_probe};
use crate::routes::realtime::SSE_CONNECTION_COUNT;
use crate::state::AppState;

/// m023:进程级 sysinfo 单例。sysinfo 要求两次 refresh 之间 ≥200ms 才能拿到 CPU%,
/// 所以挂全局 + Mutex 保持采样上下文(否则每次 new 出来 CPU 永远是 0)。
static SYS_SNAPSHOT: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    s.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    Mutex::new(s)
});

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(system_health))
        .route("/database", get(database_stats))
        .route("/check-update", get(check_update))
        // M1-A5：worker 执行状态区
        .route("/workers", get(worker_status))
        // Flow 端点流量表：按 (method,route) 聚合的累积延迟分位/错误率/rpm
        .route("/endpoints", get(endpoint_metrics))
        // Worker cron 卡：cadence + next-run + 健康判定（cron 表达式推算下次执行）
        .route("/workers/cron", get(worker_cron))
        // BA3a：worker 运行历史时间线（worker_runs，append-only）
        .route("/workers/history", get(worker_history))
        // worker SLO：按 worker 聚合成功率 + 成功 run 耗时分位（p50/p95/p99/max）
        .route("/workers/slo", get(worker_slo))
        // BA3a：进程内 CPU/内存/load 资源历史（sparkline + 时序）
        .route("/resource-history", get(resource_history))
        // BA3b：records 热路径分阶段管线指标 + 最近一条摄取瀑布 trace
        .route("/pipeline", get(pipeline_metrics))
        .route("/trace/latest", get(trace_latest))
        // m066:records 摄取分步瀑布 trace 环形缓冲快照（最新在前）
        .route("/traces", get(traces))
        // M0-P5：监控页对齐设计图新增——滚动请求指标 / 实时日志 / 派生告警时间线
        .route("/requests", get(request_metrics))
        .route("/logs", get(recent_logs))
        // P1：实时日志 SSE + 每日归档（列目录 / 读单日）
        .route("/logs/stream", get(logs_stream))
        .route("/logs/archive", get(logs_archive))
        .route("/logs/archive/:date", get(logs_archive_date))
        // P1：每小时可用率桶（availability_rollup）
        .route("/availability-hourly", get(availability_hourly))
        .route("/events", get(alert_events))
        // W1-2：outbox 死信运维——明细列表 + 人工重投 / 丢弃
        .route("/dead-letter", get(dead_letter_list))
        .route("/dead-letter/:id/requeue", post(dead_letter_requeue))
        .route("/dead-letter/:id", delete(dead_letter_purge))
}

// B62: System health monitoring
async fn system_health(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let size_on_disk = state
        .run_store_task("admin.monitoring.db_size_bytes", |store| {
            store.db_size_bytes()
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(0);
    let uptime_secs = state.uptime_secs();
    let store_probe_ok = state
        .run_store_task("admin.monitoring.db_ping", |store| store.db_ping())
        .await
        .is_ok_and(|result| result.is_ok());
    let amas_healthy = state.amas().is_healthy();
    let sse_healthy = sse_probe_ok(&state);
    let (wbc_healthy, wbc_probe_skipped) = wordbook_center_probe(&state).await;

    // 饱和度/延迟 SLO 纳入健康判定:过载(P99 暴涨/在途堆积/写池耗尽/SSE 近上限)即便无 5xx
    // 也应降级,避免「可用性 100% 掩盖过载」。资源采样上移以参与判定。
    let resources = sample_resources(&state).await;
    let overload = detect_overload(&state, &resources);

    let status = if !store_probe_ok {
        "down"
    } else if !amas_healthy || !sse_healthy || !wbc_healthy || overload.degraded {
        "degraded"
    } else {
        "healthy"
    };

    // M0-P1 全局计数器快照：计算生命周期内 5xx 错误率（0.0–1.0）
    let (total_req, total_5xx) = crate::metrics_counters::snapshot();
    let error_rate = if total_req > 0 {
        total_5xx as f64 / total_req as f64
    } else {
        0.0
    };

    // m023:资源采样已上移至 status 判定前（参与过载/饱和度判定）。

    // S2-1：outbox 异步消费健康（待处理数 / lag 秒 / 死信累计）。读失败降级为默认零值。
    let outbox = state
        .run_store_task("admin.monitoring.outbox_stats", |store| {
            store.outbox_stats()
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();

    Ok(ok(serde_json::json!({
        "status": status,
        "storeProbeOk": store_probe_ok,
        "dbSizeBytes": size_on_disk,
        "uptimeSecs": uptime_secs,
        "version": env!("GIT_VERSION"),
        "errorRate": error_rate,
        "resources": resources,
        "saturation": overload.to_json(),
        "outbox": outbox,
        "services": {
            "amas": { "healthy": amas_healthy },
            "sse": {
                "healthy": sse_healthy,
                "activeConnections": SSE_CONNECTION_COUNT.load(Ordering::Relaxed),
                "activeDevices": state.active_sse().len(),
                "maxConnections": state.config().limits.max_sse_connections,
            },
            "wordbookCenter": {
                "healthy": wbc_healthy,
                "probeSkipped": wbc_probe_skipped,
            },
        },
    })))
}

/// 过载/饱和度判定:延迟 SLO 违约 + 在途堆积 + 写池耗尽 + SSE 近上限。
/// 把「慢/满但不报 5xx」的过载纳入 health.status=degraded（不再被 5xx 口径的可用性掩盖）。
struct Overload {
    degraded: bool,
    p99_ms: f64,
    latency_breach: bool,
    inflight: u64,
    inflight_high: bool,
    pool_exhausted: bool,
    sse_near_cap: bool,
}

impl Overload {
    fn to_json(&self) -> serde_json::Value {
        let mut reasons: Vec<&str> = Vec::new();
        if self.latency_breach {
            reasons.push("latency_p99");
        }
        if self.inflight_high {
            reasons.push("inflight_backlog");
        }
        if self.pool_exhausted {
            reasons.push("db_pool_exhausted");
        }
        if self.sse_near_cap {
            reasons.push("sse_near_cap");
        }
        serde_json::json!({
            "degraded": self.degraded,
            "p99Ms": self.p99_ms,
            "inflightRequests": self.inflight,
            "reasons": reasons,
        })
    }
}

/// 1h 窗口 P99 超此阈值（且样本足够）视为延迟 SLO 违约。与前端 P99 卡红线一致。
const OVERLOAD_P99_MS: f64 = 800.0;
const OVERLOAD_MIN_SAMPLES: u64 = 50;
/// 在途请求堆积阈值（正常远低于此；堆积=请求排在 SQLite/信号量后面）。
const OVERLOAD_INFLIGHT: u64 = 64;

fn detect_overload(state: &AppState, resources: &serde_json::Value) -> Overload {
    // 延迟 SLO（1h 滚动窗口 P99）。样本不足时不判违约,避免冷启动误报。
    let agg = crate::middleware::http_metrics::aggregate(3600);
    let latency_breach =
        agg.total_requests >= OVERLOAD_MIN_SAMPLES && agg.p99_ms > OVERLOAD_P99_MS;

    let inflight = resources
        .get("inflightRequests")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let inflight_high = inflight >= OVERLOAD_INFLIGHT;

    // r2d2 写池耗尽:连接占满且无空闲。
    let pool_exhausted = resources
        .get("pool")
        .and_then(|p| {
            let conns = p.get("connections")?.as_u64()?;
            let max = p.get("max")?.as_u64()?;
            let idle = p.get("idle")?.as_u64()?;
            Some(max > 0 && conns >= max && idle == 0)
        })
        .unwrap_or(false);

    // SSE 长连接逼近配置上限。
    let active_sse = SSE_CONNECTION_COUNT.load(Ordering::Relaxed);
    let max_sse = state.config().limits.max_sse_connections;
    let sse_near_cap = max_sse > 0 && active_sse as f64 >= max_sse as f64 * 0.9;

    let degraded = latency_breach || inflight_high || pool_exhausted || sse_near_cap;
    Overload {
        degraded,
        p99_ms: agg.p99_ms,
        latency_breach,
        inflight,
        inflight_high,
        pool_exhausted,
        sse_near_cap,
    }
}

async fn database_stats(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let size = state
        .run_store_task("admin.monitoring.database_stats", |store| {
            Ok::<_, crate::store::StoreError>((
                store.db_size_bytes()?,
                store.db_table_list()?,
                store.db_page_size()?,
                store.db_page_count()?,
                store.db_wal_enabled()?,
                store.db_wal_size_bytes()?,
            ))
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or((0, Vec::new(), 0, 0, false, 0));
    let (size, tables, page_size, page_count, wal_enabled, wal_size) = size;

    Ok(ok(serde_json::json!({
        "sizeOnDisk": size,
        "tableCount": tables.len(),
        "tables": tables,
        "pageSize": page_size,
        "pageCount": page_count,
        "walEnabled": wal_enabled,
        "walSizeBytes": wal_size,
    })))
}

async fn check_update(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let cache_ttl = Duration::from_secs(state.config().update_check.cache_ttl_secs);
    {
        let cache = state.update_cache().read().await;
        if let Some((cached_at, ref data)) = *cache {
            if cached_at.elapsed() < cache_ttl {
                return Ok(ok(data.clone()));
            }
        }
    }

    let git_version = env!("GIT_VERSION");
    let current_version = git_version.trim_start_matches('v');
    let config = state.config();
    let api_url = config.update_check.api_url.trim();

    if api_url.is_empty() {
        let fallback = update_check_fallback(git_version, current_version);
        *state.update_cache().write().await = Some((Instant::now(), fallback.clone()));
        return Ok(ok(fallback));
    }

    match fetch_latest_release(api_url, git_version, current_version).await {
        Ok(data) => {
            *state.update_cache().write().await = Some((Instant::now(), data.clone()));
            Ok(ok(data))
        }
        Err(e) => {
            tracing::warn!("Failed to check for updates: {e}");
            let fallback = update_check_fallback(git_version, current_version);
            *state.update_cache().write().await = Some((Instant::now(), fallback.clone()));
            Ok(ok(fallback))
        }
    }
}

fn update_check_fallback(git_version: &str, current_version: &str) -> serde_json::Value {
    serde_json::json!({
        "currentVersion": git_version,
        "latestVersion": current_version,
        "hasUpdate": false,
        "releaseUrl": null,
        "releaseNotes": null,
    })
}

async fn fetch_latest_release(
    api_url: &str,
    git_version: &str,
    current_version: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("wordforge-update-checker")
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client.get(api_url).send().await?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()).into());
    }

    let body: serde_json::Value = resp.json().await?;
    let tag_name = body["tag_name"].as_str().unwrap_or("");
    let latest_version = tag_name.trim_start_matches('v');
    let has_update = is_newer(latest_version, current_version);

    Ok(serde_json::json!({
        "currentVersion": git_version,
        "latestVersion": latest_version,
        "hasUpdate": has_update,
        "releaseUrl": body["html_url"].as_str(),
        "releaseNotes": body["body"].as_str(),
    }))
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

/// m023:Dashboard 系统资源条。所有字段失败兜底 null,不让监控自身故障穿透 health。
async fn sample_resources(state: &AppState) -> serde_json::Value {
    // 1) sysinfo:进程 CPU% + RSS。两次 refresh 间隔填 200ms 由 sysinfo 自己保证(全局单例第 2 次起有效)
    let pid = Pid::from_u32(std::process::id());
    let (cpu_pct, rss_bytes) = {
        let mut sys = match SYS_SNAPSHOT.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        sys.process(pid)
            .map(|p| (Some(p.cpu_usage() as f64), Some(p.memory())))
            .unwrap_or((None, None))
    };

    // 2) r2d2 池占用 + WAL 文件大小（同一 store task,避免二次调度）
    let (pool, wal_size) = state
        .run_store_task("admin.monitoring.resources_db", |store| {
            Ok::<_, crate::store::StoreError>((store.pool_status(), store.db_wal_size_bytes()?))
        })
        .await
        .ok()
        .and_then(Result::ok)
        .map(|(p, w)| (Some(p), Some(w)))
        .unwrap_or((None, None));

    // 3) 磁盘:工作目录所在磁盘 free/total。Disks::new_with_refreshed_list 拿全部挂载点,
    //    选第一个 mount_point 是当前 cwd 前缀的(覆盖大多数 Linux/macOS 部署形态)
    let cwd = std::env::current_dir().ok();
    let (disk_total, disk_free) = {
        let disks = Disks::new_with_refreshed_list();
        let mut best: Option<&sysinfo::Disk> = None;
        if let Some(ref c) = cwd {
            for d in disks.list() {
                if c.starts_with(d.mount_point()) {
                    let take = match best {
                        None => true,
                        Some(b) => {
                            d.mount_point().as_os_str().len() > b.mount_point().as_os_str().len()
                        }
                    };
                    if take {
                        best = Some(d);
                    }
                }
            }
        }
        best.map(|d| (Some(d.total_space()), Some(d.available_space())))
            .unwrap_or((None, None))
    };

    // 4) 系统总内存（让 RSS 进度条有真实分母）。新建轻量 System 只刷内存。
    let mem_total = {
        let mut s = System::new_with_specifics(
            RefreshKind::new().with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
        s.refresh_memory();
        let t = s.total_memory();
        if t > 0 {
            Some(t)
        } else {
            None
        }
    };

    // 5) 在途请求数（饱和度信号）+ nginx 边缘指标（连接层真实压力，env 门控）
    let inflight = crate::metrics_counters::inflight();
    let nginx_edge = sample_nginx_edge().await;

    // 6) 系统负载均值（1/5/15 分钟）。sysinfo 静态方法，Linux/macOS/FreeBSD 有效；
    //    Windows 恒返回 0（无 load avg 概念），前端据此判定是否渲染。
    let load_avg = System::load_average();

    serde_json::json!({
        "cpuPct": cpu_pct,
        "memoryRssBytes": rss_bytes,
        "memoryTotalBytes": mem_total,
        "diskTotalBytes": disk_total,
        "diskFreeBytes": disk_free,
        "loadAvg": {
            "one": load_avg.one,
            "five": load_avg.five,
            "fifteen": load_avg.fifteen,
        },
        "walSizeBytes": wal_size,
        "pool": pool.map(|p| serde_json::json!({
            "max": p.max,
            "connections": p.connections,
            "idle": p.idle,
        })),
        "inflightRequests": inflight,
        "nginxEdge": nginx_edge,
    })
}

/// 抓取 nginx stub_status（默认 localhost）并解析。
/// 未配置 `NGINX_STATUS_URL` env 时返回 null（功能关闭，无部署形态依赖）；
/// 抓取/解析失败同样兜底 null，不让边缘探针穿透 health 本体。
async fn sample_nginx_edge() -> Option<serde_json::Value> {
    let url = std::env::var("NGINX_STATUS_URL")
        .ok()
        .filter(|s| !s.is_empty())?;
    let body = reqwest::Client::new()
        .get(&url)
        .timeout(Duration::from_millis(800))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    parse_stub_status(&body)
}

/// 解析 nginx stub_status 文本：
/// ```text
/// Active connections: 291
/// server accepts handled requests
///  16630948 16630948 31070465
/// Reading: 6 Writing: 179 Waiting: 106
/// ```
/// `dropped = accepts - handled`（累计被丢弃的连接）。任一关键行缺失即返回 None。
fn parse_stub_status(body: &str) -> Option<serde_json::Value> {
    let mut active = None;
    let mut reading = None;
    let mut writing = None;
    let mut waiting = None;
    let mut accepts = None;
    let mut handled = None;
    let mut requests = None;

    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("Active connections:") {
            active = rest.trim().parse::<u64>().ok();
        } else if l.starts_with("server accepts") {
            // 计数三元组在下一行
            if let Some(data) = lines.next() {
                let nums: Vec<u64> = data
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                if nums.len() >= 3 {
                    accepts = Some(nums[0]);
                    handled = Some(nums[1]);
                    requests = Some(nums[2]);
                }
            }
        } else if l.starts_with("Reading:") {
            let nums: Vec<u64> = l
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            if nums.len() >= 3 {
                reading = Some(nums[0]);
                writing = Some(nums[1]);
                waiting = Some(nums[2]);
            }
        }
    }

    let active = active?;
    let accepts = accepts?;
    let handled = handled?;
    let requests = requests?;
    Some(serde_json::json!({
        "active": active,
        "accepts": accepts,
        "handled": handled,
        "requests": requests,
        "dropped": accepts.saturating_sub(handled),
        "reading": reading,
        "writing": writing,
        "waiting": waiting,
    }))
}

/// M1-A5：返回所有 worker 的最后执行记录，供 admin 控制台 "Worker 状态" 区展示。
async fn worker_status(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let rows = state
        .run_store_task("admin.monitoring.workers", |store| {
            store.list_worker_last_run()
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;

    let workers: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "workerName": r.worker_name,
                "lastRunAt": r.last_run_at,
                "lastDurationMs": r.last_duration_ms,
                "lastOutcome": r.last_outcome,
                "lastError": r.last_error,
            })
        })
        .collect();

    Ok(ok(serde_json::json!({ "workers": workers })))
}

/// GET /api/admin/monitoring/endpoints —— 端点流量表。
/// 按 (method, route) 聚合 http_metrics REGISTRY，给出累积（自进程启动以来）的
/// p50/p95/p99 延迟、5xx 错误率、rpm。累积口径无时间窗口。
async fn endpoint_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let uptime = state.uptime_secs();
    let endpoints = crate::middleware::http_metrics::endpoints_snapshot(uptime).await;
    Ok(ok(json!({ "endpoints": endpoints })))
}

/// GET /api/admin/monitoring/workers/cron —— worker cron 卡。
/// cron 表达式来自 planned_jobs 的单一事实源（worker_cron_specs），next-run 由
/// cron 表达式推算；last-run 来自 worker_last_run；健康判定复用 watchdog 静默阈值。
async fn worker_cron(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use std::str::FromStr;

    let last_runs = state
        .run_store_task("admin.monitoring.workers_cron", |store| {
            store.list_worker_last_run()
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;
    let last_map: std::collections::HashMap<String, _> = last_runs
        .into_iter()
        .map(|r| (r.worker_name.clone(), r))
        .collect();

    let cfg = state.config();
    // 条件 worker 的启用判定：生产中 leader 启动时恒注入 watchdog/health/canary state，
    // 故据 is_leader 还原；update_checker 据其专用配置开关。
    let leader = cfg.worker.is_leader;
    let specs = crate::workers::worker_cron_specs(
        &cfg.worker,
        cfg.update_check.worker_enabled,
        leader,
        leader,
        leader,
    );
    let thresholds = crate::workers::scheduler_health_watchdog::expected_max_silence_secs();
    let now = Utc::now();
    let now_unix = now.timestamp();

    let workers: Vec<serde_json::Value> = specs
        .into_iter()
        .map(|spec| {
            let name = spec.name.as_str();
            let next_run_at = Schedule::from_str(spec.cron)
                .ok()
                .and_then(|s| s.after(&now).next())
                .map(|dt| dt.timestamp());
            let last = last_map.get(name);
            let last_run_at = last.map(|r| r.last_run_at);
            let last_outcome = last.map(|r| r.last_outcome.clone());
            let last_duration_ms = last.map(|r| r.last_duration_ms);
            let last_error = last.and_then(|r| r.last_error.clone());
            let panic_count = last.map(|r| r.panic_count).unwrap_or(0);

            // 健康：最近一次成功 且 静默时长 ≤ watchdog 阈值（cron 间隔 × 3）。
            // 无 last-run 记录或无阈值映射时按未知处理（healthy=false）。
            let outcome_ok = last_outcome
                .as_deref()
                .map(|o| matches!(o, "success" | "ok" | "completed"))
                .unwrap_or(false);
            let within_silence = match (last_run_at, thresholds.get(name)) {
                (Some(lr), Some(max)) if lr > 0 => {
                    (now_unix.saturating_sub(lr)) as u64 <= *max
                }
                _ => false,
            };
            let healthy = spec.enabled && outcome_ok && within_silence;

            serde_json::json!({
                "workerName": name,
                "cron": spec.cron,
                "enabled": spec.enabled,
                "cadenceHuman": cadence_human(spec.cron),
                "nextRunAt": next_run_at,
                "lastRunAt": last_run_at,
                "lastOutcome": last_outcome,
                "lastDurationMs": last_duration_ms,
                "lastError": last_error,
                "panicCount": panic_count,
                "healthy": healthy,
            })
        })
        .collect();

    Ok(ok(json!({ "workers": workers })))
}

/// 已知 cron 表达式 → 人类可读节奏文案（仅覆盖 planned_jobs 实际使用的表达式，纯展示）。
fn cadence_human(cron: &str) -> &'static str {
    match cron {
        "0 0 * * * *" => "每小时",
        "0 30 * * * *" => "每小时（第 30 分）",
        "0 */5 * * * *" => "每 5 分钟",
        "0 */10 * * * *" => "每 10 分钟",
        "0 */20 * * * *" => "每 20 分钟",
        "0 * * * * *" => "每分钟",
        "0 0 */1 * * *" => "每小时",
        "0 30 6 * * *" => "每日 06:30",
        "0 0 0 * * *" => "每日 00:00",
        "0 0 1 * * *" => "每日 01:00",
        "0 0 5 * * 1" => "每周一 05:00",
        "0 0 5 * * SUN" => "每周日 05:00",
        "0 30 6 * * 1" => "每周一 06:30",
        "0 0 3 1 * *" => "每月 1 日 03:00",
        _ => "自定义",
    }
}

// ─────────────── BA3a：worker 运行历史 + 资源历史 ───────────────

#[derive(Debug, Deserialize)]
struct WorkerHistoryQuery {
    worker: Option<String>,
    limit: Option<u32>,
}

/// GET /api/admin/monitoring/workers/history —— worker 运行历史时间线（worker_runs DESC）。
/// `worker` 缺省返回全部 worker；`limit` 默认 100，夹到 1..=1000。
async fn worker_history(
    _admin: AdminAuthUser,
    Query(q): Query<WorkerHistoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let worker = q.worker.clone();
    let runs = state
        .run_store_task("admin.monitoring.workers_history", move |store| {
            store.list_worker_runs(worker.as_deref(), limit)
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;
    let runs_json: Vec<serde_json::Value> = runs
        .into_iter()
        .map(|r| {
            json!({
                "workerName": r.worker_name,
                "ranAt": r.ran_at,
                "durationMs": r.duration_ms,
                "outcome": r.outcome,
                "error": r.error,
            })
        })
        .collect();
    Ok(ok(json!({ "runs": runs_json })))
}

#[derive(Debug, Deserialize)]
struct WorkerSloQuery {
    worker: Option<String>,
    window: Option<u32>,
}

/// nearest-rank 百分位：`sorted` 必须升序且非空，`p` ∈ (0,100]。
/// 取排名 ceil(p/100 × N) 处的样本（1-indexed，越界夹到末位）。
fn nearest_rank(sorted: &[i64], p: f64) -> i64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted[idx]
}

/// GET /api/admin/monitoring/workers/slo —— 按 worker 聚合 SLO。
/// `window` 取最近多少条 worker_runs（默认 500，夹到 1..=5000）；`worker` 给定时仅返回该 worker。
/// 每个 worker 输出：总数 / success / failure / skipped 计数、成功率（0.0–1.0），
/// 以及「成功且耗时非空」样本的 p50/p95/p99/max（nearest-rank，毫秒）。无成功样本时分位为 null。
async fn worker_slo(
    _admin: AdminAuthUser,
    Query(q): Query<WorkerSloQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let window = q.window.unwrap_or(500).clamp(1, 5000);
    let worker = q.worker.clone();
    let runs = state
        .run_store_task("admin.monitoring.workers_slo", move |store| {
            store.list_worker_runs(worker.as_deref(), window)
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;

    // 按 worker_name 分组聚合；BTreeMap 的 key 序即「按 worker 名升序」。
    #[derive(Default)]
    struct Agg {
        count: u64,
        success: u64,
        failure: u64,
        skipped: u64,
        /// 仅 success 且 duration_ms 非空的耗时样本。
        durations: Vec<i64>,
    }
    let mut groups: std::collections::BTreeMap<String, Agg> = std::collections::BTreeMap::new();
    for r in runs {
        let g = groups.entry(r.worker_name).or_default();
        g.count += 1;
        match r.outcome.as_str() {
            "success" => {
                g.success += 1;
                if let Some(d) = r.duration_ms {
                    g.durations.push(d);
                }
            }
            "failure" => g.failure += 1,
            "skipped" => g.skipped += 1,
            _ => {}
        }
    }

    let workers: Vec<serde_json::Value> = groups
        .into_iter()
        .map(|(name, mut g)| {
            g.durations.sort_unstable();
            let (p50, p95, p99, max_ms) = if g.durations.is_empty() {
                (None, None, None, None)
            } else {
                (
                    Some(nearest_rank(&g.durations, 50.0)),
                    Some(nearest_rank(&g.durations, 95.0)),
                    Some(nearest_rank(&g.durations, 99.0)),
                    Some(*g.durations.last().unwrap()),
                )
            };
            // 成功率取比值（0.0–1.0，与 /health errorRate 同口径），保留 4 位小数。
            let success_rate = if g.count == 0 {
                0.0
            } else {
                (g.success as f64 / g.count as f64 * 10000.0).round() / 10000.0
            };
            json!({
                "worker": name,
                "count": g.count,
                "success": g.success,
                "failure": g.failure,
                "skipped": g.skipped,
                "successRate": success_rate,
                "p50Ms": p50,
                "p95Ms": p95,
                "p99Ms": p99,
                "maxMs": max_ms,
            })
        })
        .collect();

    Ok(ok(json!({ "workers": workers })))
}

/// GET /api/admin/monitoring/resource-history —— 进程 CPU/RSS/load 时序（oldest-first）。
/// mem 以字节返回（与 /health resources.memoryRssBytes 同口径）。
async fn resource_history(
    _admin: AdminAuthUser,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let samples = crate::resource_sampler::snapshot();
    let mut cpu = Vec::with_capacity(samples.len());
    let mut mem = Vec::with_capacity(samples.len());
    let mut load_one = Vec::with_capacity(samples.len());
    let mut ts_ms = Vec::with_capacity(samples.len());
    for s in samples {
        cpu.push(s.cpu_pct);
        mem.push(s.mem_rss_bytes);
        load_one.push(s.load_one);
        ts_ms.push(s.ts_ms);
    }
    Ok(ok(json!({
        "cpu": cpu,
        "mem": mem,
        "loadOne": load_one,
        "tsMs": ts_ms,
    })))
}

// ─────────────── M0-P5：监控页对齐设计图新增端点 ───────────────

#[derive(Debug, Deserialize)]
struct WindowQuery {
    window: Option<String>,
}

/// 设计图分段控件 15m / 1h / 6h / 24h / 7d → 秒。未识别回退 1h。
fn parse_window_secs(w: Option<&str>) -> u64 {
    match w.unwrap_or("1h") {
        "15m" => 15 * 60,
        "1h" => 3600,
        "6h" => 6 * 3600,
        "24h" => 24 * 3600,
        "7d" => 7 * 24 * 3600,
        _ => 3600,
    }
}

/// GET /api/admin/monitoring/requests?window=1h —— 滚动请求指标。
/// 喂设计图 SLO 卡(P50/P99/错误率/可用性)、请求延迟图(QPS+P99 时序)、
/// axum 服务行(rps)。可用性按实际可得窗口如实标注(见 effectiveSecs)。
async fn request_metrics(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let secs = parse_window_secs(q.window.as_deref());
    Ok(ok(crate::middleware::http_metrics::aggregate(secs)))
}

/// GET /api/admin/monitoring/pipeline?window=1h —— records 摄取热路径分阶段管线指标。
/// 数据来自 process_single_record 各阶段（amas/sqlite/sse，async 路径含 outbox）的进程内
/// 埋点（stage_metrics），按窗口聚合 per-stage {rpm,p50,p95,p99,max,errorRate}。
/// 重启清零；尚无埋点时 stages 为空数组（前端渲染「暂无已埋点阶段」）。
async fn pipeline_metrics(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let secs = parse_window_secs(q.window.as_deref());
    Ok(ok(json!({ "stages": crate::stage_metrics::aggregate(secs) })))
}

/// GET /api/admin/monitoring/trace/latest —— 最近一条 records 摄取的分步瀑布 trace。
/// 无（启动后尚无记录被处理）时返回 data:null（前端渲染「暂无最近记录」，非红错）。
async fn trace_latest(
    _admin: AdminAuthUser,
) -> Result<impl axum::response::IntoResponse, AppError> {
    Ok(ok(crate::stage_metrics::latest_trace()))
}

#[derive(Debug, Deserialize)]
struct TracesQuery {
    limit: Option<usize>,
}

/// GET /api/admin/monitoring/traces?limit=100 —— records 摄取分步瀑布 trace 环形缓冲快照（最新在前）。
/// 环形缓冲容量有界（TRACE_CAP），limit 默认 100、夹到 1..=1000，实际返回不超过现有条数。
async fn traces(
    _admin: AdminAuthUser,
    Query(q): Query<TracesQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    Ok(ok(json!({ "traces": crate::stage_metrics::snapshot_traces(limit) })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsQuery {
    limit: Option<usize>,
    level: Option<String>,
    target: Option<String>,
    q: Option<String>,
    request_id: Option<String>,
    since_ms: Option<i64>,
}

/// 把空白/空串归一化为 None，避免空过滤条件污染查询。
fn opt_clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 原始日志访问元审计(A8)：单层鉴权下任一 admin 可读全量请求数据/PII，故对原始日志/流/归档
/// 访问写一条元审计。尽力而为(spawn_blocking 旁路, 失败忽略)，不给响应加延迟、不阻断。
fn audit_raw_log_access(
    state: &AppState,
    admin_id: &str,
    action: &'static str,
    meta: serde_json::Value,
) {
    let store = state.store().clone();
    let admin_id = admin_id.to_string();
    tokio::task::spawn_blocking(move || {
        let _ = store.insert_admin_audit(&admin_id, action, Some("logs"), None, Some(&meta));
    });
}

/// GET /api/admin/monitoring/logs?limit&level&target&q&requestId&sinceMs ——
/// 进程内日志环形缓冲快照(最新在前)，支持级别/target 子串/message 子串/request_id/时间窗过滤。
async fn recent_logs(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 3000);
    let filter = LogFilter {
        level: opt_clean(q.level),
        target: opt_clean(q.target),
        q: opt_clean(q.q),
        request_id: opt_clean(q.request_id),
        since_ms: q.since_ms,
    };
    audit_raw_log_access(
        &state,
        &admin.admin_id,
        "read_raw_logs",
        json!({ "level": filter.level, "target": filter.target, "q": filter.q,
                "requestId": filter.request_id, "limit": limit }),
    );
    let logs = logging_buffer::snapshot_filtered(limit, &filter);
    Ok(ok(json!({ "logs": logs })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogStreamQuery {
    replay: Option<usize>,
    level: Option<String>,
    target: Option<String>,
    q: Option<String>,
    request_id: Option<String>,
}

/// GET /api/admin/monitoring/logs/stream —— 实时日志 SSE（先回放后续传）。
/// 连接即回放 `snapshot_filtered(replay)`（旧→新）作 event:"log"；随后订阅 broadcast 续传，
/// 命中过滤的记录发 "log"，落后丢弃发 "dropped"{droppedCount}。记录 seq 供前端按序去重。
/// 注意：循环内绝不调用 `tracing::*`，避免反馈环 / RING 锁竞争。
async fn logs_stream(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<LogStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let replay = q.replay.unwrap_or(200).min(3000);
    let filter = LogFilter {
        level: opt_clean(q.level),
        target: opt_clean(q.target),
        q: opt_clean(q.q),
        request_id: opt_clean(q.request_id),
        since_ms: None,
    };
    audit_raw_log_access(
        &state,
        &admin.admin_id,
        "stream_raw_logs",
        json!({ "level": filter.level, "target": filter.target, "q": filter.q,
                "requestId": filter.request_id, "replay": replay }),
    );
    // 先订阅再取快照：保证回放与续传之间无空窗（重叠记录由前端按 seq 去重）。
    let mut rx = logging_buffer::subscribe();
    let replayed = logging_buffer::snapshot_filtered(replay, &filter);

    let stream = async_stream::stream! {
        // 回放：snapshot 为最新在前，反转成旧→新下发。
        for rec in replayed.into_iter().rev() {
            yield Ok(Event::default()
                .event("log")
                .data(serde_json::to_string(&rec).unwrap_or_default()));
        }
        loop {
            match rx.recv().await {
                Ok(rec) => {
                    if filter.matches(&rec) {
                        yield Ok(Event::default()
                            .event("log")
                            .data(serde_json::to_string(&rec).unwrap_or_default()));
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    let data = serde_json::json!({ "droppedCount": n });
                    yield Ok(Event::default().event("dropped").data(data.to_string()));
                }
                Err(RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keepalive"),
    )
}

/// 解析归档文件名 `learning-backend.YYYY-MM-DD.log` → 日期；不匹配返回 None。
fn parse_archive_date(name: &str) -> Option<String> {
    let rest = name
        .strip_prefix("learning-backend.")?
        .strip_suffix(".log")?;
    if is_valid_log_date(rest) {
        Some(rest.to_string())
    } else {
        None
    }
}

/// 校验日期形如 `^\d{4}-\d{2}-\d{2}$`（路径穿越防御 + 文件名约束）。
fn is_valid_log_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// GET /api/admin/monitoring/logs/archive —— 列出每日归档文件（不含内容）。
async fn logs_archive(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    audit_raw_log_access(&state, &admin.admin_id, "list_log_archive", json!({}));
    let dir = logging_buffer::log_dir();
    let enabled = logging_buffer::file_logs_enabled();
    let files = crate::blocking::run_blocking("admin.monitoring.logs_archive", move || {
        let mut out: Vec<(String, u64)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(date) = parse_archive_date(&name) {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push((date, size));
                }
            }
        }
        out.sort_by(|a, b| b.0.cmp(&a.0)); // 日期倒序（最新在前）
        out
    })
    .await?;
    let files_json: Vec<serde_json::Value> = files
        .into_iter()
        .map(|(date, size)| json!({ "date": date, "sizeBytes": size }))
        .collect();
    Ok(ok(json!({ "files": files_json, "fileLogsEnabled": enabled })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveDateQuery {
    level: Option<String>,
    target: Option<String>,
    q: Option<String>,
    page: Option<u64>,
    per_page: Option<u64>,
}

/// GET /api/admin/monitoring/logs/archive/:date?level&target&q&page&perPage ——
/// 读取并过滤某日归档（JSON 行），分页返回（最新在前）。
async fn logs_archive_date(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(date): Path<String>,
    Query(q): Query<ArchiveDateQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !is_valid_log_date(&date) {
        return Err(AppError::bad_request("INVALID_DATE", "日期格式应为 YYYY-MM-DD"));
    }
    audit_raw_log_access(
        &state,
        &admin.admin_id,
        "read_log_archive",
        json!({ "date": date, "level": opt_clean(q.level.clone()),
                "target": opt_clean(q.target.clone()), "q": opt_clean(q.q.clone()) }),
    );
    let dir = logging_buffer::log_dir();
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(200).clamp(1, 1000);
    let filter = LogFilter {
        level: opt_clean(q.level),
        target: opt_clean(q.target),
        q: opt_clean(q.q),
        request_id: None,
        since_ms: None,
    };
    let start = ((page - 1) * per_page) as usize;
    let take = per_page as usize;
    let (logs, total) =
        crate::blocking::run_blocking("admin.monitoring.logs_archive_date", move || {
            // 路径仅由已校验日期拼成，限定在 log_dir 下，无穿越风险。
            let path = format!("{dir}/learning-backend.{date}.log");
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let mut matched: Vec<logging_buffer::LogRecord> = content
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    if line.is_empty() {
                        return None;
                    }
                    serde_json::from_str::<logging_buffer::LogRecord>(line).ok()
                })
                .filter(|r| filter.matches(r))
                .collect();
            matched.reverse(); // 最新在前
            let total = matched.len() as u64;
            let page_items: Vec<_> = matched.into_iter().skip(start).take(take).collect();
            (page_items, total)
        })
        .await?;
    Ok(ok(json!({
        "logs": logs,
        "total": total,
        "page": page,
        "perPage": per_page,
    })))
}

#[derive(Debug, Deserialize)]
struct AvailabilityHourlyQuery {
    hours: Option<i64>,
}

/// GET /api/admin/monitoring/availability-hourly?hours=720 —— 每小时可用率桶（升序）。
/// 数据源 availability_rollup（metrics_flush 持久化），availability = 1 - err5xx/count。
async fn availability_hourly(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<AvailabilityHourlyQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let hours = q.hours.unwrap_or(720).clamp(1, 720);
    let now_hour = Utc::now().timestamp() / 3600;
    let from_hour = now_hour - hours + 1;
    let rows = state
        .run_store_task("admin.monitoring.availability_hourly", move |store| {
            store.list_availability_hourly(from_hour)
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;
    let hours_json: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(hour_key, count, err5xx, bytes_in, buckets)| {
            let availability = if count > 0 {
                1.0 - err5xx as f64 / count as f64
            } else {
                1.0
            };
            json!({
                "hourKey": hour_key,
                "count": count,
                "err5xx": err5xx,
                "availability": availability,
                "bytesIn": bytes_in,
                "buckets": buckets,
            })
        })
        .collect();
    Ok(ok(json!({ "hours": hours_json })))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    hours: Option<i64>,
}

/// GET /api/admin/monitoring/events?hours=6 —— 派生告警时间线。
/// 由真实信号聚合:worker 失败(worker_last_run)、版本更新(update_cache)、
/// AMAS 决策异常(engine_monitoring_events.is_anomaly)。无独立告警表。
async fn alert_events(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<EventsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let hours = q.hours.unwrap_or(6).clamp(1, 168);
    let cutoff_dt = Utc::now() - ChronoDuration::hours(hours);
    let cutoff_unix = cutoff_dt.timestamp();
    let cutoff_rfc = cutoff_dt.to_rfc3339();

    // DB 派生:worker 最近执行 + 近窗口 AMAS 异常计数/最近时间
    let (workers, anomaly, sys_alerts) = state
        .run_store_task("admin.monitoring.events", move |store| {
            let workers = store.list_worker_last_run()?;
            // is_anomaly 列若不存在则吞错返回 (0, None),不产生异常告警
            let anomaly: (i64, Option<String>) = store
                .conn()?
                .query_row(
                    "SELECT COUNT(*), MAX(timestamp) FROM engine_monitoring_events \
                     WHERE is_anomaly = 1 AND timestamp >= ?1",
                    rusqlite::params![cutoff_rfc],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((0, None));
            // m037:AMAS 软拦截告警(失败则空 Vec,保持降级语义)
            let sys_alerts = store
                .list_recent_system_alerts(&cutoff_rfc)
                .unwrap_or_default();
            Ok::<_, crate::store::StoreError>((workers, anomaly, sys_alerts))
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or((Vec::new(), (0, None), Vec::new()));

    let mut events: Vec<serde_json::Value> = Vec::new();

    // 1) worker 失败/异常(窗口内)
    for w in &workers {
        let outcome = w.last_outcome.to_ascii_lowercase();
        let healthy = matches!(
            outcome.as_str(),
            "success" | "ok" | "completed" | "skipped" | "idle"
        ) && w.last_error.is_none();
        if !healthy && w.last_run_at >= cutoff_unix {
            events.push(json!({
                "tsMs": w.last_run_at * 1000,
                "severity": "error",
                "title": format!("worker {} {}", w.worker_name, w.last_outcome),
                "desc": w.last_error.clone()
                    .unwrap_or_else(|| format!("最近一次耗时 {} ms", w.last_duration_ms)),
            }));
        }
    }

    // 1b) 周期任务成功完成 → resolved(绿点)事件,对齐设计图「每日备份完成」类条目
    const DONE_WORKERS: &[&str] = &[
        "daily_aggregation",
        "weekly_report",
        "monitoring_retention",
        "backup_vacuum",
        "log_export",
    ];
    for w in &workers {
        let outcome = w.last_outcome.to_ascii_lowercase();
        let succeeded = matches!(outcome.as_str(), "success" | "ok" | "completed");
        if succeeded
            && w.last_error.is_none()
            && w.last_run_at >= cutoff_unix
            && DONE_WORKERS.contains(&w.worker_name.as_str())
        {
            events.push(json!({
                "tsMs": w.last_run_at * 1000,
                "severity": "resolved",
                "title": format!("{} 已完成", w.worker_name),
                "desc": format!("周期任务成功 · 耗时 {} ms", w.last_duration_ms),
            }));
        }
    }

    // 2) 版本更新可用(update_cache)
    {
        let cache = state.update_cache().read().await;
        if let Some((_, ref data)) = *cache {
            if data["hasUpdate"].as_bool().unwrap_or(false) {
                let latest = data["latestVersion"].as_str().unwrap_or("?");
                events.push(json!({
                    "tsMs": Utc::now().timestamp_millis(),
                    "severity": "info",
                    "title": format!("新版本 {latest} 可用"),
                    "desc": "GitHub release 检测到可升级版本",
                }));
            }
        }
    }

    // 3) AMAS 决策异常聚合
    if anomaly.0 > 0 {
        let ts_ms = anomaly
            .1
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_else(|| Utc::now().timestamp_millis());
        events.push(json!({
            "tsMs": ts_ms,
            "severity": "warning",
            "title": format!("AMAS 决策异常 {} 次", anomaly.0),
            "desc": format!("近 {hours}h 内 engine_monitoring_events 标记 is_anomaly"),
        }));
    }

    // 4) 系统告警(m037 AMAS 数据软拦截:worker 落库失败 / 学习记录处理失败)
    for a in &sys_alerts {
        let ts_ms = chrono::DateTime::parse_from_rfc3339(&a.last_seen_at)
            .map(|dt| dt.timestamp_millis())
            .unwrap_or_else(|_| Utc::now().timestamp_millis());
        events.push(json!({
            "tsMs": ts_ms,
            "severity": a.severity,
            "title": a.title,
            "desc": format!(
                "累计 {} 次 · 首次 {} · 最近 {}",
                a.count, a.first_seen_at, a.last_seen_at
            ),
        }));
    }

    // 最新在前,限 50 条(放宽自 30,避免新告警被 worker resolved 绿点挤掉)
    events.sort_by(|a, b| {
        b["tsMs"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["tsMs"].as_i64().unwrap_or(0))
    });
    events.truncate(50);

    Ok(ok(json!({ "events": events })))
}

// ─────────────── W1-2：outbox 死信运维端点 ───────────────

#[derive(Debug, Deserialize)]
struct DeadLetterQuery {
    limit: Option<i64>,
}

/// GET /api/admin/monitoring/dead-letter?limit=100 —— 死信明细列表（含 user/事件类型/失败原因/
/// 进死信时间），按进死信时间倒序。死信价值随 opt-in async 启用兑现，默认同步老路恒空。
async fn dead_letter_list(
    _admin: AdminAuthUser,
    Query(q): Query<DeadLetterQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let entries = state
        .run_store_task("admin.monitoring.dead_letter_list", move |store| {
            store.list_dead_letter(limit)
        })
        .await??;
    Ok(ok(json!({ "entries": entries })))
}

/// POST /api/admin/monitoring/dead-letter/:id/requeue —— 人工重投：死信原子回 outbox
/// （attempts 归零、立即可领取）。id 已不存在返回 404（并发已被他处处理）。
async fn dead_letter_requeue(
    _admin: AdminAuthUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let requeued = state
        .run_store_task("admin.monitoring.dead_letter_requeue", move |store| {
            store.requeue_dead_letter(id)
        })
        .await??;
    if requeued {
        Ok(ok(json!({ "requeued": true, "id": id })))
    } else {
        Err(AppError::not_found("死信不存在或已被处理"))
    }
}

/// DELETE /api/admin/monitoring/dead-letter/:id —— 人工丢弃一条死信。
async fn dead_letter_purge(
    _admin: AdminAuthUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let purged = state
        .run_store_task("admin.monitoring.dead_letter_purge", move |store| {
            store.purge_dead_letter(id)
        })
        .await??;
    if purged {
        Ok(ok(json!({ "purged": true, "id": id })))
    } else {
        Err(AppError::not_found("死信不存在或已被处理"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_versions() {
        assert!(!is_newer("0.1.3", "0.1.3"));
    }

    #[test]
    fn patch_increment() {
        assert!(is_newer("0.1.4", "0.1.3"));
        assert!(!is_newer("0.1.2", "0.1.3"));
    }

    #[test]
    fn minor_increment() {
        assert!(is_newer("0.2.0", "0.1.3"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn major_increment() {
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn shorter_version_string() {
        assert!(!is_newer("0.1", "0.1.3"));
        assert!(is_newer("0.2", "0.1.3"));
    }

    #[test]
    fn prerelease_suffix_ignored() {
        // filter_map 会跳过无法解析的段，"3-beta" 解析失败变成空
        assert!(!is_newer("0.1.3-beta", "0.1.3"));
    }

    #[test]
    fn parse_stub_status_extracts_fields() {
        let body = "Active connections: 291 \nserver accepts handled requests\n 16630948 16630940 31070465 \nReading: 6 Writing: 179 Waiting: 106 \n";
        let v = super::parse_stub_status(body).expect("parse ok");
        assert_eq!(v["active"], 291);
        assert_eq!(v["accepts"], 16630948u64);
        assert_eq!(v["handled"], 16630940u64);
        assert_eq!(v["requests"], 31070465u64);
        assert_eq!(v["dropped"], 8); // accepts - handled
        assert_eq!(v["writing"], 179);
        assert_eq!(v["waiting"], 106);
    }

    #[test]
    fn parse_stub_status_rejects_garbage() {
        assert!(super::parse_stub_status("not nginx output").is_none());
        assert!(super::parse_stub_status("").is_none());
    }
}
