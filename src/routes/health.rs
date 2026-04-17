use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::auth::AdminAuthUser;
use crate::state::AppState;

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
    let store_healthy = store_probe_ok(&state).await;
    let amas_healthy = true;
    let sse_healthy = true;

    let settings = state
        .run_store_task("health.get_system_settings", |store| {
            store.get_system_settings()
        })
        .await
        .ok()
        .and_then(Result::ok);
    let wbc_url = settings
        .as_ref()
        .and_then(|s| s.wordbook_center_url.clone());
    let wbc_probe_skipped = wbc_url.is_some();
    let wbc_healthy = true;

    let status = if !store_healthy {
        "down"
    } else if !amas_healthy || !sse_healthy || !wbc_healthy {
        "degraded"
    } else {
        "ok"
    };

    Json(serde_json::json!({
        "status": status,
        "uptimeSecs": startup_instant().elapsed().as_secs(),
        "services": {
            "store": { "healthy": store_healthy },
            "amas": { "healthy": amas_healthy },
            "sse": { "healthy": sse_healthy },
            "wordbookCenter": {
                "healthy": wbc_healthy,
                "probeSkipped": wbc_probe_skipped
            },
        }
    }))
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
