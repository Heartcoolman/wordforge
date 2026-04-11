use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::{AppState, SseEvent};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_clients))
        .route("/:id/ban", post(ban_client))
        .route("/:id/unban", post(unban_client))
        .route("/:id/request-telemetry", post(request_telemetry))
}

pub fn telemetry_router() -> Router<AppState> {
    Router::new().route("/:device_id", get(get_telemetry))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SseLiveEntry {
    device_id: String,
    platform: String,
    user_id: String,
    connected_secs: u64,
    connection_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentlyActiveEntry {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    last_seen_at: String,
    is_banned: bool,
}

async fn list_clients(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // SSE live connections (deduplicated by device_id)
    let sse_live: Vec<SseLiveEntry> = state
        .active_sse()
        .iter()
        .filter_map(|entry| {
            let device_id = entry.key().clone();
            let conns = entry.value();
            let first = conns.first()?;
            Some(SseLiveEntry {
                device_id,
                platform: first.platform.clone(),
                user_id: first.user_id.clone(),
                connected_secs: first.connected_at.elapsed().as_secs(),
                connection_count: conns.len(),
            })
        })
        .collect();

    // Recently active (last 15 minutes)
    let recently_active: Vec<RecentlyActiveEntry> = state
        .store()
        .get_recently_active_clients(15)?
        .into_iter()
        .map(|d| RecentlyActiveEntry {
            device_id: d.device_id,
            platform: d.platform,
            user_id: d.user_id,
            last_seen_at: d.last_seen_at,
            is_banned: d.is_banned,
        })
        .collect();

    Ok(ok(serde_json::json!({
        "sseLive": sse_live,
        "recentlyActive": recently_active,
    })))
}

#[derive(Deserialize)]
struct BanRequest {
    reason: Option<String>,
}

async fn ban_client(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    body: Option<JsonBody<BanRequest>>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !state.store().client_device_exists(&id)? {
        return Err(AppError::not_found("设备不存在"));
    }

    let reason = body.and_then(|JsonBody(b)| {
        b.reason
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take(500).collect::<String>())
    });

    state
        .store()
        .ban_client_device(&id, &admin.admin_id, reason.as_deref())?;

    // Notify then drop active SSE connections for this device
    if let Some((_, conns)) = state.active_sse().remove(&id) {
        for conn in conns {
            let _ = conn.tx.send(SseEvent::Banned);
        }
    }

    tracing::info!(admin_id = %admin.admin_id, device_id = %id, "管理员封禁设备");
    Ok(ok(serde_json::json!({ "banned": true, "deviceId": id })))
}

async fn unban_client(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !state.store().client_device_exists(&id)? {
        return Err(AppError::not_found("设备不存在"));
    }
    state.store().unban_client_device(&id)?;
    tracing::info!(admin_id = %admin.admin_id, device_id = %id, "管理员解封设备");
    Ok(ok(serde_json::json!({ "banned": false, "deviceId": id })))
}

async fn request_telemetry(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let conns = state.active_sse().get(&id).filter(|c| !c.is_empty()).ok_or_else(|| AppError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "DEVICE_OFFLINE".into(),
        message: "设备当前无活跃 SSE 连接".into(),
        is_operational: true,
    })?;

    let request_id = uuid::Uuid::new_v4().to_string();
    let event = SseEvent::TelemetryRequest {
        request_id: request_id.clone(),
    };
    for conn in conns.value() {
        let _ = conn.tx.send(event.clone());
    }

    Ok(ok(serde_json::json!({ "requestId": request_id })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn get_telemetry(
    _admin: AdminAuthUser,
    Path(device_id): Path<String>,
    Query(q): Query<TelemetryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !state.store().client_device_exists(&device_id)? {
        return Err(AppError::not_found("设备不存在"));
    }

    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (records, total) = state
        .store()
        .get_telemetry_by_device(&device_id, limit, offset)?;

    Ok(ok(serde_json::json!({ "records": records, "total": total })))
}
