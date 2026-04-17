use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::response::{ok, AppError};
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
) -> Result<impl axum::response::IntoResponse, AppError> {
    let device_id = q.device_id;
    let banned = state
        .run_store_task("status.get_device_ban", move |store| {
            store.is_device_banned(&device_id)
        })
        .await??;
    Ok(ok(serde_json::json!({ "banned": banned })))
}
