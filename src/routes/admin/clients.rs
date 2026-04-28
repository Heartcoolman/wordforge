use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::{AppState, SseEvent};
use crate::store::operations::clients::{ClientDevice, DataChannelStatus};

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
    is_banned: bool,
    data_channels: DataChannelStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentlyActiveEntry {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    last_seen_at: String,
    is_banned: bool,
    data_channels: DataChannelStatus,
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
                device_id: device_id.clone(),
                platform: first.platform.clone(),
                user_id: first.user_id.clone(),
                connected_secs: first.connected_at.elapsed().as_secs(),
                connection_count: conns.len(),
                is_banned: false,
                data_channels: DataChannelStatus::default(),
            })
        })
        .collect();

    let live_user_ids: Vec<String> = sse_live.iter().map(|entry| entry.user_id.clone()).collect();
    let live_device_ids: Vec<String> = sse_live
        .iter()
        .map(|entry| entry.device_id.clone())
        .collect();
    let (banned_by_device, recently_active_devices, status) = state
        .run_store_task("admin.clients.list", move |store| -> Result<_, AppError> {
            let banned_by_device = live_device_ids
                .iter()
                .map(|device_id| Ok((device_id.clone(), store.is_device_banned(device_id)?)))
                .collect::<Result<std::collections::HashMap<String, bool>, crate::store::StoreError>>()?;

            let recently_active_devices =
                exclude_live_devices(store.get_recently_active_clients(15)?, &live_device_ids);
            let user_ids: Vec<String> = live_user_ids
                .into_iter()
                .chain(
                    recently_active_devices
                        .iter()
                        .filter_map(|entry| entry.user_id.clone()),
                )
                .collect();
            let device_ids: Vec<String> = live_device_ids
                .iter()
                .cloned()
                .chain(
                    recently_active_devices
                        .iter()
                        .map(|entry| entry.device_id.clone()),
                )
                .collect();
            let status = store.get_data_upload_status(&user_ids, &device_ids)?;
            Ok((banned_by_device, recently_active_devices, status))
        })
        .await??;

    let mut sse_live = sse_live;
    for entry in &mut sse_live {
        entry.is_banned = banned_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or(false);
    }

    let recently_active: Vec<RecentlyActiveEntry> = recently_active_devices
        .iter()
        .map(|d| RecentlyActiveEntry {
            device_id: d.device_id.clone(),
            platform: d.platform.clone(),
            user_id: d.user_id.clone(),
            last_seen_at: d.last_seen_at.clone(),
            is_banned: d.is_banned,
            data_channels: DataChannelStatus::default(),
        })
        .collect();

    for entry in &mut sse_live {
        let amas = status
            .amas_by_user
            .get(&entry.user_id)
            .copied()
            .unwrap_or("none");
        // AMAS exists but no events means learning is also "nil"
        let learning = status
            .learning_by_user
            .get(&entry.user_id)
            .copied()
            .unwrap_or_else(|| if amas != "none" { "nil" } else { "none" });
        let telemetry = status
            .telemetry_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or("none");
        entry.data_channels = DataChannelStatus {
            amas,
            learning,
            telemetry,
        };
    }

    let mut recently_active = recently_active;
    for entry in &mut recently_active {
        let (amas, learning) = match &entry.user_id {
            Some(uid) => {
                let a = status.amas_by_user.get(uid).copied().unwrap_or("none");
                // AMAS exists but no events means learning is also "nil"
                let l = status
                    .learning_by_user
                    .get(uid)
                    .copied()
                    .unwrap_or_else(|| if a != "none" { "nil" } else { "none" });
                (a, l)
            }
            None => ("none", "none"),
        };
        let telemetry = status
            .telemetry_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or("none");
        entry.data_channels = DataChannelStatus {
            amas,
            learning,
            telemetry,
        };
    }

    Ok(ok(serde_json::json!({
        "sseLive": sse_live,
        "recentlyActive": recently_active,
    })))
}

fn exclude_live_devices(
    devices: Vec<ClientDevice>,
    live_device_ids: &[String],
) -> Vec<ClientDevice> {
    let live: HashSet<&str> = live_device_ids.iter().map(String::as_str).collect();
    devices
        .into_iter()
        .filter(|device| !live.contains(device.device_id.as_str()))
        .collect()
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
    let reason = body.and_then(|JsonBody(b)| {
        b.reason
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take(500).collect::<String>())
    });
    let device_id_for_store = id.clone();
    let admin_id = admin.admin_id.clone();

    state
        .run_store_task("admin.clients.ban", move |store| -> Result<_, AppError> {
            if !store.client_device_exists(&device_id_for_store)? {
                return Err(AppError::not_found("设备不存在"));
            }
            store.ban_client_device(&device_id_for_store, &admin_id, reason.as_deref())?;
            Ok(())
        })
        .await??;

    // Notify via SSE but keep connection alive for instant unban
    if let Some(conns) = state.active_sse().get(&id) {
        for conn in conns.value() {
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
    let device_id_for_store = id.clone();
    state
        .run_store_task("admin.clients.unban", move |store| -> Result<_, AppError> {
            if !store.client_device_exists(&device_id_for_store)? {
                return Err(AppError::not_found("设备不存在"));
            }
            store.unban_client_device(&device_id_for_store)?;
            Ok(())
        })
        .await??;

    // Notify via existing SSE connection for instant unban
    if let Some(conns) = state.active_sse().get(&id) {
        for conn in conns.value() {
            let _ = conn.tx.send(SseEvent::Unbanned);
        }
    }

    tracing::info!(admin_id = %admin.admin_id, device_id = %id, "管理员解封设备");
    Ok(ok(serde_json::json!({ "banned": false, "deviceId": id })))
}

async fn request_telemetry(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let conns = state
        .active_sse()
        .get(&id)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| AppError {
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
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (records, total) = state
        .run_store_task(
            "admin.clients.get_telemetry",
            move |store| -> Result<_, AppError> {
                if !store.client_device_exists(&device_id)? {
                    return Err(AppError::not_found("设备不存在"));
                }

                Ok(store.get_telemetry_summaries_by_device(&device_id, limit, offset)?)
            },
        )
        .await??;

    Ok(ok(
        serde_json::json!({ "records": records, "total": total }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(device_id: &str) -> ClientDevice {
        ClientDevice {
            device_id: device_id.to_string(),
            platform: "ios".to_string(),
            user_id: Some("user-1".to_string()),
            first_seen_at: "2026-04-25 12:00:00".to_string(),
            last_seen_at: "2026-04-25 12:01:00".to_string(),
            is_banned: false,
            banned_at: None,
            banned_by: None,
            ban_reason: None,
        }
    }

    #[test]
    fn recent_clients_exclude_live_devices() {
        let devices = vec![client("live-device"), client("recent-only")];
        let live_device_ids = vec!["live-device".to_string()];

        let filtered = exclude_live_devices(devices, &live_device_ids);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].device_id, "recent-only");
    }
}
