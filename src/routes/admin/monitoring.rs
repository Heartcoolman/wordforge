use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{Duration as ChronoDuration, Utc};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::json;
use sysinfo::{Disks, Pid, ProcessRefreshKind, RefreshKind, System};

use crate::auth::AdminAuthUser;
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
        // M0-P5：监控页对齐设计图新增——滚动请求指标 / 实时日志 / 派生告警时间线
        .route("/requests", get(request_metrics))
        .route("/logs", get(recent_logs))
        .route("/events", get(alert_events))
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

    let status = if !store_probe_ok {
        "down"
    } else if !amas_healthy || !sse_healthy || !wbc_healthy {
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

    // m023:进程级 CPU/RSS + DB 池 + 工作目录磁盘。采样 ~negligible(~1ms),
    // 失败时字段返回 null,前端不渲染进度条 ——不让监控干扰本体。
    let resources = sample_resources(&state).await;

    Ok(ok(serde_json::json!({
        "status": status,
        "storeProbeOk": store_probe_ok,
        "dbSizeBytes": size_on_disk,
        "uptimeSecs": uptime_secs,
        "version": env!("GIT_VERSION"),
        "errorRate": error_rate,
        "resources": resources,
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
    let api_url = state.config().update_check.api_url.trim();

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
                        Some(b) => d.mount_point().as_os_str().len() > b.mount_point().as_os_str().len(),
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

    serde_json::json!({
        "cpuPct": cpu_pct,
        "memoryRssBytes": rss_bytes,
        "memoryTotalBytes": mem_total,
        "diskTotalBytes": disk_total,
        "diskFreeBytes": disk_free,
        "walSizeBytes": wal_size,
        "pool": pool.map(|p| serde_json::json!({
            "max": p.max,
            "connections": p.connections,
            "idle": p.idle,
        })),
    })
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsQuery {
    limit: Option<usize>,
    level: Option<String>,
}

/// GET /api/admin/monitoring/logs?limit=200&level=WARN —— 进程内日志环形缓冲快照(最新在前)。
async fn recent_logs(
    _admin: AdminAuthUser,
    Query(q): Query<LogsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let logs = crate::logging_buffer::snapshot(limit, q.level.as_deref());
    Ok(ok(json!({ "logs": logs })))
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
    let (workers, anomaly) = state
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
            Ok::<_, crate::store::StoreError>((workers, anomaly))
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or((Vec::new(), (0, None)));

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

    // 最新在前,限 30 条
    events.sort_by(|a, b| {
        b["tsMs"].as_i64().unwrap_or(0).cmp(&a["tsMs"].as_i64().unwrap_or(0))
    });
    events.truncate(30);

    Ok(ok(json!({ "events": events })))
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
}
