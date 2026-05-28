use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use once_cell::sync::Lazy;
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
            ))
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or((0, Vec::new(), 0, 0, false));
    let (size, tables, page_size, page_count, wal_enabled) = size;

    Ok(ok(serde_json::json!({
        "sizeOnDisk": size,
        "tableCount": tables.len(),
        "tables": tables,
        "pageSize": page_size,
        "pageCount": page_count,
        "walEnabled": wal_enabled,
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

    // 2) r2d2 池占用
    let pool = state
        .run_store_task("admin.monitoring.pool_status", |store| {
            Ok::<_, crate::store::StoreError>(store.pool_status())
        })
        .await
        .ok()
        .and_then(Result::ok);

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

    serde_json::json!({
        "cpuPct": cpu_pct,
        "memoryRssBytes": rss_bytes,
        "diskTotalBytes": disk_total,
        "diskFreeBytes": disk_free,
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
