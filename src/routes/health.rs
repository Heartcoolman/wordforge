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

pub fn router() -> Router<AppState> {
    // Ensure startup time is recorded when the router is built
    let _ = startup_instant();

    Router::new()
        .route("/", get(health_check))
        .route("/live", get(liveness))
        .route("/ready", get(readiness))
        .route("/database", get(database_health))
        .route("/metrics", get(metrics))
}

pub async fn health_check() -> impl axum::response::IntoResponse {
    // 注意: uptimeSecs 存在低风险的信息泄露（审计建议），攻击者可推断重启时间。
    // 保留此字段用于运维监控，如有需要可通过环境变量控制是否暴露。
    Json(serde_json::json!({
        "status": "ok",
        "uptimeSecs": startup_instant().elapsed().as_secs(),
        "store": {
            "healthy": true,
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
