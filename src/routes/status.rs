use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::response::ok;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_status))
        .route("/device-ban", get(get_device_ban))
}

async fn get_status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    ok(serde_json::json!({
        "maintenanceMode": state.is_maintenance(),
        "version": env!("GIT_VERSION"),
    }))
}

#[derive(Deserialize)]
struct DeviceBanQuery {
    #[serde(rename = "deviceId")]
    device_id: String,
}

async fn get_device_ban(
    Query(q): Query<DeviceBanQuery>,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let banned = state.store().is_device_banned(&q.device_id).unwrap_or(false);
    ok(serde_json::json!({ "banned": banned }))
}
