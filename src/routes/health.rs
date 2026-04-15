use std::sync::OnceLock;
use std::time::{Duration, Instant};

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
    let store_healthy = state.store().get_user_by_id("__health_check__").is_ok();
    let amas_healthy = true;
    let sse_healthy = true;

    let settings = state.store().get_system_settings().ok();
    let wbc_url = settings.as_ref().and_then(|s| s.wordbook_center_url.clone());

    let wbc_healthy = match &wbc_url {
        Some(url) => {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .ok();
            match client {
                Some(c) => c.head(url).send().await.is_ok(),
                None => false,
            }
        }
        None => true,
    };

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
            "wordbookCenter": { "healthy": wbc_healthy, "url": wbc_url },
        }
    }))
}

pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

pub async fn readiness(State(state): State<AppState>) -> StatusCode {
    if state.store().get_user_by_id("__health_check__").is_ok() {
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
    let healthy = state.store().get_user_by_id("__health_check__").is_ok();
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
