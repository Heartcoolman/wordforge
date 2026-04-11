use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(submit_telemetry))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryRequest {
    event_type: String,
    request_id: Option<String>,
    client_ts: String,
    payload: serde_json::Value,
}

async fn submit_telemetry(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<TelemetryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("MISSING_DEVICE_ID", "缺少 X-Device-Id 请求头"))?;

    if body.event_type == "on_demand" && body.request_id.is_none() {
        return Err(AppError::bad_request(
            "INVALID_TELEMETRY",
            "on_demand 事件必须携带 requestId",
        ));
    }

    // Validate non-negative values
    if let Some(obj) = body.payload.as_object() {
        for key in ["sessionDurationSecs", "errorCount"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_i64()) {
                if v < 0 {
                    return Err(AppError {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "INVALID_PAYLOAD".into(),
                        message: format!("{} 不能为负数", key),
                        is_operational: true,
                    });
                }
            }
        }
        for key in ["actionsPerMin", "avgResponseTimeMs"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_f64()) {
                if v < 0.0 {
                    return Err(AppError {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "INVALID_PAYLOAD".into(),
                        message: format!("{} 不能为负数", key),
                        is_operational: true,
                    });
                }
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(&body.payload)
        .map_err(|e| AppError::internal(&format!("payload serialization: {e}")))?;

    state.store().insert_telemetry(
        &id,
        device_id,
        &auth.user_id,
        &body.event_type,
        body.request_id.as_deref(),
        &payload_json,
        &body.client_ts,
    )?;

    Ok(ok(serde_json::json!({ "id": id })))
}
