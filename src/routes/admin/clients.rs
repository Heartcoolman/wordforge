use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
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
        // m027:设备表后端分页 + 平台聚合 + 升级策略 CRUD + 强制升级广播
        .route("/paginated", get(list_clients_paginated))
        .route("/distribution", get(get_distribution))
        .route(
            "/upgrade-policy",
            get(list_upgrade_policy_handler),
        )
        .route(
            "/upgrade-policy/:platform",
            put(put_upgrade_policy_handler),
        )
        .route(
            "/broadcast-upgrade/:platform",
            post(broadcast_upgrade_handler),
        )
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
    /// m022:`x-app-version` 头落库后透出。SseClientInfo 不存版本,这里从 client_devices 表反查。
    app_version: Option<String>,
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
    /// m022:同上,直接来自 ClientDevice.app_version 字段。
    app_version: Option<String>,
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
                app_version: None, // 稍后在 store task 后填充
            })
        })
        .collect();

    let live_user_ids: Vec<String> = sse_live.iter().map(|entry| entry.user_id.clone()).collect();
    let live_device_ids: Vec<String> = sse_live
        .iter()
        .map(|entry| entry.device_id.clone())
        .collect();
    let (banned_by_device, recently_active_devices, status, sse_app_versions) = state
        .run_store_task("admin.clients.list", move |store| -> Result<_, AppError> {
            let banned_by_device = live_device_ids
                .iter()
                .map(|device_id| Ok((device_id.clone(), store.is_device_banned(device_id)?)))
                .collect::<Result<std::collections::HashMap<String, bool>, crate::store::StoreError>>()?;

            // m022:为 SSE live 设备查 app_version(recently_active 自带 app_version 字段)
            let sse_app_versions = store.get_app_versions_for_devices(&live_device_ids)?;

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
            Ok((banned_by_device, recently_active_devices, status, sse_app_versions))
        })
        .await??;

    let mut sse_live = sse_live;
    for entry in &mut sse_live {
        entry.is_banned = banned_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or(false);
        entry.app_version = sse_app_versions
            .get(&entry.device_id)
            .cloned()
            .unwrap_or(None);
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
            app_version: d.app_version.clone(),
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

/// m027:设备表后端分页 + 搜索 + 平台过滤。返回 paginated envelope。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaginatedClientsQuery {
    page: Option<u32>,
    per_page: Option<u32>,
    q: Option<String>,
    platform: Option<String>,
    /// 仅保留 N 分钟内活跃(也包含 banned)。None 表示不过滤(全表 + 历史)。
    recent_minutes: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListedDevice {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    app_version: Option<String>,
    country: Option<String>,
    first_seen_at: String,
    last_seen_at: String,
    is_banned: bool,
}

impl From<ClientDevice> for ListedDevice {
    fn from(d: ClientDevice) -> Self {
        Self {
            device_id: d.device_id,
            platform: d.platform,
            user_id: d.user_id,
            app_version: d.app_version,
            country: d.country,
            first_seen_at: d.first_seen_at,
            last_seen_at: d.last_seen_at,
            is_banned: d.is_banned,
            // 故意不暴露 last_ip / banned_by / ban_reason 等审计敏感字段
        }
    }
}

async fn list_clients_paginated(
    _admin: AdminAuthUser,
    Query(q): Query<PaginatedClientsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1) as i64;
    let per_page = q.per_page.unwrap_or(20).clamp(1, 200) as i64;
    let offset = (page - 1) * per_page;
    let (rows, total) = state
        .run_store_task(
            "admin.clients.list_paginated",
            move |store| -> Result<_, AppError> {
                Ok(store.list_client_devices_paginated(
                    q.q.as_deref(),
                    q.platform.as_deref(),
                    q.recent_minutes,
                    per_page,
                    offset,
                )?)
            },
        )
        .await??;
    let data: Vec<ListedDevice> = rows.into_iter().map(Into::into).collect();
    let total_pages = if total == 0 { 0 } else { (total + per_page - 1) / per_page };
    Ok(ok(serde_json::json!({
        "data": data,
        "total": total,
        "page": page,
        "perPage": per_page,
        "totalPages": total_pages,
    })))
}

/// m027:平台聚合 + 平台×版本分布 + 升级策略快照(给前端一站式渲染 hero+柱状+面板)。
async fn get_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (platforms, versions, policies) = state
        .run_store_task(
            "admin.clients.distribution",
            move |store| -> Result<_, AppError> {
                Ok((
                    store.aggregate_clients_by_platform()?,
                    store.aggregate_clients_by_platform_version()?,
                    store.list_upgrade_policies()?,
                ))
            },
        )
        .await??;

    let platforms_json: Vec<serde_json::Value> = platforms
        .into_iter()
        .map(|(platform, total, active7d, pct)| {
            serde_json::json!({
                "platform": platform,
                "total": total,
                "active7d": active7d,
                "monthOverMonthPct": (pct * 10.0).round() / 10.0,
            })
        })
        .collect();
    let versions_json: Vec<serde_json::Value> = versions
        .into_iter()
        .map(|(platform, version, count)| {
            serde_json::json!({
                "platform": platform,
                "version": version,
                "count": count,
            })
        })
        .collect();
    Ok(ok(serde_json::json!({
        "platforms": platforms_json,
        "versions": versions_json,
        "policies": policies,
    })))
}

async fn list_upgrade_policy_handler(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let policies = state
        .run_store_task(
            "admin.clients.list_upgrade_policy",
            |store| -> Result<_, AppError> { Ok(store.list_upgrade_policies()?) },
        )
        .await??;
    Ok(ok(serde_json::json!({ "policies": policies })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpgradePolicyPayload {
    min_version: Option<String>,
    suggested_version: Option<String>,
    /// 0-100 整数;超出范围 400。
    grayscale_pct: Option<i64>,
    pwa_silent_update: Option<bool>,
}

fn normalize_platform(s: &str) -> Result<&'static str, AppError> {
    match s {
        "web" => Ok("web"),
        "ios" => Ok("ios"),
        "android" => Ok("android"),
        _ => Err(AppError::bad_request(
            "INVALID_PLATFORM",
            "平台必须是 web / ios / android 之一",
        )),
    }
}

async fn put_upgrade_policy_handler(
    admin: AdminAuthUser,
    Path(platform): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpgradePolicyPayload>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let platform = normalize_platform(&platform)?.to_string();
    let pct = req.grayscale_pct.unwrap_or(0);
    if !(0..=100).contains(&pct) {
        return Err(AppError::bad_request(
            "INVALID_GRAYSCALE",
            "灰度百分比需在 0-100 之间",
        ));
    }
    let pwa_silent = req.pwa_silent_update.unwrap_or(true);
    let admin_id = admin.admin_id.clone();
    let platform_for_db = platform.clone();
    let min_for_db = req.min_version.clone();
    let sug_for_db = req.suggested_version.clone();
    let admin_for_db = admin_id.clone();
    state
        .run_store_task(
            "admin.clients.upsert_upgrade_policy",
            move |store| -> Result<_, AppError> {
                store.upsert_upgrade_policy(
                    &platform_for_db,
                    min_for_db.as_deref(),
                    sug_for_db.as_deref(),
                    pct,
                    pwa_silent,
                    &admin_for_db,
                )?;
                let _ = store.insert_admin_audit(
                    &admin_for_db,
                    "client.upgrade_policy.update",
                    Some("platform"),
                    Some(&platform_for_db),
                    Some(&serde_json::json!({
                        "minVersion": min_for_db,
                        "suggestedVersion": sug_for_db,
                        "grayscalePct": pct,
                        "pwaSilentUpdate": pwa_silent,
                    })),
                );
                Ok(())
            },
        )
        .await??;
    state.invalidate_upgrade_cache();
    tracing::info!(admin_id = %admin.admin_id, platform = %platform, "升级策略已更新");
    Ok(ok(serde_json::json!({ "ok": true, "platform": platform })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastUpgradePayload {
    /// 目标"低于该版本"的设备会被推送 SSE 强制升级事件。
    below_version: String,
    latest_version: String,
    message: Option<String>,
}

async fn broadcast_upgrade_handler(
    admin: AdminAuthUser,
    Path(platform): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastUpgradePayload>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let platform = normalize_platform(&platform)?;
    let below = req.below_version.trim().to_string();
    let latest = req.latest_version.trim().to_string();
    if below.is_empty() || latest.is_empty() {
        return Err(AppError::bad_request(
            "INVALID_VERSION",
            "belowVersion 与 latestVersion 不能为空",
        ));
    }

    // 找全平台 + version < below 的设备 id list
    let platform_owned = platform.to_string();
    let below_for_db = below.clone();
    let targets: Vec<String> = state
        .run_store_task(
            "admin.clients.broadcast_upgrade.list",
            move |store| -> Result<_, AppError> {
                let (rows, _) = store.list_client_devices_paginated(
                    None,
                    Some(&platform_owned),
                    None,
                    10_000,
                    0,
                )?;
                Ok(rows
                    .into_iter()
                    .filter(|d| {
                        d.app_version
                            .as_deref()
                            .map(|v| {
                                let a = v.trim_start_matches('v');
                                let t = below_for_db.trim_start_matches('v');
                                match (
                                    semver::Version::parse(a),
                                    semver::Version::parse(t),
                                ) {
                                    (Ok(av), Ok(tv)) => av < tv,
                                    _ => false,
                                }
                            })
                            .unwrap_or(false)
                    })
                    .map(|d| d.device_id)
                    .collect())
            },
        )
        .await??;

    let mut hit = 0usize;
    for device_id in &targets {
        if let Some(conns) = state.active_sse().get(device_id) {
            let event = SseEvent::UpgradeRequired {
                latest_version: latest.clone(),
                message: req.message.clone(),
            };
            for conn in conns.value() {
                if conn.tx.send(event.clone()).is_ok() {
                    hit += 1;
                }
            }
        }
    }

    let admin_id = admin.admin_id.clone();
    let platform_str = platform.to_string();
    let targets_len = targets.len() as i64;
    let _ = state
        .run_store_task(
            "admin.clients.broadcast_upgrade.audit",
            move |store| -> Result<(), AppError> {
                store.insert_admin_audit(
                    &admin_id,
                    "client.broadcast_upgrade",
                    Some("platform"),
                    Some(&platform_str),
                    Some(&serde_json::json!({
                        "belowVersion": below,
                        "latestVersion": latest,
                        "matched": targets_len,
                        "pushedConnections": hit,
                    })),
                )?;
                Ok(())
            },
        )
        .await;

    tracing::info!(
        admin_id = %admin.admin_id, platform = %platform,
        matched = targets.len(), pushed_connections = hit,
        "强制升级广播已派发"
    );
    Ok(ok(serde_json::json!({
        "matched": targets.len(),
        "pushedConnections": hit,
    })))
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
            app_version: None,
            country: None,
            last_ip: None,
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
