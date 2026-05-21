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

pub(crate) async fn wordbook_center_probe(state: &AppState) -> (bool, bool) {
    let settings = state
        .run_store_task("health.wbc_settings", |store| store.get_system_settings())
        .await
        .ok()
        .and_then(Result::ok);

    let url = settings.and_then(|s| s.wordbook_center_url);
    let Some(base_url) = url else {
        return (true, true);
    };

    let client = match reqwest::Client::builder()
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

pub async fn health_check(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
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
    let amas_healthy = state.amas().is_healthy();
    let sse_healthy = sse_probe_ok(&state);
    let (wbc_healthy, wbc_probe_skipped) = wordbook_center_probe(&state).await;

    let status = if !store_healthy {
        "down"
    } else if !amas_healthy || !sse_healthy || !wbc_healthy {
        "degraded"
    } else {
        "ok"
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": status,
            "uptimeSecs": startup_instant().elapsed().as_secs(),
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
        // TODO: real error tracking not yet implemented
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
